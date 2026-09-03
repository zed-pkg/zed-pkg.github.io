use std::{collections::BTreeMap, num::NonZeroUsize};

use lru::LruCache;

use crate::{CacheEvent, CacheOperation, CachePolicy, CacheSnapshot, Error, Keyspace, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyOutcome {
    Applied { revision: u64 },
    Duplicate { revision: u64 },
    ReconcileRequested { revision: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotApplyOutcome {
    Applied { revision: u64 },
    StaleIgnored { current: u64, incoming: u64 },
    InvalidRejected { current: u64, incoming: u64 },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CacheState {
    pub revision: u64,
    pub ready: bool,
    pub stale: bool,
    pub entry_count: usize,
}

pub struct LocalLru<P: CachePolicy> {
    keyspace: Keyspace,
    entries: LruCache<String, String>,
    capacity: usize,
    revision: u64,
    ready: bool,
    stale: bool,
    _policy: std::marker::PhantomData<P>,
}

impl<P: CachePolicy> LocalLru<P> {
    pub fn new(namespace: impl Into<String>, capacity: usize) -> Result<Self> {
        let capacity = NonZeroUsize::new(capacity).ok_or(Error::InvalidCapacity)?;
        Ok(Self {
            keyspace: Keyspace::new(namespace, P::CACHE_NAME)?,
            entries: LruCache::new(capacity),
            capacity: capacity.get(),
            revision: 0,
            ready: false,
            stale: false,
            _policy: std::marker::PhantomData,
        })
    }

    pub fn keyspace(&self) -> &Keyspace {
        &self.keyspace
    }

    pub fn state(&self) -> CacheState {
        CacheState {
            revision: self.revision,
            ready: self.ready,
            stale: self.stale,
            entry_count: self.entries.len(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        self.entries.get(key).cloned()
    }

    pub fn snapshot(&self) -> CacheSnapshot {
        CacheSnapshot {
            revision: self.revision,
            entries: self
                .entries
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }

    /// Validates and installs an authoritative snapshot without revision regression.
    pub fn try_replace_from_snapshot(
        &mut self,
        snapshot: CacheSnapshot,
    ) -> Result<SnapshotApplyOutcome> {
        snapshot.validate_bounds()?;
        if snapshot.revision < self.revision {
            return Ok(SnapshotApplyOutcome::StaleIgnored {
                current: self.revision,
                incoming: snapshot.revision,
            });
        }

        let allowed_entries = snapshot
            .entries
            .keys()
            .filter(|key| P::allow_key(key))
            .count();
        if allowed_entries > self.capacity {
            self.mark_stale();
            return Err(Error::SnapshotExceedsCapacity {
                entries: allowed_entries,
                capacity: self.capacity,
            });
        }

        let revision = snapshot.revision;
        self.entries.clear();
        for (key, value) in snapshot.entries {
            if P::allow_key(&key) {
                self.entries.put(key, value);
            }
        }
        self.revision = revision;
        self.ready = true;
        self.stale = false;
        Ok(SnapshotApplyOutcome::Applied { revision })
    }

    /// Compatibility helper for already-validated in-process fixtures.
    ///
    /// Runtime and backend integrations should use [`Self::try_replace_from_snapshot`] so callers
    /// receive the underlying validation error. Invalid fixtures mark the cache stale instead of
    /// silently truncating or rolling state backward.
    pub fn replace_from_snapshot(&mut self, snapshot: CacheSnapshot) -> SnapshotApplyOutcome {
        let incoming = snapshot.revision;
        match self.try_replace_from_snapshot(snapshot) {
            Ok(outcome) => outcome,
            Err(_) => {
                let current = self.revision;
                self.mark_stale();
                SnapshotApplyOutcome::InvalidRejected { current, incoming }
            }
        }
    }

    pub fn mark_ready_without_backend(&mut self) {
        self.ready = true;
        self.stale = false;
    }

    /// Backward-compatible name for consumers written before the backend-neutral API.
    #[deprecated(note = "use mark_ready_without_backend")]
    pub fn mark_ready_without_redis(&mut self) {
        self.mark_ready_without_backend();
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.ready = false;
    }

    pub fn apply_event(&mut self, event: CacheEvent) -> Result<ApplyOutcome> {
        event.validate()?;
        self.validate_target(&event)?;
        if event.revision <= self.revision {
            return Ok(ApplyOutcome::Duplicate {
                revision: self.revision,
            });
        }
        if event.revision != self.revision.saturating_add(1) {
            self.mark_stale();
            return Err(Error::RevisionGap {
                local: self.revision,
                incoming: event.revision,
            });
        }

        match event.operation {
            CacheOperation::Upsert => {
                for (key, value) in event.entries {
                    if P::allow_key(&key) {
                        self.entries.put(key, value);
                    }
                }
            }
            CacheOperation::Delete => {
                for key in event.keys {
                    self.entries.pop(&key);
                }
            }
            CacheOperation::Replace => {
                let allowed_entries = event.entries.keys().filter(|key| P::allow_key(key)).count();
                if allowed_entries > self.capacity {
                    self.mark_stale();
                    return Err(Error::SnapshotExceedsCapacity {
                        entries: allowed_entries,
                        capacity: self.capacity,
                    });
                }
                self.entries.clear();
                for (key, value) in event.entries {
                    if P::allow_key(&key) {
                        self.entries.put(key, value);
                    }
                }
            }
            CacheOperation::Invalidate => {
                self.entries.clear();
            }
            CacheOperation::Resync => {
                self.mark_stale();
                return Ok(ApplyOutcome::ReconcileRequested {
                    revision: event.revision,
                });
            }
        }

        self.revision = event.revision;
        self.ready = true;
        self.stale = false;
        Ok(ApplyOutcome::Applied {
            revision: self.revision,
        })
    }

    pub fn redacted_debug_entries(&self) -> BTreeMap<String, &'static str> {
        self.entries
            .iter()
            .map(|(key, _)| {
                let marker = if P::redact_key(key) {
                    "[REDACTED]"
                } else {
                    "[PRESENT]"
                };
                (key.clone(), marker)
            })
            .collect()
    }

    fn validate_target(&self, event: &CacheEvent) -> Result<()> {
        if event.namespace != self.keyspace.namespace() || event.cache != self.keyspace.cache() {
            return Err(Error::TargetMismatch {
                expected_namespace: self.keyspace.namespace().to_owned(),
                expected_cache: self.keyspace.cache().to_owned(),
                actual_namespace: event.namespace.clone(),
                actual_cache: event.cache.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendSyncMode, CachePolicy};
    use std::collections::BTreeMap;

    struct TestPolicy;
    impl CachePolicy for TestPolicy {
        const CACHE_NAME: &'static str = "runtime-env";
        const REDIS_SYNC: BackendSyncMode = BackendSyncMode::ReadOnly;
    }

    fn event(revision: u64, operation: CacheOperation) -> CacheEvent {
        CacheEvent::new("svc", "runtime-env", revision, operation, "test")
    }

    #[test]
    fn applies_ordered_events_and_ignores_duplicates() {
        let mut cache = LocalLru::<TestPolicy>::new("svc", 8).unwrap();
        cache.replace_from_snapshot(CacheSnapshot::default());

        let mut first = event(1, CacheOperation::Upsert);
        first.entries = BTreeMap::from([("FEATURE_X".to_owned(), "on".to_owned())]);
        assert_eq!(
            cache.apply_event(first.clone()).unwrap(),
            ApplyOutcome::Applied { revision: 1 }
        );
        assert_eq!(cache.get("FEATURE_X").as_deref(), Some("on"));
        assert_eq!(
            cache.apply_event(first).unwrap(),
            ApplyOutcome::Duplicate { revision: 1 }
        );
    }

    #[test]
    fn gaps_make_readiness_false_until_reconciled() {
        let mut cache = LocalLru::<TestPolicy>::new("svc", 8).unwrap();
        cache.replace_from_snapshot(CacheSnapshot::default());
        let error = cache
            .apply_event(event(2, CacheOperation::Invalidate))
            .unwrap_err();
        assert!(matches!(
            error,
            Error::RevisionGap {
                local: 0,
                incoming: 2
            }
        ));
        assert!(!cache.state().ready);
        assert!(cache.state().stale);
    }

    #[test]
    fn stale_snapshot_cannot_roll_back_newer_event_state() {
        let mut cache = LocalLru::<TestPolicy>::new("svc", 8).unwrap();
        cache.replace_from_snapshot(CacheSnapshot {
            revision: 4,
            entries: BTreeMap::from([("FEATURE_A".to_owned(), "old".to_owned())]),
        });
        let mut update = event(5, CacheOperation::Upsert);
        update.entries = BTreeMap::from([("FEATURE_A".to_owned(), "new".to_owned())]);
        cache.apply_event(update).unwrap();

        assert_eq!(
            cache.replace_from_snapshot(CacheSnapshot {
                revision: 4,
                entries: BTreeMap::from([("FEATURE_A".to_owned(), "stale".to_owned())]),
            }),
            SnapshotApplyOutcome::StaleIgnored {
                current: 5,
                incoming: 4
            }
        );
        assert_eq!(cache.state().revision, 5);
        assert_eq!(cache.get("FEATURE_A").as_deref(), Some("new"));
    }

    #[test]
    fn authoritative_snapshots_cannot_silently_overflow_capacity() {
        let mut cache = LocalLru::<TestPolicy>::new("svc", 1).unwrap();
        let snapshot = CacheSnapshot {
            revision: 1,
            entries: BTreeMap::from([
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned()),
            ]),
        };
        assert!(matches!(
            cache.try_replace_from_snapshot(snapshot),
            Err(Error::SnapshotExceedsCapacity {
                entries: 2,
                capacity: 1
            })
        ));
        assert!(!cache.state().ready);
    }

    #[test]
    fn malformed_event_does_not_mutate_or_advance_state() {
        let mut cache = LocalLru::<TestPolicy>::new("svc", 8).unwrap();
        cache.replace_from_snapshot(CacheSnapshot::default());
        let mut malformed = event(1, CacheOperation::Invalidate);
        malformed.entries = BTreeMap::from([("FEATURE_A".to_owned(), "on".to_owned())]);

        assert!(matches!(
            cache.apply_event(malformed),
            Err(Error::InvalidEventPayload {
                operation: CacheOperation::Invalidate
            })
        ));
        assert_eq!(cache.state().revision, 0);
        assert_eq!(cache.state().entry_count, 0);
    }
}
