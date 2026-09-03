use std::{marker::PhantomData, time::Duration};

const SENSITIVE_RUNTIME_ENV_KEY_PARTS: [&str; 6] = [
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PRIVATE_KEY",
    "DATABASE_URL",
    "CREDENTIAL",
];

/// Returns true when a key name is likely to identify a secret-bearing runtime value.
///
/// Matching is case-insensitive and deliberately conservative. Runtime environment
/// caches must never ingest these keys from a shared backend, even when diagnostics
/// would redact the corresponding values.
pub fn is_sensitive_runtime_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SENSITIVE_RUNTIME_ENV_KEY_PARTS
        .iter()
        .any(|part| upper.contains(part))
}

/// Default allowlist predicate for runtime environment caches.
pub fn is_runtime_env_key_allowed(key: &str) -> bool {
    !key.is_empty() && !is_sensitive_runtime_env_key(key)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendSyncMode {
    Disabled,
    ReadOnly,
    WriteThrough,
    Bidirectional,
}

impl BackendSyncMode {
    pub const fn reads_backend(self) -> bool {
        matches!(self, Self::ReadOnly | Self::Bidirectional)
    }

    pub const fn writes_backend(self) -> bool {
        matches!(self, Self::WriteThrough | Self::Bidirectional)
    }

    /// Compatibility helper for callers using the original Redis-specific API.
    pub const fn reads_redis(self) -> bool {
        self.reads_backend()
    }

    /// Compatibility helper for callers using the original Redis-specific API.
    pub const fn writes_redis(self) -> bool {
        self.writes_backend()
    }
}

/// Backward-compatible type name retained for the `ores.lru-redis.v1` adapter.
pub type RedisSyncMode = BackendSyncMode;

pub trait CachePolicy: Send + Sync + 'static {
    const CACHE_NAME: &'static str;

    /// Compatibility boundary for existing consumers. New backend-neutral code
    /// should call [`Self::backend_sync_mode`] rather than naming Redis directly.
    const REDIS_SYNC: BackendSyncMode;

    const DEFAULT_CAPACITY: usize = 1_024;
    const POLL_INTERVAL: Duration = Duration::from_secs(180);
    const FAIL_OPEN_ON_STARTUP: bool = false;

    fn backend_sync_mode() -> BackendSyncMode {
        Self::REDIS_SYNC
    }

    fn allow_key(key: &str) -> bool {
        !key.is_empty()
    }

    fn redact_key(key: &str) -> bool {
        is_sensitive_runtime_env_key(key)
    }
}

/// Policy adapter that preserves cache naming and filtering while disabling all
/// external backend I/O. Pair it with [`crate::DisabledBackend`] for local-only use.
pub struct LocalOnly<P: CachePolicy>(PhantomData<P>);

impl<P: CachePolicy> CachePolicy for LocalOnly<P> {
    const CACHE_NAME: &'static str = P::CACHE_NAME;
    const REDIS_SYNC: BackendSyncMode = BackendSyncMode::Disabled;
    const DEFAULT_CAPACITY: usize = P::DEFAULT_CAPACITY;
    const POLL_INTERVAL: Duration = P::POLL_INTERVAL;
    const FAIL_OPEN_ON_STARTUP: bool = P::FAIL_OPEN_ON_STARTUP;

    fn allow_key(key: &str) -> bool {
        P::allow_key(key)
    }

    fn redact_key(key: &str) -> bool {
        P::redact_key(key)
    }
}

pub struct RuntimeEnvPolicy;

impl CachePolicy for RuntimeEnvPolicy {
    const CACHE_NAME: &'static str = "runtime-env";
    const REDIS_SYNC: BackendSyncMode = BackendSyncMode::ReadOnly;

    fn allow_key(key: &str) -> bool {
        is_runtime_env_key_allowed(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_env_policy_rejects_secret_bearing_key_names() {
        for key in [
            "APP_SECRET",
            "api_token",
            "DATABASE_PASSWORD",
            "TLS_PRIVATE_KEY_PEM",
            "DATABASE_URL",
            "aws_credential_process",
        ] {
            assert!(
                is_sensitive_runtime_env_key(key),
                "{key} was not classified"
            );
            assert!(!RuntimeEnvPolicy::allow_key(key), "{key} was allowed");
            assert!(RuntimeEnvPolicy::redact_key(key), "{key} was not redacted");
        }
    }

    #[test]
    fn runtime_env_policy_allows_non_secret_configuration_names() {
        for key in ["FEATURE_ENABLED", "HTTP_PORT", "CACHE_TTL_SECONDS"] {
            assert!(RuntimeEnvPolicy::allow_key(key), "{key} was rejected");
            assert!(!RuntimeEnvPolicy::redact_key(key), "{key} was redacted");
        }
    }

    #[test]
    fn local_only_preserves_policy_contract_but_disables_backend_io() {
        assert_eq!(
            LocalOnly::<RuntimeEnvPolicy>::backend_sync_mode(),
            BackendSyncMode::Disabled
        );
        assert_eq!(
            LocalOnly::<RuntimeEnvPolicy>::CACHE_NAME,
            RuntimeEnvPolicy::CACHE_NAME
        );
        assert!(LocalOnly::<RuntimeEnvPolicy>::allow_key("HTTP_PORT"));
        assert!(!LocalOnly::<RuntimeEnvPolicy>::allow_key("APP_SECRET"));
    }
}
