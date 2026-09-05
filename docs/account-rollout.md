# Customer account handoff and promotion

Tracking: [DEN-3970](https://linear.app/denman/issue/DEN-3970) and
[platform delivery issue](https://github.com/zed-pkg/.github/issues/61).

## Current boundary

`/account/` and the homepage offer individual and organization journeys without
collecting credentials, identity fields, or membership data. Both journeys use
the **customer** application. Workspace ownership is product authorization, not
access to the independent platform-admin realm. Never authorize from an email
domain or route marketing users to the admin application.

`src/lib/account-links.ts` owns the exact handoff links and the promotion switch.
They match `zed-web-server.rs@0251593c9f0d44d75cfa2544349a2ef9c2f89db7`,
[PR #48](https://github.com/zed-pkg/zed-web-server.rs/pull/48):

| Journey | Canonical sign-in | Signed-in setup |
| --- | --- | --- |
| Individual | `/auth/sign-in?return_to=%2Fonboarding%2Findividual` | `/onboarding/individual` |
| Organization | `/auth/sign-in?return_to=%2Fonboarding%2Forganization` | `/onboarding/organization` |

All four destinations are under `https://app.zpkg.net`. The generic `/login`
and `/signup` aliases currently use a fixed dashboard return path; appending a
journey query to those aliases does **not** preserve it. Do not add a separate
signup ceremony or pass credentials through the marketing origin.

## Deployment evidence, not source-only promotion

The 2026-09-05 public GET probe found `/`, `/healthz`, `/auth/sign-in`,
`/auth/session/status`, and `/onboarding` returning 404; `/login` and `/signup`
returned 503. Both onboarding journey paths also returned 404. This is evidence
that the public route is not serving the merged application; it does not by
itself identify which proxy, origin, or rollout is responsible.

`HOSTED_ONBOARDING_ENABLED` therefore remains **false**. The marketing page
renders a clear rollout notice and non-interactive sign-in labels, with usable
CLI/documentation alternatives. Publishing this page is not a claim that hosted
onboarding is operational. Do not automatically enable it from a green build,
DNS resolution, a 200 homepage, or a successful health check alone.

Promote in a reviewed change only after all of these are recorded on DEN-3970:

1. The public customer application serves the reviewed Rust image/revision.
   Reconcile the Cloudflare app proxy, origin, GitOps pins, and deployment before
   changing the marketing switch. Do not weaken auth, TLS, or network isolation
   to make a probe pass.
2. Both anonymous onboarding pages render a usable customer sign-in action;
   their headers include `private, no-store`. Test GET, not only HEAD.
3. Shared Auth's registered customer client allowlists **both** local return
   paths. Its generic example lists `/`, `/dashboard`, and `/settings`, which
   does not authorize either onboarding path. Keep exact redirect URI, audience,
   client and realm boundaries; never add wildcard return paths.
4. A controlled synthetic browser completes PKCE and returns to the selected
   individual or organization journey. Verify existing membership, empty
   membership, duplicate namespace, permission denial, rotation, and outage
   paths against the real API/database. No production personal data or secrets
   belong in evidence artifacts.
5. The token-blind session-status endpoint accepts only the exact marketing
   origin, sends credentialed CORS and no-store headers, and returns a coarse
   boolean only on a successful authority decision. An unavailable authority
   must remain unavailable, never an anonymous or authenticated success.
6. Run all commands below and the browser contract on the promoted build;
   verify both sign-in links and both signed-in setup links, mobile containment,
   keyboard navigation, no-JavaScript fallback, and no admin/foreign redirect.

Rollback disables the promotion switch and republishes the marketing site.
It does not revoke sessions, modify membership, or roll back database state.

## Session-hint state machine

`public/account-state.js` has an exhaustively type-checked presentation map:

| Evidence | State | Primary navigation |
| --- | --- | --- |
| Initial page or check in progress | `unknown` | Local `/account/` |
| Successful response with boolean false | `anonymous` | Customer `/login` |
| Successful response with boolean true | `authenticated` | Customer `/dashboard` |
| Error, redirect, timeout, malformed or oversized response | `unavailable` | Local `/account/` |

Every fresh check clears stale presentation. Failure does not claim logout.
Both journey choices remain visible in every state. This map is a convenience,
never an authorization gate: the destination application checks authority again.

The effect layer pins the origin, uses only a credentialed GET, refuses redirects,
bounds JSON to 2 KiB and the request to 10 seconds, deduplicates concurrent
checks, and bounds/jitters retry timing. No response-supplied URL becomes
navigation. It does not store session material or register a service worker.

## Validation

Use the pinned Node 24.19.0 runtime. The migration from Astro 5.18.2 to 7.3.1
follows the official [Astro 6](https://docs.astro.build/en/guides/upgrade-to/v6/)
and [Astro 7](https://docs.astro.build/en/guides/upgrade-to/v7/) guides. This site
uses no server adapter, content collections, Markdown plugins, or hydrated
framework islands. `compressHTML: true` preserves the previous HTML-aware
spacing. The audit reported six affected packages before the upgrade and zero
after it on 2026-09-05; this is a dependency scan, not a proof of no security bugs.

PR validation and the Pages publication build both run the locked install,
dependency audit, type checks, and contract tests. Deployment actions are pinned
and only the final Pages job receives write permissions. The existing live-site
smoke check additionally requires `/account/` after deployment (not against the
old public site while the PR is still under review).

```sh
npm ci --no-audit --no-fund
npm audit --audit-level=low
npm run check
npm run test:unit
npm run build
python3 tests/marketing_pages_contract.py
.venv-site-contract/bin/python tests/site_contract.py
```

The browser contract exercises 14 failure classes followed by recovery from
both initially anonymous and initially authenticated states, as well as real
fetch/body handling, real timeout and concurrent-check deduplication, keyboard
anchors, narrow viewports, and JavaScript-disabled
navigation. Pure tests cover every presentation variant, malformed field types,
retry bounds, and untrusted destination hints. These are finite conformance
tests, not a new formal proof of Shared Auth or hosted deployment. The Rust
mutation model and implementation trace replay remain in `zed-web-server.rs`.

The existing Python Playwright/HTML harness is retained for continuity while
this UI contract changes. A future Rust contract checker should own static
route/link/promotion checks, preserve these positive/negative fixtures, and
leave browser rendering to the established browser driver; migration belongs
to DEN-3970's cross-repository lint/contract-parity work.
