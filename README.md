# ores-lru-redis.rs

Reference Rust implementation of `ores.lru-redis.v1`: a bounded local LRU with a backend-neutral I/O contract and an optional Redis adapter for hash snapshots, periodic repair, and Pub/Sub updates.

The crate requires Rust 1.85 or newer. Its committed lockfile and CI exclude the vulnerable
`lru < 0.18.2` lines, and timestamp generation uses the formatting-only, Rust-1.70-compatible
`jiff` API rather than pulling a parser into the cache event path.

The local LRU does **not** require a Redis connection. `CacheBackend` is the generic snapshot/mutation/event boundary; `RedisStore` is one feature-gated implementation. New adapters can target another key/value system without changing cache policy or event-reduction code.

## Rate-limit deny-cache policy

`RateLimitDenyPolicy` is the shared fast-path policy for `github.com/ores-rate-limit`. It is deliberately **not** an authoritative request counter:

- local capacity defaults to 10,000 recently denied opaque principals;
- Redis mutation and Pub/Sub distribute deny markers while periodic snapshots repair missed events;
- startup is fail-open because the service/edge limiter still owns the decision;
- keys must be canonical `rl1:<64 lowercase hex>` HMAC outputs—raw IPs, emails, subjects, bearer tokens, and cookies are rejected;
- values must be canonical `until:<unix-ms>` markers produced and parsed by `encode_rate_limit_deny_marker` / `parse_rate_limit_deny_marker`;
- all key diagnostics are redacted.

```rust
use ores_lru_redis::{
    parse_rate_limit_deny_marker, RateLimitDenyPolicy, RuntimeConfig, SyncRuntime,
};

let config = RuntimeConfig::for_policy::<RateLimitDenyPolicy>();
assert_eq!(config.capacity, 10_000);

// After reading a cache value, always enforce expiry locally.
let still_blocked = parse_rate_limit_deny_marker("until:1700000000123")
    .is_some_and(|blocked_until_ms| blocked_until_ms > now_unix_ms);
```

Use the authoritative rate-limit implementation for ordinary accounting and a transactional data-store quota for billing, scarce-resource allocation, or irreversible writes. The LRU/Pub/Sub path permits a short eventual-consistency burst and intentionally favors availability when Redis is down.

## Strict rate-limit block-state facade

`RateLimitBlockCache` is a separate, richer contract for callers that must distinguish a known block from unavailable or unreconciled cache state. It does not replace `RateLimitDenyPolicy` and there is no implicit conversion between their key or value formats:

- the cache namespace is `rate-limit-blocks`, separate from `rate-limit-deny`;
- keys are bare 64-character lowercase hexadecimal principal digests; raw identity and malformed digests are rejected;
- values use the versioned `ores.rate-limit.block.v1` JSON contract and include policy ID, HMAC key version, issuance, expiry, and bounded reason code;
- local capacity is hard-bounded at 10,000 entries and block TTL is at most one hour;
- startup and stale state are exposed as `RateLimitBlockLookup::Unavailable`, allowing strict consumers to fail closed explicitly;
- malformed, expired, or identity-leaking events and snapshots are rejected before mutation, mark the cache stale, and require reconciliation;
- every key remains redacted in diagnostics.

```rust
use ores_lru_redis::{RateLimitBlockCache, RateLimitBlockLookup};

let mut cache = RateLimitBlockCache::new("shared-auth", 10_000)?;
match cache.lookup(&principal_digest, now_unix_ms)? {
    RateLimitBlockLookup::Blocked(block) => deny_until(block.expires_at_unix_ms),
    RateLimitBlockLookup::NotBlocked => continue_to_authoritative_limiter(),
    RateLimitBlockLookup::Unavailable => fail_closed_or_query_authority(),
}
```

Neither cache is the authoritative quota ledger. `RateLimitDenyPolicy` is an availability-favoring shortcut; `RateLimitBlockCache` is a fail-closed state facade whose caller must still consult the authoritative limiter when a definitive accounting decision is required.

The repository-root `.zpkg.toml` declares `ores-rate-limit/ores-rl-lib-core`; cross-repository adoption must use zed-pkg rather than hand-vendored copies.

## Operating modes

- **Local only:** `LocalRuntime<P>` combines `LocalOnly<P>` with `DisabledBackend`; no network connection is opened and accidental backend I/O fails closed.
- **Redis synchronized:** `RedisStore` implements `CacheBackend` behind the `redis-backend` Cargo feature.
- **Alternative backend:** implement `CacheBackend` for another store while preserving the `ores.lru-redis.v1` revision and event contract, or define a versioned successor protocol if the store cannot provide those semantics.

A consumer that does not need the Redis crate can disable default features:

```toml
[dependencies]
ores-lru-redis = { git = "https://github.com/ores-redis-lru-cache/ores-lru-redis.rs", default-features = false }
```

Local-only construction preserves the wrapped policy's cache name, capacity, allowlist, and redaction behavior:

```rust
use ores_lru_redis::{LocalOnly, LocalRuntime, RuntimeConfig, RuntimeEnvPolicy};

let runtime = LocalRuntime::<RuntimeEnvPolicy>::local_only(
    "example-service",
    RuntimeConfig::for_policy::<LocalOnly<RuntimeEnvPolicy>>(),
)?;
```

## Guarantees

- startup reconciliation before readiness when the selected policy reads a backend;
- event fanout for `upsert`, `delete`, `replace`, `invalidate`, and `resync`;
- monotonically increasing revisions in the cross-runtime exact-integer range;
- one atomic Redis script reads snapshot entries and revision together;
- event reduction and snapshot replacement are serialized, and stale snapshots cannot roll state backward;
- duplicate events are ignored and revision gaps force a full reconciliation;
- the repair interval defaults to 180 seconds and backend-reading caches cannot configure a longer interval;
- malformed, oversized, or operation-inconsistent payloads fail before cache state is touched;
- secret values are never included in diagnostics emitted by this crate.

See [`docs/SERVER_INTEGRATION.md`](docs/SERVER_INTEGRATION.md) for the required Rust server module and `flags-2-env` mapping contract, and [`docs/BACKEND_ABSTRACTION.md`](docs/BACKEND_ABSTRACTION.md) for adapter responsibilities.

## Redis keyspace

New keyspaces place every key used by one atomic mutation in the same Redis Cluster hash slot:

```text
ores:lru:v1:{namespace:cache}:snapshot
ores:lru:v1:{namespace:cache}:meta
ores:lru:v1:{namespace:cache}:events
```

`Keyspace::legacy(namespace, cache)` explicitly preserves the original unhashed layout for controlled migration. The protocol never mixes layouts implicitly. A multi-node Redis Cluster still requires a cluster-aware client; the hash tag guarantees that this protocol's related script keys are eligible for one slot.

Namespace and cache segments are limited to 64 ASCII letters, digits, `.`, `_`, and `-`, which also prevents hash-tag injection.

## Cross-runtime safety bounds

| Boundary | Limit |
|---|---:|
| revision | `2^53 - 1` |
| namespace/cache segment | 64 ASCII bytes |
| mutation items | 1,024 |
| cache key | 512 bytes |
| one value | 1 MiB |
| mutation/event payload | 4 MiB |
| snapshot | 100,000 entries / 64 MiB |
| required reconciliation interval | at most 180 seconds |

The mutation Lua script validates and encodes the complete event before its first Redis write. Snapshot reads execute `HLEN`, `HGET revision`, and `HGETALL` inside one Redis script, rejecting orphaned snapshots, invalid revisions, and oversized content before returning it.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --no-default-features
```

With `REDIS_URL` configured, the all-feature suite includes live Redis 7.4 mutation, atomic snapshot consistency, concurrency, Pub/Sub, reconnect repair, malformed-event, revision-ceiling, explicit legacy-layout, and no-partial-write scenarios.

Linear project: [Redis-backed LRU Runtime Configuration](https://linear.app/denman/project/redis-backed-lru-runtime-configuration-bfcfa30db468)
