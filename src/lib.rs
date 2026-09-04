#![forbid(unsafe_code)]

mod cache;
mod error;
mod keyspace;
mod local;
mod policy;
mod protocol;
mod rate_limit;
#[cfg(feature = "redis-backend")]
mod redis_store;
mod runtime;
mod store;

pub use cache::{ApplyOutcome, CacheState, LocalLru, SnapshotApplyOutcome};
pub use error::{Error, Result};
pub use keyspace::{Keyspace, KeyspaceLayout, MAX_KEYSPACE_SEGMENT_BYTES};
pub use local::LocalRuntime;
pub use policy::{
    encode_rate_limit_deny_marker, is_rate_limit_deny_key_allowed, is_runtime_env_key_allowed,
    is_sensitive_runtime_env_key, parse_rate_limit_deny_marker, BackendSyncMode, CachePolicy,
    LocalOnly, RateLimitDenyPolicy, RedisSyncMode, RuntimeEnvPolicy,
    RATE_LIMIT_DENY_CACHE_CAPACITY, RATE_LIMIT_DENY_KEY_PREFIX, RATE_LIMIT_DENY_RECONCILE_INTERVAL,
    RATE_LIMIT_DENY_VALUE_PREFIX,
};
pub use protocol::{
    CacheEvent, CacheOperation, CacheSnapshot, MAX_EVENT_BYTES, MAX_KEY_BYTES, MAX_MUTATION_BYTES,
    MAX_MUTATION_ITEMS, MAX_SAFE_REVISION, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ENTRIES,
    MAX_SOURCE_BYTES, MAX_TIMESTAMP_BYTES, MAX_VALUE_BYTES, PROTOCOL,
};
pub use rate_limit::{
    is_opaque_principal_digest, RateLimitBlock, RateLimitBlockCache, RateLimitBlockLookup,
    RateLimitBlockPolicy, MAX_RATE_LIMIT_BLOCK_TTL_MS, MAX_RATE_LIMIT_CACHE_ENTRIES,
    RATE_LIMIT_BLOCK_PROTOCOL, RATE_LIMIT_PRINCIPAL_DIGEST_BYTES,
};
#[cfg(feature = "redis-backend")]
pub use redis_store::RedisStore;
pub use runtime::{RuntimeConfig, SyncRuntime, MAX_RECONCILE_INTERVAL};
pub use store::{CacheBackend, DisabledBackend, EventStream, Mutation, SnapshotStore};
