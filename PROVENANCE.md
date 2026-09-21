# ORES browser runtime distribution snapshot

This branch is an immutable, source-derived public distribution snapshot for cross-organization Rust consumers that must not require credentials for private canonical repositories.

Canonical sources:

- `ORESoftware/ores-websocket` at `8cceeaf7142c469bc7ef55c16dc51ae0f9aeb43e`
- `ORESoftware/ores-sw.js` at `af40988742570008d3b7cc573b673fca97b28112`

Published Cargo packages:

- `ores-websocket` 0.1.0
- `ores-sw-assets` 0.1.0
- `ores-sw-axum08` 0.1.0

The source files in this branch are copied without semantic modification from those exact canonical revisions, except for the root Cargo workspace and this provenance record. Consumers must pin the immutable commit SHA for this branch, never the moving branch name.

This distribution branch is not website source and must not be merged into `main`. A future runtime release must use a new immutable `dist/*` branch and a new consumer pin.
