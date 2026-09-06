import { defineConfig } from "astro/config";
import { oresWasmLoader } from "./integrations/ores-wasm-loader.mjs";

// Served at https://zpkg.net (GitHub Pages custom domain; public/CNAME).
// DNS lives in zed-infra/terraform/cloudflare — see docs/dns-zpkg-net.md.
export default defineConfig({
  site: "https://zpkg.net",
  compressHTML: true,
  integrations: [
    oresWasmLoader({
      appId: "zed-pkg",
      triggerSelector: 'a[href="/account/"],a[href^="https://app.zpkg.net"]',
    }),
  ],
});
