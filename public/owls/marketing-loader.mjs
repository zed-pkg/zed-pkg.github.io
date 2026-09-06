const INTERFACES_COMMIT = '231510f5d01046af657be42a5d4215be12622042';
const LOADER_COMMIT = 'deae23537d27aed94bdc2510649f99393379a617';
const PROBE_SHA256 = '93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476';
const PUBLIC_SITE_ORIGIN = 'https://zpkg.net';

const parameters = new URL(import.meta.url).searchParams;
const appId = parameters.get('appId');
const triggerSelector = parameters.get('triggerSelector');
if (!appId || !triggerSelector) throw new Error('OWLS marketing bootstrap requires appId and triggerSelector');

const interfacesUrl = new URL('./vendor/owls-interfaces/index.mjs', import.meta.url).href;
const loaderRoot = new URL('./vendor/owls-web-loader/', import.meta.url);
const probeUrl = new URL('/owls/vendor/owls-web-loader/fixtures/empty.wasm', PUBLIC_SITE_ORIGIN).href;
if (globalThis.__OWLS_INTERFACES_URL__ && globalThis.__OWLS_INTERFACES_URL__ !== interfacesUrl) {
  throw new Error('A different OWLS interface release already owns this document');
}
globalThis.__OWLS_INTERFACES_URL__ = interfacesUrl;

const [coordinatorModule, adapterModule, hintsModule, contractModule] = await Promise.all([
  import(new URL('src/coordinator.mjs', loaderRoot).href),
  import(new URL('src/adapters.mjs', loaderRoot).href),
  import(new URL('src/hints.mjs', loaderRoot).href),
  import(new URL('src/contract.mjs', loaderRoot).href)
]);
const { Coordinator, browserPolicy } = coordinatorModule;
const { RawWasmAdapter } = adapterModule;
const { prepareOnIntent } = hintsModule;
const { releaseKey } = contractModule;
const emit = (name, detail) => globalThis.dispatchEvent(new CustomEvent(name, { detail }));
const coordinator = new Coordinator(browserPolicy([new URL(probeUrl).origin], {
  maxPrepareBytes: 8,
  maxAssetBytes: 8,
  concurrency: 1,
  timeoutMs: 10_000,
  activationJoinMs: 25
}), { report: (event) => emit('ores-wasm-loader:telemetry', { appId, event }) });

const release = coordinator.register({
  schemaVersion: 2,
  appId,
  release: `marketing-probe-${LOADER_COMMIT.slice(0, 12)}`,
  runtime: 'raw-wasm',
  entrypoint: 'probe',
  assets: [{
    id: 'probe',
    url: probeUrl,
    kind: 'wasm',
    role: 'module',
    stage: 'critical',
    bytes: 8,
    sha256: PROBE_SHA256,
    prepare: true
  }],
  prepareBudget: { maxBytes: 8, maxConcurrency: 1, furthestStage: 'fetch' },
  activation: { mode: 'run-app' }
});
const key = releaseKey(release);
const adapter = new RawWasmAdapter();
const installed = new WeakSet();
const disposers = new Set();
function dispose() {
  for (const stop of disposers) stop();
  disposers.clear();
}
function install() {
  for (const element of document.querySelectorAll(triggerSelector)) {
    if (installed.has(element)) continue;
    installed.add(element);
    disposers.add(prepareOnIntent(element, coordinator, key, {
      dwellMs: 150,
      exitGraceMs: 150,
      onOutcome: (outcome) => emit('ores-wasm-loader:prepared', { appId, outcome }),
      onError: (error) => emit('ores-wasm-loader:error', {
        appId,
        phase: 'prepare',
        name: error?.name ?? 'Error'
      })
    }));
  }
}
if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install, { once: true });
else install();
document.addEventListener('astro:before-swap', dispose);
document.addEventListener('astro:page-load', install);
const api = Object.freeze({
  appId,
  key,
  release: Object.freeze({
    interfaces: INTERFACES_COMMIT,
    loader: LOADER_COMMIT,
    probeSha256: PROBE_SHA256
  }),
  prefetch: (signal) => coordinator.prefetch(key, signal),
  activateProbe: () => coordinator.activate(key, adapter),
  receipt: () => coordinator.receiptFor(key),
  dispose
});
globalThis.__ORES_WASM_LOADER__ = api;
emit('ores-wasm-loader:ready', { appId, key, release: api.release });
