# ores-runtime-server

A thin, modular Axum lifecycle crate for ORE Rust servers that embeds the reviewed `ores.lru-redis.v1` runtime cache.

Consumers own a small `runtime_env_flags.rs` contract. This crate validates that mutable keys are mapped, secrets are never exposed as flags, Redis reconciliation completes before readiness, revision gaps and stale workers withdraw readiness, local-only mode opens no backend connection, maintenance mode withdraws readiness, and status routes never return cache values.

The crate is distributed from an immutable commit. Development remains in the canonical `ores-redis-lru-cache/ores-lru-redis.rs` source and the runtime-server distribution branch records that provenance.
