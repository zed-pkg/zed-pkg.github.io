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

## Repository-local Git worktrees

- Create or use a Git worktree only when the human operator explicitly authorizes it for the current task. Concurrency or a dirty checkout is not permission by itself.
- Put every authorized worktree at `<repository-root>/tmp/worktrees/<name>`; from the repository root, use `./tmp/worktrees/<name>`. Never place worktrees beside repositories or organization directories.
- Keep `tmp`, `temp`, `tmp/worktrees`, and `temp/worktrees` ignored in the repository-root `.gitignore`. Do not commit files from those directories.
- Relocate or remove a worktree only when the operator explicitly requests it. Before removal, preserve and publish intended changes, verify its commit is represented on the target branch, and confirm there are no tracked, untracked, ignored-sensitive, or in-use files that must survive. Remove it with `git worktree remove <path>` without `--force`; never delete a worktree directory with `rm`.
