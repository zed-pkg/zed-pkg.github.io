use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use ores_lru_redis::{
    is_sensitive_runtime_env_key, BackendSyncMode, CachePolicy, CacheState, LocalRuntime,
    RedisStore, RuntimeConfig, SyncRuntime,
};
use tokio::sync::watch;

use crate::{BoxError, ServiceContract};

type RedisRuntime = SyncRuntime<ProcessPolicy, RedisStore>;
type LocalRuntimeType = LocalRuntime<ProcessPolicy>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PolicySpec {
    mutable: BTreeSet<&'static str>,
    secrets: BTreeSet<&'static str>,
}

static POLICY: OnceLock<PolicySpec> = OnceLock::new();

struct ProcessPolicy;

impl CachePolicy for ProcessPolicy {
    const CACHE_NAME: &'static str = "runtime-env";
    const REDIS_SYNC: BackendSyncMode = BackendSyncMode::ReadOnly;
    const DEFAULT_CAPACITY: usize = 64;
    const POLL_INTERVAL: Duration = Duration::from_secs(180);
    const FAIL_OPEN_ON_STARTUP: bool = false;

    fn allow_key(key: &str) -> bool {
        POLICY.get().is_some_and(|policy| {
            policy.mutable.contains(key)
                && !policy.secrets.contains(key)
                && !is_sensitive_runtime_env_key(key)
        })
    }

    fn redact_key(key: &str) -> bool {
        POLICY
            .get()
            .is_some_and(|policy| policy.secrets.contains(key))
            || is_sensitive_runtime_env_key(key)
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeEnvConfig {
    pub redis_url: Option<String>,
    pub redis_required: bool,
    pub namespace: String,
    pub capacity: usize,
    pub repair_interval: Duration,
    pub baseline: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheMode {
    LocalOnly,
    Redis,
}

impl CacheMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::Redis => "redis",
        }
    }
}

enum RuntimeInner {
    Redis(Arc<RedisRuntime>),
    Local(Arc<LocalRuntimeType>),
}

struct RuntimeState {
    inner: RuntimeInner,
    mode: CacheMode,
    baseline: BTreeMap<String, String>,
    shutdown: watch::Sender<bool>,
    worker_failed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct RuntimeEnv(Arc<RuntimeState>);

impl RuntimeEnv {
    /// Initialize local or Redis-synchronized runtime configuration before readiness.
    ///
    /// # Errors
    ///
    /// Returns an error when the policy conflicts with a prior initialization,
    /// required Redis configuration is absent, the backend cannot connect or
    /// reconcile, or the cache/runtime configuration is invalid.
    pub async fn start(
        config: &RuntimeEnvConfig,
        contract: ServiceContract,
    ) -> Result<Self, BoxError> {
        install_policy(contract)?;
        if config.redis_required && config.redis_url.is_none() {
            return Err(invalid("REDIS_URL is required for the runtime cache").into());
        }

        let runtime_config = RuntimeConfig {
            capacity: config.capacity,
            poll_interval: config.repair_interval,
        };
        let (shutdown, shutdown_rx) = watch::channel(false);
        let worker_failed = Arc::new(AtomicBool::new(false));

        let (inner, mode) = if let Some(redis_url) = config.redis_url.as_deref() {
            let backend = Arc::new(RedisStore::connect(redis_url).await?);
            let runtime = Arc::new(RedisRuntime::new(
                config.namespace.clone(),
                backend,
                runtime_config,
            )?);
            runtime.reconcile_once().await?;
            spawn_worker(
                Arc::clone(&runtime),
                shutdown_rx,
                Arc::clone(&worker_failed),
            );
            (RuntimeInner::Redis(runtime), CacheMode::Redis)
        } else {
            let runtime = Arc::new(LocalRuntimeType::local_only(
                config.namespace.clone(),
                runtime_config,
            )?);
            let cache = runtime.cache();
            cache.write().await.mark_ready_without_backend();
            spawn_worker(
                Arc::clone(&runtime),
                shutdown_rx,
                Arc::clone(&worker_failed),
            );
            (RuntimeInner::Local(runtime), CacheMode::LocalOnly)
        };

        Ok(Self(Arc::new(RuntimeState {
            inner,
            mode,
            baseline: config.baseline.clone(),
            shutdown,
            worker_failed,
        })))
    }

    pub const fn protocol() -> &'static str {
        ores_lru_redis::PROTOCOL
    }

    pub fn mode(&self) -> CacheMode {
        self.0.mode
    }

    pub fn worker_failed(&self) -> bool {
        self.0.worker_failed.load(Ordering::Acquire)
    }

    pub async fn state(&self) -> CacheState {
        match &self.0.inner {
            RuntimeInner::Redis(runtime) => {
                let cache = runtime.cache();
                let guard = cache.read().await;
                let state = guard.state();
                drop(guard);
                state
            }
            RuntimeInner::Local(runtime) => {
                let cache = runtime.cache();
                let guard = cache.read().await;
                let state = guard.state();
                drop(guard);
                state
            }
        }
    }

    pub async fn value(&self, key: &str) -> Option<String> {
        if !ProcessPolicy::allow_key(key) {
            return None;
        }
        let cached = match &self.0.inner {
            RuntimeInner::Redis(runtime) => {
                let cache = runtime.cache();
                let mut guard = cache.write().await;
                let value = guard.get(key);
                drop(guard);
                value
            }
            RuntimeInner::Local(runtime) => {
                let cache = runtime.cache();
                let mut guard = cache.write().await;
                let value = guard.get(key);
                drop(guard);
                value
            }
        };
        cached.or_else(|| self.0.baseline.get(key).cloned())
    }

    pub async fn maintenance_mode(&self) -> bool {
        match self.value("MAINTENANCE_MODE").await.as_deref() {
            None | Some("") | Some("0" | "false" | "no" | "off") => false,
            Some("1" | "true" | "yes" | "on") => true,
            Some(_) => true,
        }
    }

    pub async fn banner_configured(&self) -> bool {
        self.value("SERVICE_BANNER")
            .await
            .is_some_and(|value| !value.trim().is_empty())
    }

    pub fn shutdown(&self) {
        let _ = self.0.shutdown.send(true);
    }
}

fn spawn_worker<P, B>(
    runtime: Arc<SyncRuntime<P, B>>,
    shutdown: watch::Receiver<bool>,
    worker_failed: Arc<AtomicBool>,
) where
    P: CachePolicy,
    B: ores_lru_redis::CacheBackend,
{
    drop(tokio::spawn(async move {
        if let Err(error) = runtime.run(shutdown).await {
            worker_failed.store(true, Ordering::Release);
            tracing::error!(%error, "runtime cache worker stopped");
        }
    }));
}

fn install_policy(contract: ServiceContract) -> Result<(), BoxError> {
    let incoming = PolicySpec {
        mutable: contract.mutable_keys.iter().copied().collect(),
        secrets: contract.secret_keys.iter().copied().collect(),
    };
    if let Some(existing) = POLICY.get() {
        if existing == &incoming {
            return Ok(());
        }
        return Err(invalid("runtime cache policy was already initialized differently").into());
    }
    POLICY
        .set(incoming)
        .map_err(|_| invalid("runtime cache policy initialization raced"))?;
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn flags() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            ("maintenance-mode", "MAINTENANCE_MODE"),
            ("service-banner", "SERVICE_BANNER"),
        ])
    }

    fn config_map() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            ("MAINTENANCE_MODE", "runtime.maintenance"),
            ("SERVICE_BANNER", "runtime.banner"),
        ])
    }

    fn contract() -> ServiceContract {
        ServiceContract {
            service: "runtime-server-test",
            title: "Runtime Server Test",
            private_listener: false,
            mutable_keys: &["MAINTENANCE_MODE", "SERVICE_BANNER"],
            secret_keys: &["REDIS_URL"],
            flag_to_env: flags,
            env_to_config: config_map,
        }
    }

    #[tokio::test]
    async fn local_runtime_is_ready_without_backend_io() {
        let runtime = RuntimeEnv::start(
            &RuntimeEnvConfig {
                redis_url: None,
                redis_required: false,
                namespace: "runtime-server-test".to_owned(),
                capacity: 8,
                repair_interval: Duration::from_secs(180),
                baseline: BTreeMap::from([
                    ("MAINTENANCE_MODE".to_owned(), "false".to_owned()),
                    ("SERVICE_BANNER".to_owned(), "ready".to_owned()),
                ]),
            },
            contract(),
        )
        .await
        .expect("local runtime should start");
        let state = runtime.state().await;
        assert!(state.ready);
        assert!(!state.stale);
        assert!(!runtime.worker_failed());
        assert!(!runtime.maintenance_mode().await);
        assert!(runtime.banner_configured().await);
        runtime.shutdown();
    }
}
