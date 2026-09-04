# Runtime server distribution provenance

`ores-runtime-server` is a public distribution module layered on the canonical private cache source.

- Canonical cache source commit: `9a1936061ff980d434ea17552d353f280592e753`
- Canonical cache source tree: `7255d570902094104d19b191bf70ac30d8325313`
- Previous public runtime distribution commit: `51c090077619e6be5d5a8055071cf04c46ebfaac`
- Protocol: `ores.lru-redis.v1`

The runtime-server behavior is unchanged from the previous public
distribution. This snapshot advances its path dependency to the advisory-free
canonical cache source, makes the cache-guard lifetime explicit for
compatibility with the updated dependency, normalizes formatting under Rust
1.85.1, and locks the complete public workspace dependency graph.

Consumer repositories must pin the exact runtime-server distribution commit, retain their own `runtime_env_flags.rs` mapping contract, and preserve exact-head CI. The branch is a distribution branch and must not be merged into the website's `main` branch.
