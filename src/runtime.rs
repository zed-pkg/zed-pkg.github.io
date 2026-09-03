use std::{marker::PhantomData, sync::Arc, time::Duration};

use futures_util::StreamExt;
use tokio::{
    sync::{watch, Mutex, RwLock},
    task::JoinError,
    time::{interval_at, Instant, MissedTickBehavior},
};
use tracing::{debug, info, warn};

use crate::{
    ApplyOutcome, CacheBackend, CacheEvent, CachePolicy, CacheState, LocalLru, Result,
    SnapshotApplyOutcome,
};

pub const MAX_RECONCILE_INTERVAL: Duration = Duration::from_secs(180);
const MIN_RECONNECT_BACKOFF: Duration = Duration::from_secs(1);
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub capacity: usize,
    pub poll_interval: Duration,
}

impl RuntimeConfig {
    pub fn for_policy<P: CachePolicy>() -> Self {
        Self {
            capacity: P::DEFAULT_CAPACITY,
            poll_interval: P::POLL_INTERVAL,
        }
    }
}

pub struct SyncRuntime<P: CachePolicy, B: CacheBackend> {
    cache: Arc<RwLock<LocalLru<P>>>,
    backend: Arc<B>,
    config: RuntimeConfig,
    state_tx: watch::Sender<CacheState>,
    coordination: Mutex<()>,
    _policy: PhantomData<P>,
}

impl<P: CachePolicy, B: CacheBackend> SyncRuntime<P, B> {
    pub fn new(
        namespace: impl Into<String>,
        backend: Arc<B>,
        config: RuntimeConfig,
    ) -> Result<Self> {
        if P::backend_sync_mode().reads_backend()
            && (config.poll_interval.is_zero() || config.poll_interval > MAX_RECONCILE_INTERVAL)
        {
            return Err(crate::Error::InvalidPollInterval {
                actual: config.poll_interval,
                max: MAX_RECONCILE_INTERVAL,
            });
        }
        let cache = LocalLru::<P>::new(namespace, config.capacity)?;
        let (state_tx, _) = watch::channel(cache.state());
        Ok(Self {
            cache: Arc::new(RwLock::new(cache)),
            backend,
            config,
            state_tx,
            coordination: Mutex::new(()),
            _policy: PhantomData,
        })
    }

    pub fn cache(&self) -> Arc<RwLock<LocalLru<P>>> {
        Arc::clone(&self.cache)
    }

    pub fn subscribe_state(&self) -> watch::Receiver<CacheState> {
        self.state_tx.subscribe()
    }

    /// Reads and installs one authoritative snapshot while excluding event reduction.
    pub async fn reconcile_once(&self) -> Result<CacheState> {
        let _coordination = self.coordination.lock().await;
        let keyspace = { self.cache.read().await.keyspace().clone() };
        let snapshot = self.backend.read_snapshot(&keyspace).await?;
        let (outcome, state) = {
            let mut cache = self.cache.write().await;
            let outcome = cache.try_replace_from_snapshot(snapshot)?;
            (outcome, cache.state())
        };
        let _ = self.state_tx.send(state.clone());
        match outcome {
            SnapshotApplyOutcome::Applied { .. } => info!(
                namespace = keyspace.namespace(),
                cache = keyspace.cache(),
                revision = state.revision,
                entries = state.entry_count,
                "cache backend reconciliation complete"
            ),
            SnapshotApplyOutcome::StaleIgnored { current, incoming } => debug!(
                namespace = keyspace.namespace(),
                cache = keyspace.cache(),
                current,
                incoming,
                "ignored stale cache backend snapshot"
            ),
            SnapshotApplyOutcome::InvalidRejected { .. } => {
                unreachable!("validated snapshot application cannot return InvalidRejected")
            }
        }
        Ok(state)
    }

    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        if shutdown_requested(&shutdown) {
            return Ok(());
        }

        if !P::backend_sync_mode().reads_backend() {
            let state = {
                let mut cache = self.cache.write().await;
                cache.mark_ready_without_backend();
                cache.state()
            };
            let _ = self.state_tx.send(state);
            wait_for_shutdown(&mut shutdown).await;
            return Ok(());
        }

        if let Err(error) = self.reconcile_once().await {
            if P::FAIL_OPEN_ON_STARTUP {
                warn!(%error, "initial cache backend reconciliation failed; policy permits fail-open");
                let state = {
                    let mut cache = self.cache.write().await;
                    cache.mark_ready_without_backend();
                    cache.state()
                };
                let _ = self.state_tx.send(state);
            } else {
                return Err(error);
            }
        }

        let poller = Arc::clone(&self);
        let poll_shutdown = shutdown.clone();
        let mut poll_task = tokio::spawn(async move { poller.run_poller(poll_shutdown).await });

        let subscriber = Arc::clone(&self);
        let mut subscriber_task =
            tokio::spawn(async move { subscriber.run_subscriber(shutdown).await });

        tokio::select! {
            result = &mut poll_task => {
                subscriber_task.abort();
                let _ = subscriber_task.await;
                flatten_worker(result)
            }
            result = &mut subscriber_task => {
                poll_task.abort();
                let _ = poll_task.await;
                flatten_worker(result)
            }
        }
    }

    async fn run_poller(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut ticker = interval_at(
            Instant::now() + self.config.poll_interval,
            self.config.poll_interval,
        );
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(error) = self.reconcile_once().await {
                        warn!(%error, "periodic cache backend reconciliation failed");
                        self.publish_stale_state().await;
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || shutdown_requested(&shutdown) {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn run_subscriber(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let keyspace = { self.cache.read().await.keyspace().clone() };
        let mut retry_delay = MIN_RECONNECT_BACKOFF;
        loop {
            if shutdown_requested(&shutdown) {
                return Ok(());
            }

            let mut stream = match self.backend.subscribe(&keyspace).await {
                Ok(stream) => stream,
                Err(error) => {
                    warn!(%error, ?retry_delay, "cache backend subscription failed");
                    self.publish_stale_state().await;
                    if let Err(reconcile_error) = self.reconcile_once().await {
                        warn!(%reconcile_error, "reconciliation after subscription failure failed");
                        self.publish_stale_state().await;
                    }
                    if sleep_or_shutdown(retry_delay, &mut shutdown).await {
                        return Ok(());
                    }
                    retry_delay = next_backoff(retry_delay);
                    continue;
                }
            };

            // Close the gap between the previous snapshot and successful subscription. Events
            // arriving while this authoritative read runs remain queued by the backend stream.
            if let Err(error) = self.reconcile_once().await {
                warn!(%error, "post-subscribe cache backend reconciliation failed");
                self.publish_stale_state().await;
                if sleep_or_shutdown(retry_delay, &mut shutdown).await {
                    return Ok(());
                }
                retry_delay = next_backoff(retry_delay);
                continue;
            }
            retry_delay = MIN_RECONNECT_BACKOFF;

            loop {
                tokio::select! {
                    next = stream.next() => {
                        let Some(next) = next else {
                            warn!("cache backend event stream ended; reconnecting");
                            self.publish_stale_state().await;
                            break;
                        };
                        match next {
                            Ok(event) => {
                                match self.apply_event_serialized(event).await {
                                    Ok(ApplyOutcome::ReconcileRequested { .. }) => {
                                        self.reconcile_once().await?;
                                    }
                                    Ok(_) => {}
                                    Err(error) if error.event_requires_reconcile() => {
                                        warn!(%error, "revision gap detected; forcing reconciliation");
                                        self.reconcile_once().await?;
                                    }
                                    Err(error) => warn!(%error, "discarding invalid cache backend event"),
                                }
                            }
                            Err(error) => {
                                warn!(%error, "cache backend event stream error; reconnecting");
                                self.publish_stale_state().await;
                                break;
                            }
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || shutdown_requested(&shutdown) {
                            return Ok(());
                        }
                    }
                }
            }

            if let Err(error) = self.reconcile_once().await {
                warn!(%error, "reconciliation before backend reconnect failed");
                self.publish_stale_state().await;
            }
            if sleep_or_shutdown(retry_delay, &mut shutdown).await {
                return Ok(());
            }
            retry_delay = next_backoff(retry_delay);
        }
    }

    async fn apply_event_serialized(&self, event: CacheEvent) -> Result<ApplyOutcome> {
        let _coordination = self.coordination.lock().await;
        let (outcome, state) = {
            let mut cache = self.cache.write().await;
            let outcome = cache.apply_event(event);
            (outcome, cache.state())
        };
        let _ = self.state_tx.send(state);
        outcome
    }

    async fn publish_stale_state(&self) {
        let _coordination = self.coordination.lock().await;
        let state = {
            let mut cache = self.cache.write().await;
            cache.mark_stale();
            cache.state()
        };
        let _ = self.state_tx.send(state);
    }
}

fn flatten_worker(result: std::result::Result<Result<()>, JoinError>) -> Result<()> {
    result.map_err(|_| crate::Error::WorkerStopped)?
}

fn shutdown_requested(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow()
}

fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_RECONNECT_BACKOFF)
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    while !shutdown_requested(shutdown) {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}

async fn sleep_or_shutdown(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    if shutdown_requested(shutdown) {
        return true;
    }
    tokio::select! {
        () = tokio::time::sleep(delay) => false,
        changed = shutdown.changed() => changed.is_err() || shutdown_requested(shutdown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendSyncMode, CacheEvent, CacheSnapshot, EventStream, Keyspace, Mutation};
    use async_trait::async_trait;
    use futures_util::stream;
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex as StdMutex,
        },
    };

    struct TestPolicy;
    impl CachePolicy for TestPolicy {
        const CACHE_NAME: &'static str = "runtime-env";
        const REDIS_SYNC: BackendSyncMode = BackendSyncMode::ReadOnly;
    }

    struct MemoryBackend {
        snapshot: StdMutex<CacheSnapshot>,
        read_calls: AtomicUsize,
    }

    #[async_trait]
    impl CacheBackend for MemoryBackend {
        async fn read_snapshot(&self, _: &Keyspace) -> Result<CacheSnapshot> {
            self.read_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn mutate(&self, _: &Keyspace, _: Mutation, _: &str) -> Result<CacheEvent> {
            unreachable!()
        }

        async fn subscribe(&self, _: &Keyspace) -> Result<EventStream> {
            Ok(Box::pin(stream::pending()))
        }
    }

    fn memory_backend(snapshot: CacheSnapshot) -> Arc<MemoryBackend> {
        Arc::new(MemoryBackend {
            snapshot: StdMutex::new(snapshot),
            read_calls: AtomicUsize::new(0),
        })
    }

    #[tokio::test]
    async fn reconciliation_replaces_local_state() {
        let backend = memory_backend(CacheSnapshot {
            revision: 7,
            entries: BTreeMap::from([("A".to_owned(), "1".to_owned())]),
        });
        let runtime = SyncRuntime::<TestPolicy, _>::new(
            "svc",
            backend,
            RuntimeConfig::for_policy::<TestPolicy>(),
        )
        .unwrap();
        let state = runtime.reconcile_once().await.unwrap();
        assert_eq!(state.revision, 7);
        assert_eq!(runtime.cache.write().await.get("A").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn older_reconciliation_cannot_regress_local_state() {
        let backend = memory_backend(CacheSnapshot {
            revision: 4,
            entries: BTreeMap::from([("A".to_owned(), "stale".to_owned())]),
        });
        let runtime = SyncRuntime::<TestPolicy, _>::new(
            "svc",
            backend,
            RuntimeConfig::for_policy::<TestPolicy>(),
        )
        .unwrap();
        runtime
            .cache
            .write()
            .await
            .try_replace_from_snapshot(CacheSnapshot {
                revision: 5,
                entries: BTreeMap::from([("A".to_owned(), "current".to_owned())]),
            })
            .unwrap();

        let state = runtime.reconcile_once().await.unwrap();
        assert_eq!(state.revision, 5);
        assert_eq!(
            runtime.cache.write().await.get("A").as_deref(),
            Some("current")
        );
    }

    #[test]
    fn rejects_repair_intervals_outside_the_three_minute_bound() {
        let backend = memory_backend(CacheSnapshot::default());
        for poll_interval in [
            Duration::ZERO,
            MAX_RECONCILE_INTERVAL + Duration::from_secs(1),
        ] {
            let result = SyncRuntime::<TestPolicy, _>::new(
                "svc",
                Arc::clone(&backend),
                RuntimeConfig {
                    capacity: 8,
                    poll_interval,
                },
            );
            assert!(matches!(
                result,
                Err(crate::Error::InvalidPollInterval { .. })
            ));
        }
    }

    #[tokio::test]
    async fn already_requested_shutdown_performs_no_backend_io() {
        let backend = memory_backend(CacheSnapshot::default());
        let runtime = Arc::new(
            SyncRuntime::<TestPolicy, _>::new(
                "svc",
                Arc::clone(&backend),
                RuntimeConfig::for_policy::<TestPolicy>(),
            )
            .unwrap(),
        );
        let (_shutdown, shutdown_rx) = watch::channel(true);

        runtime.run(shutdown_rx).await.unwrap();
        assert_eq!(backend.read_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reconnect_backoff_is_bounded() {
        assert_eq!(next_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(20)), MAX_RECONNECT_BACKOFF);
        assert_eq!(next_backoff(MAX_RECONNECT_BACKOFF), MAX_RECONNECT_BACKOFF);
    }
}
