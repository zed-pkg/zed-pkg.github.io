import { defineConfig } from "astro/config";

// Served at https://zpkg.net (GitHub Pages custom domain; public/CNAME).
// DNS lives in zed-infra/terraform/cloudflare — see docs/dns-zpkg-net.md
// there for the cutover runbook. registry.zpkg.net is the API,
// web.zpkg.net the registry UI; zpkg.tech is parked.
export default defineConfig({
  site: "https://zpkg.net",
});
