const INTERFACES_COMMIT = 'b0e687c88b652d25964c041e2fdd0222f512fddd';
const LOADER_COMMIT = '3b92396e34ffd0ba6411261957e47dd62cf3b4a4';
const PROBE_SHA256 = '93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476';
const CDN = 'https://cdn.jsdelivr.net/gh/ores-wasm-loaders';
const parameters = new URL(import.meta.url).searchParams;
const appId = parameters.get('appId');
const triggerSelector = parameters.get('triggerSelector');
if (!appId || !triggerSelector) throw new Error('OWLS marketing bootstrap requires appId and triggerSelector');

const interfacesUrl = `${CDN}/owls-interfaces@${INTERFACES_COMMIT}/index.mjs`;
const loaderUrl = `${CDN}/owls-web-loader@${LOADER_COMMIT}/index.mjs`;
const probeUrl = `${CDN}/owls-web-loader@${LOADER_COMMIT}/fixtures/empty.wasm`;
if (globalThis.__OWLS_INTERFACES_URL__ && globalThis.__OWLS_INTERFACES_URL__ !== interfacesUrl) {
  throw new Error('A different OWLS interface release already owns this document');
}
globalThis.__OWLS_INTERFACES_URL__ = interfacesUrl;
const { Coordinator, RawWasmAdapter, browserPolicy, prepareOnIntent, releaseKey } = await import(loaderUrl);
const emit = (name, detail) => globalThis.dispatchEvent(new CustomEvent(name, { detail }));
const coordinator = new Coordinator(browserPolicy([new URL(probeUrl).origin], {
  maxPrepareBytes: 8, maxAssetBytes: 8, concurrency: 1, timeoutMs: 10_000, activationJoinMs: 25,
}), { report: (event) => emit('ores-wasm-loader:telemetry', { appId, event }) });
const release = coordinator.register({
  schemaVersion: 2,
  appId,
  release: `marketing-probe-${LOADER_COMMIT.slice(0, 12)}`,
  runtime: 'raw-wasm',
  entrypoint: 'probe',
  assets: [{ id: 'probe', url: probeUrl, kind: 'wasm', role: 'module', stage: 'critical', bytes: 8, sha256: PROBE_SHA256, prepare: true }],
  prepareBudget: { maxBytes: 8, maxConcurrency: 1, furthestStage: 'fetch' },
  activation: { mode: 'run-app' },
});
const key = releaseKey(release);
const adapter = new RawWasmAdapter();
const installed = new WeakSet();
const disposers = new Set();
function dispose() { for (const stop of disposers) stop(); disposers.clear(); }
function install() {
  for (const element of document.querySelectorAll(triggerSelector)) {
    if (installed.has(element)) continue;
    installed.add(element);
    disposers.add(prepareOnIntent(element, coordinator, key, {
      dwellMs: 150,
      exitGraceMs: 150,
      onOutcome: (outcome) => emit('ores-wasm-loader:prepared', { appId, outcome }),
      onError: (error) => emit('ores-wasm-loader:error', { appId, phase: 'prepare', name: error?.name ?? 'Error' }),
    }));
  }
}
if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install, { once: true }); else install();
document.addEventListener('astro:before-swap', dispose);
document.addEventListener('astro:page-load', install);
const api = Object.freeze({
  appId, key,
  release: Object.freeze({ interfaces: INTERFACES_COMMIT, loader: LOADER_COMMIT, probeSha256: PROBE_SHA256 }),
  prefetch: (signal) => coordinator.prefetch(key, signal),
  activateProbe: () => coordinator.activate(key, adapter),
  receipt: () => coordinator.receiptFor(key),
  dispose,
});
globalThis.__ORES_WASM_LOADER__ = api;
emit('ores-wasm-loader:ready', { appId, key, release: api.release });
