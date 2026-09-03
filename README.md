# ores-lru-redis.rs

Reference Rust implementation of `ores.lru-redis.v1`: a bounded local LRU with a backend-neutral I/O contract and an optional Redis adapter for hash snapshots, periodic repair, and Pub/Sub updates.

The local LRU does **not** require a Redis connection. `CacheBackend` is the generic snapshot/mutation/event boundary; `RedisStore` is one feature-gated implementation. New adapters can target another key/value system without changing cache policy or event-reduction code.

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
