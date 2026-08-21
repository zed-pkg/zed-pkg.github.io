import { defineConfig } from "astro/config";

// Served at https://zpkg.net (GitHub Pages custom domain; public/CNAME).
// DNS lives in zed-infra/terraform/cloudflare — see docs/dns-zpkg-net.md
// there for the cutover runbook. api.zpkg.net is the API,
// registry.zpkg.net the artifact surface, and app.zpkg.net the browser UI.
export default defineConfig({
  site: "https://zpkg.net",
});
