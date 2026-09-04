use std::time::Duration;

use crate::protocol::CacheEvent;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("namespace and cache names must be 1..=64 ASCII letters, digits, '.', '_' or '-'")]
    InvalidKeyspace,
    #[error("cache capacity must be greater than zero")]
    InvalidCapacity,
    #[error("backend-reading caches must reconcile at a positive interval no greater than {max:?}; received {actual:?}")]
    InvalidPollInterval { actual: Duration, max: Duration },
    #[error("cache backend is disabled for this runtime")]
    BackendDisabled,
    #[error("received protocol {actual}; expected {expected}")]
    ProtocolMismatch {
        expected: &'static str,
        actual: String,
    },
    #[error("event targets {actual_namespace}/{actual_cache}, expected {expected_namespace}/{expected_cache}")]
    TargetMismatch {
        expected_namespace: String,
        expected_cache: String,
        actual_namespace: String,
        actual_cache: String,
    },
    #[error("revision gap: local={local}, incoming={incoming}")]
    RevisionGap { local: u64, incoming: u64 },
    #[error("revision {revision} is outside the cross-runtime exact integer range 1..={max}")]
    RevisionOutOfRange { revision: u64, max: u64 },
    #[error("event payload is invalid for operation {operation:?}")]
    InvalidEventPayload {
        operation: crate::protocol::CacheOperation,
    },
    #[error("invalid cache event or snapshot: {0}")]
    InvalidEvent(&'static str),
    #[error("invalid rate-limit block: {0}")]
    InvalidRateLimitBlock(&'static str),
    #[error("rate-limit cache keys must be 64 lowercase hexadecimal characters")]
    InvalidRateLimitPrincipalDigest,
    #[error("rate-limit block expired at {expires_at_unix_ms}; observed at {observed_at_unix_ms}")]
    ExpiredRateLimitBlock {
        expires_at_unix_ms: u64,
        observed_at_unix_ms: u64,
    },
    #[error("rate-limit cache capacity {requested} exceeds hard maximum {max}")]
    RateLimitCacheCapacityExceeded { requested: usize, max: usize },
    #[error("{kind} size/count {actual} exceeds limit {max}")]
    PayloadLimitExceeded {
        kind: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("snapshot contains {entries} allowed entries but local capacity is {capacity}")]
    SnapshotExceedsCapacity { entries: usize, capacity: usize },
    #[error("Redis error: {0}")]
    #[cfg(feature = "redis-backend")]
    Redis(#[from] redis::RedisError),
    #[error("event JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("background task ended unexpectedly")]
    WorkerStopped,
}

impl Error {
    pub fn event_requires_reconcile(&self) -> bool {
        matches!(
            self,
            Self::RevisionGap { .. }
                | Self::InvalidRateLimitBlock(_)
                | Self::InvalidRateLimitPrincipalDigest
                | Self::ExpiredRateLimitBlock { .. }
        )
    }

    pub fn event_context<'a>(&self, event: &'a CacheEvent) -> (&'a str, &'a str, u64) {
        (&event.namespace, &event.cache, event.revision)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
