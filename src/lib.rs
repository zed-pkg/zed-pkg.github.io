#![forbid(unsafe_code)]

mod cache;
mod error;
mod keyspace;
mod local;
mod policy;
mod protocol;
#[cfg(feature = "redis-backend")]
mod redis_store;
mod runtime;
mod store;

pub use cache::{ApplyOutcome, CacheState, LocalLru, SnapshotApplyOutcome};
pub use error::{Error, Result};
pub use keyspace::{Keyspace, KeyspaceLayout, MAX_KEYSPACE_SEGMENT_BYTES};
pub use local::LocalRuntime;
pub use policy::{
    is_runtime_env_key_allowed, is_sensitive_runtime_env_key, BackendSyncMode, CachePolicy,
    LocalOnly, RedisSyncMode, RuntimeEnvPolicy,
};
pub use protocol::{
    CacheEvent, CacheOperation, CacheSnapshot, MAX_EVENT_BYTES, MAX_KEY_BYTES, MAX_MUTATION_BYTES,
    MAX_MUTATION_ITEMS, MAX_SAFE_REVISION, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ENTRIES,
    MAX_SOURCE_BYTES, MAX_TIMESTAMP_BYTES, MAX_VALUE_BYTES, PROTOCOL,
};
#[cfg(feature = "redis-backend")]
pub use redis_store::RedisStore;
pub use runtime::{RuntimeConfig, SyncRuntime, MAX_RECONCILE_INTERVAL};
pub use store::{CacheBackend, DisabledBackend, EventStream, Mutation, SnapshotStore};
