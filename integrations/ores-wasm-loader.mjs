function normalizedBase(value = '/') {
  const trimmed = String(value).trim();
  if (!trimmed || trimmed === '/') return '/';
  return `/${trimmed.replace(/^\/+|\/+$/g, '')}/`;
}

export function oresWasmLoader({ appId, triggerSelector } = {}) {
  if (typeof appId !== 'string' || !/^[a-z][a-z0-9-]{1,62}$/.test(appId)) {
    throw new TypeError('oresWasmLoader appId must be a stable lowercase identifier');
  }
  if (typeof triggerSelector !== 'string' || triggerSelector.length === 0) {
    throw new TypeError('oresWasmLoader triggerSelector must be a non-empty CSS selector');
  }
  return {
    name: `ores-wasm-loaders/marketing-${appId}`,
    hooks: {
      'astro:config:setup': ({ config, injectScript }) => {
        const query = new URLSearchParams({ appId, triggerSelector });
        const moduleUrl = `${normalizedBase(config.base)}owls/marketing-loader.mjs?${query}`;
        injectScript('page', `const key = Symbol.for('ores.wasm-loader.marketing.bootstrap.v1');
          globalThis[key] ??= import(/* @vite-ignore */ ${JSON.stringify(moduleUrl)})
            .catch((error) => {
              globalThis.dispatchEvent(new CustomEvent('ores-wasm-loader:error', {
                detail: { phase: 'bootstrap', name: error?.name ?? 'Error' },
              }));
              return null;
            });`);
      },
    },
  };
}
