use std::{marker::PhantomData, time::Duration};

const SENSITIVE_RUNTIME_ENV_KEY_PARTS: [&str; 6] = [
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PRIVATE_KEY",
    "DATABASE_URL",
    "CREDENTIAL",
];

pub const RATE_LIMIT_DENY_CACHE_CAPACITY: usize = 10_000;
pub const RATE_LIMIT_DENY_RECONCILE_INTERVAL: Duration = Duration::from_secs(30);
pub const RATE_LIMIT_DENY_KEY_PREFIX: &str = "rl1:";
pub const RATE_LIMIT_DENY_VALUE_PREFIX: &str = "until:";
const RATE_LIMIT_DENY_KEY_HEX_LENGTH: usize = 64;

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

/// Validates the canonical, PII-free key emitted by `ores-rl-lib-core` adapters.
///
/// The `rl1:` version prefix permits future rotation while exactly 64 lowercase
/// hexadecimal characters carry one HMAC-SHA-256 output. Raw IPs, email addresses,
/// subjects, tokens, and cookies cannot pass this boundary.
pub fn is_rate_limit_deny_key_allowed(key: &str) -> bool {
    let Some(hex) = key.strip_prefix(RATE_LIMIT_DENY_KEY_PREFIX) else {
        return false;
    };
    hex.len() == RATE_LIMIT_DENY_KEY_HEX_LENGTH
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Encodes a deny marker containing only its expiration time.
///
/// The cache is an optimization: the authoritative limiter determines the TTL,
/// and consumers must ignore markers that have expired according to their clock.
pub fn encode_rate_limit_deny_marker(blocked_until_unix_ms: u64) -> Option<String> {
    (blocked_until_unix_ms > 0)
        .then(|| format!("{RATE_LIMIT_DENY_VALUE_PREFIX}{blocked_until_unix_ms}"))
}

/// Parses the canonical `until:<unix-ms>` deny-marker value.
pub fn parse_rate_limit_deny_marker(value: &str) -> Option<u64> {
    let raw = value.strip_prefix(RATE_LIMIT_DENY_VALUE_PREFIX)?;
    if raw.is_empty() || raw.len() > 20 || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = raw.parse::<u64>().ok()?;
    if parsed == 0 || parsed.to_string() != raw {
        return None;
    }
    Some(parsed)
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

/// Bounded local deny markers coordinated through atomic Redis mutation and Pub/Sub.
///
/// This policy is deliberately fail-open on startup because it is not the source
/// of truth for rate-limit accounting. The local/service limiter still applies if
/// Redis is unavailable; the cache merely short-circuits principals already known
/// to be blocked elsewhere.
pub struct RateLimitDenyPolicy;

impl CachePolicy for RateLimitDenyPolicy {
    const CACHE_NAME: &'static str = "rate-limit-deny";
    const REDIS_SYNC: BackendSyncMode = BackendSyncMode::Bidirectional;
    const DEFAULT_CAPACITY: usize = RATE_LIMIT_DENY_CACHE_CAPACITY;
    const POLL_INTERVAL: Duration = RATE_LIMIT_DENY_RECONCILE_INTERVAL;
    const FAIL_OPEN_ON_STARTUP: bool = true;

    fn allow_key(key: &str) -> bool {
        is_rate_limit_deny_key_allowed(key)
    }

    fn redact_key(_key: &str) -> bool {
        true
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

    #[test]
    fn rate_limit_policy_is_bounded_and_non_authoritative() {
        assert_eq!(
            RateLimitDenyPolicy::backend_sync_mode(),
            BackendSyncMode::Bidirectional
        );
        assert_eq!(
            RateLimitDenyPolicy::DEFAULT_CAPACITY,
            RATE_LIMIT_DENY_CACHE_CAPACITY
        );
        assert_eq!(
            RateLimitDenyPolicy::POLL_INTERVAL,
            RATE_LIMIT_DENY_RECONCILE_INTERVAL
        );
        assert!(std::hint::black_box(
            RateLimitDenyPolicy::FAIL_OPEN_ON_STARTUP
        ));
    }

    #[test]
    fn rate_limit_policy_accepts_only_canonical_opaque_keys() {
        let valid = format!("{RATE_LIMIT_DENY_KEY_PREFIX}{}", "ab".repeat(32));
        assert!(RateLimitDenyPolicy::allow_key(&valid));
        for invalid in [
            "",
            "rl1:raw@example.com",
            "rl1:127.0.0.1",
            &format!("{RATE_LIMIT_DENY_KEY_PREFIX}{}", "AB".repeat(32)),
            &format!("{RATE_LIMIT_DENY_KEY_PREFIX}{}", "ab".repeat(31)),
            &format!("{RATE_LIMIT_DENY_KEY_PREFIX}{}", "ag".repeat(32)),
        ] {
            assert!(
                !RateLimitDenyPolicy::allow_key(invalid),
                "accepted {invalid}"
            );
        }
        assert!(RateLimitDenyPolicy::redact_key(&valid));
    }

    #[test]
    fn deny_markers_are_canonical_and_pii_free() {
        assert_eq!(
            encode_rate_limit_deny_marker(1_700_000_000_123),
            Some("until:1700000000123".to_owned())
        );
        assert_eq!(
            parse_rate_limit_deny_marker("until:1700000000123"),
            Some(1_700_000_000_123)
        );
        for invalid in ["", "until:0", "until:01", "until:user@example.com"] {
            assert_eq!(parse_rate_limit_deny_marker(invalid), None);
        }
    }
}
