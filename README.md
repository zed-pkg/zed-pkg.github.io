# zed-pkg.github.io

Marketing site for [zed-pkg](https://github.com/zed-pkg), built with Astro
(no Jekyll). Deployed to GitHub Pages by `.github/workflows/deploy.yml` on
every push to `main`.

Brand palette: black `#0A0A0B` / `#050506`, orange `#FF7A1A`, baby blue
`#8FD3F4`. Logo assets live in `public/` (`logo.svg`, `logo-mark.svg`,
`favicon.svg`).

## Develop

```sh
npm install
npm run dev      # http://localhost:4321
npm run build    # static output in dist/
```

## Pages setup (one-time)

Repo Settings -> Pages -> Source: **GitHub Actions**. For the zpkg.tech
custom domain later: add a `public/CNAME` file containing `zpkg.tech`,
switch `site` in `astro.config.mjs`, and point DNS per `zed-infra`.
