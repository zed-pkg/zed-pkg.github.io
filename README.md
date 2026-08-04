# zed-pkg.github.io

Marketing site for [zed-pkg](https://github.com/zed-pkg), built with Astro
(no Jekyll). Deployed to GitHub Pages by `.github/workflows/deploy.yml` on
every push to `main`.

Brand palette: black `#0A0A0B` / `#050506`, orange `#FF7A1A`, baby blue
`#8FD3F4`. Logo assets live in `public/` (`logo.svg`, `logo-mark.svg`,
`favicon.svg`).

## Develop

```sh
npm ci
npm run dev      # http://localhost:4321
npm run build    # static output in dist/
```

## Public claim contract

The homepage names all eleven maintained zed-pkg repositories, the reviewed
ten-language SDK matrix, and the credential-free release-plan review flow.
`.github/workflows/site-contract.yml` builds the locked production site and
uses Playwright Chromium to verify those claims, keyboard focus, responsive
containment, console/page errors, and the absence of external runtime requests.

Local browser verification requires Python Playwright:

```sh
npm run build
python3 -m venv .venv-site-contract
.venv-site-contract/bin/pip install playwright==1.55.0
.venv-site-contract/bin/playwright install chromium
.venv-site-contract/bin/python tests/site_contract.py
```

## Pages setup (one-time)

Repo Settings -> Pages -> Source: **GitHub Actions**. For the zpkg.tech
custom domain later: add a `public/CNAME` file containing `zpkg.tech`,
switch `site` in `astro.config.mjs`, and point DNS per `zed-infra`.

## Governance

MIT licensed; see [LICENSE](LICENSE). Report suspected vulnerabilities using
[SECURITY.md](SECURITY.md) and keep exploit details and credentials out of
public issues.
