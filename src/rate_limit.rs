use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    ApplyOutcome, BackendSyncMode, CacheEvent, CacheOperation, CachePolicy, CacheSnapshot,
    CacheState, Error, Keyspace, LocalLru, Result, SnapshotApplyOutcome,
};

pub const RATE_LIMIT_BLOCK_PROTOCOL: &str = "ores.rate-limit.block.v1";
pub const RATE_LIMIT_PRINCIPAL_DIGEST_BYTES: usize = 64;
pub const MAX_RATE_LIMIT_CACHE_ENTRIES: usize = 10_000;
pub const MAX_RATE_LIMIT_BLOCK_TTL_MS: u64 = 60 * 60 * 1_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RateLimitBlock {
    pub protocol: String,
    pub policy_id: String,
    pub key_version: String,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub reason_code: String,
}

impl RateLimitBlock {
    pub fn new(
        policy_id: impl Into<String>,
        key_version: impl Into<String>,
        issued_at_unix_ms: u64,
        expires_at_unix_ms: u64,
        reason_code: impl Into<String>,
    ) -> Result<Self> {
        let block = Self {
            protocol: RATE_LIMIT_BLOCK_PROTOCOL.to_owned(),
            policy_id: policy_id.into(),
            key_version: key_version.into(),
            issued_at_unix_ms,
            expires_at_unix_ms,
            reason_code: reason_code.into(),
        };
        block.validate()?;
        Ok(block)
    }

    pub fn validate(&self) -> Result<()> {
        if self.protocol != RATE_LIMIT_BLOCK_PROTOCOL {
            return Err(Error::InvalidRateLimitBlock(
                "rate-limit block protocol does not match",
            ));
        }
        if !valid_label(&self.policy_id, 128) {
            return Err(Error::InvalidRateLimitBlock(
                "rate-limit policy ID is invalid",
            ));
        }
        if !valid_label(&self.key_version, 64) {
            return Err(Error::InvalidRateLimitBlock(
                "rate-limit key version is invalid",
            ));
        }
        if !valid_label(&self.reason_code, 128) {
            return Err(Error::InvalidRateLimitBlock(
                "rate-limit reason code is invalid",
            ));
        }
        let ttl = self
            .expires_at_unix_ms
            .checked_sub(self.issued_at_unix_ms)
            .filter(|ttl| *ttl > 0)
            .ok_or(Error::InvalidRateLimitBlock(
                "rate-limit block expiry must follow issuance",
            ))?;
        if self.issued_at_unix_ms == 0 || ttl > MAX_RATE_LIMIT_BLOCK_TTL_MS {
            return Err(Error::InvalidRateLimitBlock(
                "rate-limit block TTL is outside the local-cache contract",
            ));
        }
        Ok(())
    }

    pub const fn is_active_at(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.issued_at_unix_ms && now_unix_ms < self.expires_at_unix_ms
    }

    pub fn encode(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn decode(value: &str) -> Result<Self> {
        let block: Self = serde_json::from_str(value)?;
        block.validate()?;
        Ok(block)
    }
}

/// Strict block-state policy. Unlike [`crate::RateLimitDenyPolicy`], this
/// policy is not an availability-favoring shortcut: an unreconciled or stale
/// cache is surfaced as unavailable so callers can fail closed or query the
/// authoritative limiter explicitly.
pub struct RateLimitBlockPolicy;

impl CachePolicy for RateLimitBlockPolicy {
    const CACHE_NAME: &'static str = "rate-limit-blocks";
    const REDIS_SYNC: BackendSyncMode = BackendSyncMode::Bidirectional;
    const DEFAULT_CAPACITY: usize = MAX_RATE_LIMIT_CACHE_ENTRIES;
    const POLL_INTERVAL: Duration = Duration::from_secs(180);
    const FAIL_OPEN_ON_STARTUP: bool = false;

    fn allow_key(key: &str) -> bool {
        is_opaque_principal_digest(key)
    }

    fn redact_key(_key: &str) -> bool {
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RateLimitBlockLookup {
    Blocked(RateLimitBlock),
    NotBlocked,
    Unavailable,
}

pub struct RateLimitBlockCache {
    inner: LocalLru<RateLimitBlockPolicy>,
}

impl RateLimitBlockCache {
    pub fn new(namespace: impl Into<String>, capacity: usize) -> Result<Self> {
        if capacity > MAX_RATE_LIMIT_CACHE_ENTRIES {
            return Err(Error::RateLimitCacheCapacityExceeded {
                requested: capacity,
                max: MAX_RATE_LIMIT_CACHE_ENTRIES,
            });
        }
        Ok(Self {
            inner: LocalLru::new(namespace, capacity)?,
        })
    }

    pub fn keyspace(&self) -> &Keyspace {
        self.inner.keyspace()
    }

    pub fn state(&self) -> CacheState {
        self.inner.state()
    }

    pub fn lookup(
        &mut self,
        principal_digest: &str,
        now_unix_ms: u64,
    ) -> Result<RateLimitBlockLookup> {
        if !is_opaque_principal_digest(principal_digest) {
            return Err(Error::InvalidRateLimitPrincipalDigest);
        }
        let state = self.inner.state();
        if !state.ready || state.stale {
            return Ok(RateLimitBlockLookup::Unavailable);
        }

        let Some(encoded) = self.inner.get(principal_digest) else {
            return Ok(RateLimitBlockLookup::NotBlocked);
        };
        let block = match RateLimitBlock::decode(&encoded) {
            Ok(block) => block,
            Err(error) => {
                self.inner.mark_stale();
                return Err(error);
            }
        };
        if block.is_active_at(now_unix_ms) {
            Ok(RateLimitBlockLookup::Blocked(block))
        } else {
            Ok(RateLimitBlockLookup::NotBlocked)
        }
    }

    pub fn apply_event(
        &mut self,
        event: CacheEvent,
        observed_at_unix_ms: u64,
    ) -> Result<ApplyOutcome> {
        if let Err(error) = validate_rate_limit_event(&event, observed_at_unix_ms) {
            self.inner.mark_stale();
            return Err(error);
        }
        self.inner.apply_event(event)
    }

    pub fn try_replace_from_snapshot(
        &mut self,
        snapshot: CacheSnapshot,
        observed_at_unix_ms: u64,
    ) -> Result<SnapshotApplyOutcome> {
        if let Err(error) = validate_rate_limit_snapshot(&snapshot, observed_at_unix_ms) {
            self.inner.mark_stale();
            return Err(error);
        }
        self.inner.try_replace_from_snapshot(snapshot)
    }

    pub fn mark_stale(&mut self) {
        self.inner.mark_stale();
    }
}

pub fn is_opaque_principal_digest(value: &str) -> bool {
    value.len() == RATE_LIMIT_PRINCIPAL_DIGEST_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_rate_limit_event(event: &CacheEvent, observed_at_unix_ms: u64) -> Result<()> {
    event.validate()?;
    match event.operation {
        CacheOperation::Upsert | CacheOperation::Replace => {
            for (principal_digest, value) in &event.entries {
                validate_entry(principal_digest, value, observed_at_unix_ms)?;
            }
        }
        CacheOperation::Delete => {
            for principal_digest in &event.keys {
                if !is_opaque_principal_digest(principal_digest) {
                    return Err(Error::InvalidRateLimitPrincipalDigest);
                }
            }
        }
        CacheOperation::Invalidate | CacheOperation::Resync => {}
    }
    Ok(())
}

fn validate_rate_limit_snapshot(snapshot: &CacheSnapshot, observed_at_unix_ms: u64) -> Result<()> {
    snapshot.validate_bounds()?;
    for (principal_digest, value) in &snapshot.entries {
        validate_entry(principal_digest, value, observed_at_unix_ms)?;
    }
    Ok(())
}

fn validate_entry(principal_digest: &str, value: &str, observed_at_unix_ms: u64) -> Result<()> {
    if !is_opaque_principal_digest(principal_digest) {
        return Err(Error::InvalidRateLimitPrincipalDigest);
    }
    let block = RateLimitBlock::decode(value)?;
    if !block.is_active_at(observed_at_unix_ms) {
        return Err(Error::ExpiredRateLimitBlock {
            expires_at_unix_ms: block.expires_at_unix_ms,
            observed_at_unix_ms,
        });
    }
    Ok(())
}

fn valid_label(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, RATE_LIMIT_PRINCIPAL_DIGEST_BYTES).collect()
    }

    fn block(now: u64) -> RateLimitBlock {
        RateLimitBlock::new("login-ip", "v1", now, now + 1_000, "quota-exceeded").unwrap()
    }

    #[test]
    fn only_lowercase_sha256_sized_digests_are_accepted() {
        assert!(is_opaque_principal_digest(&digest('a')));
        for value in [
            "203.0.113.9".to_owned(),
            "person@example.com".to_owned(),
            "user-123".to_owned(),
            digest('A'),
            "a".repeat(63),
            "g".repeat(64),
        ] {
            assert!(!is_opaque_principal_digest(&value), "{value}");
        }
    }

    #[test]
    fn local_rate_limit_cache_is_hard_bounded() {
        assert!(RateLimitBlockCache::new("shared-auth", 10_000).is_ok());
        assert!(matches!(
            RateLimitBlockCache::new("shared-auth", 10_001),
            Err(Error::RateLimitCacheCapacityExceeded {
                requested: 10_001,
                max: 10_000
            })
        ));
    }

    #[test]
    fn active_blocks_apply_and_expire_without_exposing_raw_identity() {
        let now = 1_000_000;
        let principal = digest('b');
        let mut cache = RateLimitBlockCache::new("shared-auth", 8).unwrap();
        cache
            .try_replace_from_snapshot(CacheSnapshot::default(), now)
            .unwrap();

        let mut event = CacheEvent::new(
            "shared-auth",
            RateLimitBlockPolicy::CACHE_NAME,
            1,
            CacheOperation::Upsert,
            "test",
        );
        event
            .entries
            .insert(principal.clone(), block(now).encode().unwrap());
        assert_eq!(
            cache.apply_event(event, now).unwrap(),
            ApplyOutcome::Applied { revision: 1 }
        );
        assert!(matches!(
            cache.lookup(&principal, now + 1).unwrap(),
            RateLimitBlockLookup::Blocked(_)
        ));
        assert_eq!(
            cache.lookup(&principal, now + 1_000).unwrap(),
            RateLimitBlockLookup::NotBlocked
        );
        assert!(RateLimitBlockPolicy::redact_key(&principal));
    }

    #[test]
    fn malformed_or_expired_events_fail_before_state_mutation() {
        let now = 1_000_000;
        let mut cache = RateLimitBlockCache::new("shared-auth", 8).unwrap();
        cache
            .try_replace_from_snapshot(CacheSnapshot::default(), now)
            .unwrap();

        let mut raw_identity = CacheEvent::new(
            "shared-auth",
            RateLimitBlockPolicy::CACHE_NAME,
            1,
            CacheOperation::Upsert,
            "test",
        );
        raw_identity.entries = BTreeMap::from([(
            "person@example.com".to_owned(),
            block(now).encode().unwrap(),
        )]);
        assert!(matches!(
            cache.apply_event(raw_identity, now),
            Err(Error::InvalidRateLimitPrincipalDigest)
        ));
        assert_eq!(cache.state().revision, 0);
        assert!(cache.state().stale);

        cache
            .try_replace_from_snapshot(CacheSnapshot::default(), now)
            .unwrap();
        let expired =
            RateLimitBlock::new("login-ip", "v1", now - 2_000, now - 1_000, "quota-exceeded")
                .unwrap();
        let mut event = CacheEvent::new(
            "shared-auth",
            RateLimitBlockPolicy::CACHE_NAME,
            1,
            CacheOperation::Upsert,
            "test",
        );
        event.entries.insert(digest('c'), expired.encode().unwrap());
        assert!(matches!(
            cache.apply_event(event, now),
            Err(Error::ExpiredRateLimitBlock { .. })
        ));
        assert_eq!(cache.state().revision, 0);
    }

    #[test]
    fn unavailable_state_is_explicit_and_fail_closed_callers_can_branch_on_it() {
        let mut cache = RateLimitBlockCache::new("shared-auth", 8).unwrap();
        assert_eq!(
            cache.lookup(&digest('d'), 1).unwrap(),
            RateLimitBlockLookup::Unavailable
        );
    }
}
