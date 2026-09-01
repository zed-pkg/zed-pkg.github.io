#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"

PAGES = {
    "getting-started": (
        "Get started with zed-pkg",
        "From repository to verified package.",
    ),
    "architecture": (
        "zed-pkg architecture",
        "Small services. Explicit trust boundaries.",
    ),
    "reliability": (
        "zed-pkg reliability and fallback",
        "A registry outage should not erase public packages.",
    ),
    "security": (
        "zed-pkg security model",
        "Verify identity before convenience.",
    ),
    "self-hosting": (
        "Self-host zed-pkg",
        "Run the registry where your packages live.",
    ),
}

NAV_LINKS = (
    "/getting-started/",
    "/architecture/",
    "/reliability/",
    "/security/",
    "/self-hosting/",
)


def page_html(slug: str) -> str:
    path = DIST / slug / "index.html"
    if not path.is_file():
        raise AssertionError(f"missing built marketing page: {path}")
    return path.read_text(encoding="utf-8")


def main() -> None:
    if not (DIST / "index.html").is_file():
        raise SystemExit("dist is missing; run npm run build first")

    for slug, (title, heading) in PAGES.items():
        html = page_html(slug)
        canonical = f"https://zpkg.net/{slug}/"
        assert f"<title>{title}</title>" in html, slug
        assert heading in html, slug
        assert f'<link rel="canonical" href="{canonical}">' in html, slug
        assert f'<meta property="og:url" content="{canonical}">' in html, slug
        assert '<meta name="description"' in html, slug
        assert '<script type="module" src="/account-session.js"></script>' in html, slug
        assert 'role="group" aria-label="Account"' in html, slug
        for href in NAV_LINKS:
            assert f'href="{href}"' in html, (slug, href)

    reliability = page_html("reliability")
    for marker in (
        "x-zed-source: github-public",
        "x-zed-edge: cdn",
        "x-zed-source: github-release",
        "Live certification",
        "product-path canary",
    ):
        assert marker in reliability, marker

    architecture = page_html("architecture")
    for hostname in (
        "zpkg.net",
        "app.zpkg.net",
        "api.zpkg.net",
        "registry.zpkg.net",
        "cdn.zpkg.net",
    ):
        assert hostname in architecture, hostname

    sitemap = (DIST / "sitemap.xml").read_text(encoding="utf-8")
    for slug in PAGES:
        assert f"https://zpkg.net/{slug}/" in sitemap, slug

    robots = (DIST / "robots.txt").read_text(encoding="utf-8")
    assert "Sitemap: https://zpkg.net/sitemap.xml" in robots
    print("zed-pkg marketing pages contract passed")


if __name__ == "__main__":
    main()
