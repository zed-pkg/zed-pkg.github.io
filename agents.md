# Agent instructions

## Scope and hierarchy

- These instructions apply to the whole `zed-pkg/zed-pkg.github.io` repository unless a deeper lowercase `agents.md` adds narrower rules.
- Before editing, resolve the current working directory and load every readable ancestor `agents.md` from the filesystem root to the working directory. Do not search siblings. Resolve symlinks, deduplicate resolved files, and report unreadable or cyclic instruction files.
- `.claude/CLAUDE.md`, `.gemini/GEMINI.md`, and `.openai/AGENTS.md` are pointers only. Never duplicate instructions in tool-specific files.

## Repository role

This repository builds and publishes the public Zed website. It communicates product capabilities, installation paths, documentation links, project status, and trust information without becoming a second source of technical contracts.

## Working rules

- Verify product and compatibility claims against released code, documentation, or tracked roadmap status. Do not present planned work as generally available.
- Keep installation commands, repository links, package names, support status, and pricing or policy text exact and current.
- Preserve semantic HTML, keyboard access, readable contrast, responsive layouts, reduced-motion behavior, and fast static delivery.
- Minimize client-side JavaScript, third-party trackers, remote assets, and data collection; document any intentional analytics or external requests.
- Keep SEO metadata, canonical URLs, sitemaps, redirects, social previews, and structured data synchronized with page content.
- Optimize and attribute media; do not commit private images, personal data, internal screenshots, or licensed assets without provenance.
- Never commit deployment credentials, analytics secrets, API tokens, private endpoints, or production environment files.
- Run formatting, linting, type checks, static builds, link checks, and available accessibility/performance checks before review.

## Validation

The pinned `agents policy` workflow validates this hierarchy and the three tool pointers. Follow `README.md` and existing site/deployment CI before requesting review.
