# Shared WebAssembly loader pilot

The zpkg.net Astro site participates in the `ores-wasm-loaders` pilot. The page bootstrap loads the shared coordinator and independent `owls-interfaces` contract, but it never starts product application code during ordinary page load.

The existing individual/team account link starts fetch-only preparation after 150 ms of sustained intent. The prepared resource is an 8-byte public WebAssembly canary. Explicit diagnostics may call `globalThis.__ORES_WASM_LOADER__.activateProbe()`; navigation never does so automatically.

Immutable external inputs:

- `owls-interfaces`: `b0e687c88b652d25964c041e2fdd0222f512fddd`
- `owls-web-loader`: `3b92396e34ffd0ba6411261957e47dd62cf3b4a4`
- probe SHA-256: `93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476`

The jsDelivr GitHub-content requests are credentialless and contain no account data. This validates coordinator integration, verified intent preparation, and cancellation; it does not promise a universal cross-site cache or a runtime that survives full-page navigation.
