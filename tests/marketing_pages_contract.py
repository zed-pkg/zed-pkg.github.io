#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
from html.parser import HTMLParser
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"

PAGES = {
    "account": (
        "zed-pkg accounts — individuals and organizations",
        "Start solo. Build together.",
    ),
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
    "/account/",
)


class Links(HTMLParser):
    def __init__(self, html: str):
        super().__init__()
        self.hrefs: list[str] = []
        self.ids: set[str] = set()
        self.journey_signins: list[str] = []
        self.journey_setups: list[str] = []
        self.feed(html)

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        fields = dict(attrs)
        if "id" in fields:
            element_id = fields["id"]
            assert element_id and element_id not in self.ids, element_id
            self.ids.add(element_id)
        if tag == "a":
            href = fields.get("href")
            assert href, fields
            self.hrefs.append(href)
            if "data-journey-sign-in" in fields:
                self.journey_signins.append(href)
            if "data-journey-setup" in fields:
                self.journey_setups.append(href)


def account_and_link_contract() -> None:
    pages = {"/": (DIST / "index.html").read_text(encoding="utf-8")}
    pages.update({f"/{slug}/": page_html(slug) for slug in PAGES})
    parsed = {path: Links(html) for path, html in pages.items()}
    for path, links in parsed.items():
        for href in links.hrefs:
            url = urlsplit(href)
            if url.scheme or url.netloc:
                continue
            target = url.path or path
            if target not in parsed:
                assert (DIST / target.lstrip("/")).is_file(), (path, href)
                continue
            if url.fragment:
                assert unquote(url.fragment) in parsed[target].ids, (path, href)

    account = pages["/account/"]
    links = parsed["/account/"]
    assert {"individual", "organization"} <= links.ids
    assert "company email domain alone never grants access" in account
    assert "separate platform-administration system" in account
    assert "<form" not in account
    if 'data-account-rollout="pending-deployment"' in account:
        assert not links.journey_signins and not links.journey_setups
        assert account.count("data-journey-pending") == 2
    else:
        assert sorted(links.journey_signins) == [
            f"https://app.zpkg.net/auth/sign-in?return_to=%2Fonboarding%2F{journey}"
            for journey in ("individual", "organization")
        ]
        assert sorted(links.journey_setups) == [
            f"https://app.zpkg.net/onboarding/{journey}"
            for journey in ("individual", "organization")
        ]


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
    account_and_link_contract()
    print("zed-pkg marketing pages contract passed")


if __name__ == "__main__":
    main()
