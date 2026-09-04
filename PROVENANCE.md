# Distribution provenance

This branch is an immutable, source-derived distribution snapshot of the Rust crate maintained in `ores-redis-lru-cache/ores-lru-redis.rs`.

- Canonical source repository: `ores-redis-lru-cache/ores-lru-redis.rs`
- Canonical source commit: `9a1936061ff980d434ea17552d353f280592e753`
- Canonical source tree: `7255d570902094104d19b191bf70ac30d8325313`
- Package: `ores-lru-redis` `0.1.0`
- Protocol: `ores.lru-redis.v1`
- License declared by the canonical crate: MIT
- Previous public cache snapshot: `1f017df84b59865ff24528c1721bd77fd67b9794`

The Rust files under `src/` are copied byte-for-byte from the exact canonical
tree above. The root package, feature, dependency, and lint tables in
`Cargo.toml` match that source; this distribution adds only its existing
workspace stanza for `ores-runtime-server`. `Cargo.lock` is generated for that
combined public workspace and is the authority used by distribution CI.

This snapshot removes the vulnerable `lru 0.12` and `time 0.3` dependency
paths reported as RUSTSEC-2026-0002, RUSTSEC-2026-0253, and RUSTSEC-2026-0009.
The canonical source and this public distribution require Rust 1.85 or newer.

This public branch exists only so repositories in other organizations can
resolve an immutable Git dependency without receiving access to the private
canonical repository.

Do not develop on or merge this branch into the website's `main` branch. Changes belong in the canonical source repository, must pass its conformance suite, and should be distributed as a new immutable snapshot with a new source commit and distribution commit.
