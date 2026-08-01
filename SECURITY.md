# Security policy

## Reporting

Do not publish exploit details, credentials, signed URLs, customer data, or
private repository information in a public issue.

Use the affected repository's **Security** tab and **Report a vulnerability**
when private vulnerability reporting is available. For a cross-repository
issue, start with the repository that owns the vulnerable runtime or contract
and identify the other affected components in that private report.

When no private reporting entry is shown, open a minimal public issue asking
maintainers for a private contact channel without including technical details.

A useful report includes affected versions or commits, impact, reproduction
steps, expected and observed behavior, and any proposed mitigation. Redact
bearer tokens, cookies, deployment credentials, presigned query strings,
personal data, and unrelated logs.

## Supported surface

Security fixes target the default branch and the next appropriate release. The
public site must not load third-party runtime scripts, fonts, analytics, or
other assets that silently expand the trust boundary. Repository and SDK claims
are verified against the reviewed source inventory through browser CI.

## Coordinated disclosure

Please allow maintainers to reproduce the issue, patch the owning component,
validate dependent repositories, and prepare release or deployment guidance
before public disclosure.
