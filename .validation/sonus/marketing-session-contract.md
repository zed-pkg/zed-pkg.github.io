# Marketing session status and refresh

`sonusauris.app` is static. Authentication and session storage terminate at the
Rust BFF on `user.sonusauris.app`. The marketing site calls two exact-origin
endpoints with `credentials: include`:

- `GET /auth/session/status` returns only
  `{ "authenticated": boolean, "refreshAfterSeconds": 3000 }`.
- `POST /auth/session/refresh` rotates the Supabase access/refresh pair inside
  the encrypted database-backed browser session and returns the same token-blind
  shape.

Neither response includes a JWT, refresh token, principal, email, tenant, or
account metadata. Responses are `no-store`; credentialed CORS is emitted only
for `https://sonusauris.app`. The existing same-origin gate remains in force for
every other mutation and WebSocket handshake.

The browser performs an initial status read, refreshes authenticated sessions at
the 50-minute mark, and recovers on focus, visibility, and online events.
Periodic Background Sync is best effort only. Mobile clients should use the same
50-minute target when the operating system grants background execution, and
must always refresh on foreground/resume.

This preserves the current Supabase-backed BFF session. Shared Auth customer-
realm PKCE remains the target identity/session authority and should replace the
direct Supabase ceremony only after the exact Sonus client registration and
cutover canaries are deployed.
