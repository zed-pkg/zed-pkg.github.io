# Runtime server distribution provenance

`ores-runtime-server` is a public distribution module layered on the canonical private cache source.

- Canonical cache source commit: `4f7c46f068139c75e85548e6fe4b5f5da3ad7dcb`
- Canonical cache source tree: `22e9a597eeeaf7e0d755f196d865f5aeceae3d9f`
- Base public cache snapshot commit: `1f017df84b59865ff24528c1721bd77fd67b9794`
- Protocol: `ores.lru-redis.v1`

Consumer repositories must pin the exact runtime-server distribution commit, retain their own `runtime_env_flags.rs` mapping contract, and preserve exact-head CI. The branch is a distribution branch and must not be merged into the website's `main` branch.
