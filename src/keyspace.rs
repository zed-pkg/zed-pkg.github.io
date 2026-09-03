use crate::{Error, Result};

pub const MAX_KEYSPACE_SEGMENT_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KeyspaceLayout {
    /// Co-locates every key used by one atomic mutation in a Redis Cluster hash slot.
    #[default]
    ClusterSafe,
    /// Preserves the original unhashed key layout for controlled migration only.
    LegacyUnhashed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keyspace {
    namespace: String,
    cache: String,
    layout: KeyspaceLayout,
}

impl Keyspace {
    pub fn new(namespace: impl Into<String>, cache: impl Into<String>) -> Result<Self> {
        Self::with_layout(namespace, cache, KeyspaceLayout::ClusterSafe)
    }

    pub fn legacy(namespace: impl Into<String>, cache: impl Into<String>) -> Result<Self> {
        Self::with_layout(namespace, cache, KeyspaceLayout::LegacyUnhashed)
    }

    pub fn with_layout(
        namespace: impl Into<String>,
        cache: impl Into<String>,
        layout: KeyspaceLayout,
    ) -> Result<Self> {
        let namespace = namespace.into();
        let cache = cache.into();
        if !valid_segment(&namespace) || !valid_segment(&cache) {
            return Err(Error::InvalidKeyspace);
        }
        Ok(Self {
            namespace,
            cache,
            layout,
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn cache(&self) -> &str {
        &self.cache
    }

    pub const fn layout(&self) -> KeyspaceLayout {
        self.layout
    }

    /// Logical Redis hash-tag contents shared by this cache's cluster-safe keys.
    pub fn hash_tag(&self) -> String {
        format!("{}:{}", self.namespace, self.cache)
    }

    pub fn snapshot_key(&self) -> String {
        format!("{}:snapshot", self.prefix())
    }

    pub fn meta_key(&self) -> String {
        format!("{}:meta", self.prefix())
    }

    pub fn event_channel(&self) -> String {
        format!("{}:events", self.prefix())
    }

    fn prefix(&self) -> String {
        match self.layout {
            KeyspaceLayout::ClusterSafe => format!("ores:lru:v1:{{{}}}", self.hash_tag()),
            KeyspaceLayout::LegacyUnhashed => {
                format!("ores:lru:v1:{}:{}", self.namespace, self.cache)
            }
        }
    }
}

pub(crate) fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_KEYSPACE_SEGMENT_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_cluster_safe_canonical_keys_by_default() {
        let keys = Keyspace::new("payments-api", "runtime-env").unwrap();
        assert_eq!(keys.layout(), KeyspaceLayout::ClusterSafe);
        assert_eq!(keys.hash_tag(), "payments-api:runtime-env");
        assert_eq!(
            keys.snapshot_key(),
            "ores:lru:v1:{payments-api:runtime-env}:snapshot"
        );
        assert_eq!(
            keys.meta_key(),
            "ores:lru:v1:{payments-api:runtime-env}:meta"
        );
        assert_eq!(
            keys.event_channel(),
            "ores:lru:v1:{payments-api:runtime-env}:events"
        );
    }

    #[test]
    fn preserves_an_explicit_legacy_layout_for_migration() {
        let keys = Keyspace::legacy("payments-api", "runtime-env").unwrap();
        assert_eq!(keys.layout(), KeyspaceLayout::LegacyUnhashed);
        assert_eq!(
            keys.snapshot_key(),
            "ores:lru:v1:payments-api:runtime-env:snapshot"
        );
        assert_eq!(keys.meta_key(), "ores:lru:v1:payments-api:runtime-env:meta");
        assert_eq!(
            keys.event_channel(),
            "ores:lru:v1:payments-api:runtime-env:events"
        );
    }

    #[test]
    fn rejects_ambiguous_unbounded_or_hash_tag_injecting_segments() {
        for namespace in [
            "a:b",
            "",
            "service name",
            "service/name",
            "service{prod}",
            "service\nname",
            "servicé",
        ] {
            assert!(
                Keyspace::new(namespace, "runtime-env").is_err(),
                "{namespace}"
            );
        }
        assert!(Keyspace::new("a".repeat(MAX_KEYSPACE_SEGMENT_BYTES + 1), "runtime-env").is_err());
        assert!(Keyspace::new("service_1.prod", "runtime-env").is_ok());
    }
}
