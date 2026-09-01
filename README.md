# zed-pkg.github.io

Marketing site for [zed-pkg](https://github.com/zed-pkg), built with Astro
(no Jekyll). Deployed to GitHub Pages by `.github/workflows/deploy.yml` on
every push to `main`.

Zed supplements ecosystem-native package managers; it does not replace them.

The public host contract is intentionally split by responsibility:

- `zpkg.net` is this human-facing site;
- `api.zpkg.net` owns authenticated package and control-plane writes;
- `registry.zpkg.net` serves public package and version metadata reads;
- `cdn.zpkg.net` serves immutable artifact bytes;
- `app.zpkg.net` is the canonical authenticated browser UI; and
- `user.zpkg.net`, when enabled, is a permanent redirect to `app.zpkg.net`.

Availability and promotion are governed by the organization [public registry
reliability contract](https://github.com/zed-pkg/.github/blob/main/docs/PUBLIC_REGISTRY_RELIABILITY.md).
Site copy must not imply that an endpoint is operational merely because its
planned public name appears here.

The public information architecture has dedicated guides at
`/getting-started/`, `/architecture/`, `/reliability/`, `/security/`, and
`/self-hosting/`. Route-specific canonical metadata and the generated sitemap
are checked during the production build.

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
`tests/marketing_pages_contract.py` separately verifies every guide, canonical
URL, primary navigation target, reliability marker, sitemap entry, and robots
declaration.

Local browser verification requires Python Playwright:

```sh
npm run build
python3 -m venv .venv-site-contract
.venv-site-contract/bin/pip install playwright==1.55.0
.venv-site-contract/bin/playwright install chromium
.venv-site-contract/bin/python tests/site_contract.py
python3 tests/marketing_pages_contract.py
```

## Pages setup (one-time)

Repo Settings -> Pages -> Source: **GitHub Actions**. The custom domain is
`zpkg.net` (`public/CNAME`, `site` in `astro.config.mjs`); DNS and the
cutover runbook live in `zed-infra` (`docs/dns-zpkg-net.md`). After DNS
resolves, set the domain in Settings -> Pages (or
`gh api -X PUT repos/zed-pkg/zed-pkg.github.io/pages -f cname=zpkg.net`)
and enable **Enforce HTTPS** once GitHub issues the certificate.

## Governance

MIT licensed; see [LICENSE](LICENSE). Report suspected vulnerabilities using
[SECURITY.md](SECURITY.md) and keep exploit details and credentials out of
public issues.
