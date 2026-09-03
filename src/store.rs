use std::{collections::BTreeMap, pin::Pin};

use async_trait::async_trait;
use futures_util::Stream;

use crate::{CacheEvent, CacheSnapshot, Error, Keyspace, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mutation {
    Upsert(BTreeMap<String, String>),
    Delete(Vec<String>),
    Replace(BTreeMap<String, String>),
    Invalidate,
    Resync,
}

pub type EventStream = Pin<Box<dyn Stream<Item = Result<CacheEvent>> + Send>>;

/// Backend-neutral I/O contract for snapshot repair, mutation, and event delivery.
///
/// Redis is one implementation of this trait, not a requirement of the local LRU.
/// Other adapters may target an embedded store, etcd, FoundationDB, NATS KV, or a
/// service-specific key/value API while preserving the same revision protocol.
#[async_trait]
pub trait CacheBackend: Send + Sync + 'static {
    async fn read_snapshot(&self, keyspace: &Keyspace) -> Result<CacheSnapshot>;

    async fn mutate(
        &self,
        keyspace: &Keyspace,
        mutation: Mutation,
        source: &str,
    ) -> Result<CacheEvent>;

    async fn subscribe(&self, keyspace: &Keyspace) -> Result<EventStream>;
}

/// Backward-compatible name retained for existing Redis consumers.
pub use CacheBackend as SnapshotStore;

/// Backend used by a truly local-only runtime.
///
/// A disabled backend intentionally fails any accidental I/O call. When paired
/// with [`crate::LocalOnly`], `SyncRuntime` never invokes these methods.
#[derive(Clone, Copy, Debug, Default)]
pub struct DisabledBackend;

#[async_trait]
impl CacheBackend for DisabledBackend {
    async fn read_snapshot(&self, _: &Keyspace) -> Result<CacheSnapshot> {
        Err(Error::BackendDisabled)
    }

    async fn mutate(&self, _: &Keyspace, _: Mutation, _: &str) -> Result<CacheEvent> {
        Err(Error::BackendDisabled)
    }

    async fn subscribe(&self, _: &Keyspace) -> Result<EventStream> {
        Err(Error::BackendDisabled)
    }
}
