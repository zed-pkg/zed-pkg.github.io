#!/usr/bin/env python3
from __future__ import annotations

import threading
from contextlib import contextmanager
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
RESULTS = ROOT / "build" / "site-contract-results"
EXPECTED_REPOS = {
    "zed-cli",
    "zed-interfaces",
    "zed-api-server.rs",
    "zed-web-server.rs",
    "zed-clients",
    "zed-sync",
    "zed-infra",
    "zed-docs",
    "zed-e2e",
    "zed-monorepo",
    "zed-pkg.github.io",
}
EXPECTED_SDKS = (
    "Rust",
    "browser WebAssembly",
    "TypeScript",
    "Python",
    "Go",
    "Dart",
    "Gleam",
    "Erlang/OTP",
    "Java",
    "Swift",
)


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return


@contextmanager
def server():
    handler = lambda *args, **kwargs: QuietHandler(*args, directory=DIST, **kwargs)
    httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{httpd.server_port}"
    finally:
        httpd.shutdown()
        thread.join(timeout=5)
        httpd.server_close()


def main() -> None:
    if not (DIST / "index.html").is_file():
        raise SystemExit("dist/index.html is missing; run npm run build first")

    RESULTS.mkdir(parents=True, exist_ok=True)
    with server() as base_url, sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        context = browser.new_context(viewport={"width": 1280, "height": 900})
        context.tracing.start(screenshots=True, snapshots=True, sources=True)
        page = context.new_page()
        console_errors: list[str] = []
        page_errors: list[str] = []
        external_requests: list[str] = []

        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "request",
            lambda request: external_requests.append(request.url)
            if not request.url.startswith(base_url)
            else None,
        )

        try:
            page.goto(base_url, wait_until="networkidle")
            page.get_by_role("heading", name="Plan before publish").wait_for()

            cards = page.locator("a.repo[data-repo]")
            assert cards.count() == len(EXPECTED_REPOS)
            actual_repos = {
                cards.nth(index).get_attribute("data-repo")
                for index in range(cards.count())
            }
            assert actual_repos == EXPECTED_REPOS, (actual_repos, EXPECTED_REPOS)

            sdk_claim = page.locator("[data-sdk-matrix]").inner_text()
            for sdk in EXPECTED_SDKS:
                assert sdk in sdk_claim, f"SDK claim is missing {sdk}: {sdk_claim}"

            assert "zed release plan --json" in page.locator("#review").inner_text()
            assert "self-contained offline HTML" in page.locator("#review").inner_text()

            first_repo = cards.first
            first_repo.focus()
            assert page.evaluate("document.activeElement.dataset.repo") == "zed-cli"

            page.set_viewport_size({"width": 390, "height": 844})
            assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
            assert page.locator("#repos").is_visible()
            assert page.locator("#review").is_visible()

            assert not external_requests, external_requests
            assert not console_errors, console_errors
            assert not page_errors, page_errors
            context.tracing.stop()
        except BaseException:
            page.screenshot(path=RESULTS / "failure.png", full_page=True)
            context.tracing.stop(path=RESULTS / "trace.zip")
            raise
        finally:
            context.close()
            browser.close()

    print("zed-pkg public-site contract passed")


if __name__ == "__main__":
    main()
