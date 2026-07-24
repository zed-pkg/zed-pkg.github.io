import { defineConfig } from "astro/config";

// Served at https://zed-pkg.github.io via GitHub Pages. When the zpkg.tech
// custom domain is wired up (CNAME + DNS in zed-infra), switch `site` to
// https://zpkg.tech and add a public/CNAME file containing `zpkg.tech`.
export default defineConfig({
  site: "https://zed-pkg.github.io",
});
