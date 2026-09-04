use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{keyspace::valid_segment, Error, Result};

pub const PROTOCOL: &str = "ores.lru-redis.v1";

/// Largest integer represented exactly by every supported runtime, including JavaScript.
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
pub const MAX_MUTATION_ITEMS: usize = 1_024;
pub const MAX_KEY_BYTES: usize = 512;
pub const MAX_VALUE_BYTES: usize = 1_048_576;
pub const MAX_MUTATION_BYTES: usize = 4 * 1_048_576;
pub const MAX_EVENT_BYTES: usize = 4 * 1_048_576;
pub const MAX_SNAPSHOT_ENTRIES: usize = 100_000;
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1_048_576;
pub const MAX_SOURCE_BYTES: usize = 256;
pub const MAX_TIMESTAMP_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheOperation {
    Upsert,
    Delete,
    Replace,
    Invalidate,
    Resync,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CacheEvent {
    pub protocol: String,
    pub namespace: String,
    pub cache: String,
    pub revision: u64,
    pub operation: CacheOperation,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entries: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    pub source: String,
    pub published_at: String,
}

impl CacheEvent {
    pub fn new(
        namespace: impl Into<String>,
        cache: impl Into<String>,
        revision: u64,
        operation: CacheOperation,
        source: impl Into<String>,
    ) -> Self {
        Self {
            protocol: PROTOCOL.to_owned(),
            namespace: namespace.into(),
            cache: cache.into(),
            revision,
            operation,
            entries: BTreeMap::new(),
            keys: Vec::new(),
            source: source.into(),
            published_at: unix_timestamp_string(),
        }
    }

    /// Validates the complete language-neutral event envelope before cache state is touched.
    pub fn validate(&self) -> Result<()> {
        if self.protocol != PROTOCOL {
            return Err(Error::ProtocolMismatch {
                expected: PROTOCOL,
                actual: self.protocol.clone(),
            });
        }
        if !valid_segment(&self.namespace) || !valid_segment(&self.cache) {
            return Err(Error::InvalidKeyspace);
        }
        if self.revision == 0 || self.revision > MAX_SAFE_REVISION {
            return Err(Error::RevisionOutOfRange {
                revision: self.revision,
                max: MAX_SAFE_REVISION,
            });
        }
        validate_text("event source", &self.source, MAX_SOURCE_BYTES)?;
        validate_text("event timestamp", &self.published_at, MAX_TIMESTAMP_BYTES)?;

        let item_count = match self.operation {
            CacheOperation::Upsert => {
                if self.entries.is_empty() || !self.keys.is_empty() {
                    return Err(Error::InvalidEventPayload {
                        operation: self.operation,
                    });
                }
                self.entries.len()
            }
            CacheOperation::Delete => {
                if self.keys.is_empty() || !self.entries.is_empty() {
                    return Err(Error::InvalidEventPayload {
                        operation: self.operation,
                    });
                }
                self.keys.len()
            }
            CacheOperation::Replace => {
                if !self.keys.is_empty() {
                    return Err(Error::InvalidEventPayload {
                        operation: self.operation,
                    });
                }
                self.entries.len()
            }
            CacheOperation::Invalidate | CacheOperation::Resync => {
                if !self.entries.is_empty() || !self.keys.is_empty() {
                    return Err(Error::InvalidEventPayload {
                        operation: self.operation,
                    });
                }
                0
            }
        };
        ensure_limit("mutation item count", item_count, MAX_MUTATION_ITEMS)?;

        let mut payload_bytes = 0usize;
        for (key, value) in &self.entries {
            validate_cache_key(key)?;
            ensure_limit("cache value bytes", value.len(), MAX_VALUE_BYTES)?;
            payload_bytes = payload_bytes
                .saturating_add(key.len())
                .saturating_add(value.len());
        }
        let mut unique_keys = BTreeSet::new();
        for key in &self.keys {
            validate_cache_key(key)?;
            if !unique_keys.insert(key) {
                return Err(Error::InvalidEvent("delete event contains duplicate keys"));
            }
            payload_bytes = payload_bytes.saturating_add(key.len());
        }
        ensure_limit("mutation payload bytes", payload_bytes, MAX_MUTATION_BYTES)?;

        let encoded_bytes = serde_json::to_vec(self)?.len();
        ensure_limit("event bytes", encoded_bytes, MAX_EVENT_BYTES)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CacheSnapshot {
    pub revision: u64,
    pub entries: BTreeMap<String, String>,
}

impl CacheSnapshot {
    pub fn validate_bounds(&self) -> Result<()> {
        if self.revision > MAX_SAFE_REVISION {
            return Err(Error::RevisionOutOfRange {
                revision: self.revision,
                max: MAX_SAFE_REVISION,
            });
        }
        if self.revision == 0 && !self.entries.is_empty() {
            return Err(Error::InvalidEvent(
                "snapshot entries require non-zero revision metadata",
            ));
        }
        ensure_limit(
            "snapshot entry count",
            self.entries.len(),
            MAX_SNAPSHOT_ENTRIES,
        )?;
        let mut bytes = 0usize;
        for (key, value) in &self.entries {
            validate_cache_key(key)?;
            ensure_limit("cache value bytes", value.len(), MAX_VALUE_BYTES)?;
            bytes = bytes.saturating_add(key.len()).saturating_add(value.len());
        }
        ensure_limit("snapshot payload bytes", bytes, MAX_SNAPSHOT_BYTES)
    }
}

fn validate_cache_key(key: &str) -> Result<()> {
    validate_text("cache key bytes", key, MAX_KEY_BYTES)
}

fn validate_text(kind: &'static str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::InvalidEvent(
            "text field is empty or contains control characters",
        ));
    }
    ensure_limit(kind, value.len(), max)
}

fn ensure_limit(kind: &'static str, actual: usize, max: usize) -> Result<()> {
    if actual > max {
        return Err(Error::PayloadLimitExceeded { kind, actual, max });
    }
    Ok(())
}

pub(super) fn unix_timestamp_string() -> String {
    jiff::Timestamp::now().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(operation: CacheOperation) -> CacheEvent {
        CacheEvent::new("svc", "runtime-env", 1, operation, "test")
    }

    #[test]
    fn validates_strict_operation_shapes() {
        let mut upsert = event(CacheOperation::Upsert);
        upsert.entries.insert("A".to_owned(), "1".to_owned());
        upsert.validate().unwrap();

        upsert.keys.push("A".to_owned());
        assert!(matches!(
            upsert.validate(),
            Err(Error::InvalidEventPayload {
                operation: CacheOperation::Upsert
            })
        ));

        let mut invalidate = event(CacheOperation::Invalidate);
        invalidate.entries.insert("A".to_owned(), "1".to_owned());
        assert!(invalidate.validate().is_err());
    }

    #[test]
    fn rejects_unsafe_revisions_and_duplicate_delete_keys() {
        let mut delete = event(CacheOperation::Delete);
        delete.keys = vec!["A".to_owned(), "A".to_owned()];
        assert!(delete.validate().is_err());

        delete.keys.pop();
        delete.revision = MAX_SAFE_REVISION + 1;
        assert!(matches!(
            delete.validate(),
            Err(Error::RevisionOutOfRange { .. })
        ));
    }

    #[test]
    fn rejects_unbounded_or_control_bearing_payloads() {
        let mut upsert = event(CacheOperation::Upsert);
        upsert.entries.insert("A\nB".to_owned(), "1".to_owned());
        assert!(upsert.validate().is_err());

        upsert.entries.clear();
        upsert
            .entries
            .insert("A".to_owned(), "x".repeat(MAX_VALUE_BYTES + 1));
        assert!(matches!(
            upsert.validate(),
            Err(Error::PayloadLimitExceeded { .. })
        ));
    }

    #[test]
    fn rejects_orphaned_snapshot_entries() {
        let snapshot = CacheSnapshot {
            revision: 0,
            entries: BTreeMap::from([("A".to_owned(), "1".to_owned())]),
        };
        assert!(matches!(
            snapshot.validate_bounds(),
            Err(Error::InvalidEvent(_))
        ));
    }

    #[test]
    fn generated_timestamp_is_bounded_rfc3339() {
        let generated = unix_timestamp_string();
        let _: jiff::Timestamp = generated.parse().expect("RFC 3339 timestamp");
        assert!(generated.len() <= MAX_TIMESTAMP_BYTES);
    }
}
