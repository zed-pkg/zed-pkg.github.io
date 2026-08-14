#!/usr/bin/env python3
from __future__ import annotations

import json
import threading
from contextlib import contextmanager
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from playwright.sync_api import BrowserContext, Page, Route, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
RESULTS = ROOT / "build" / "site-contract-results"
APP_ORIGIN = "https://app.zpkg.net"
STATUS_URL = f"{APP_ORIGIN}/auth/session/status"
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


def install_session_contract(
    context: BrowserContext,
    *,
    page_origin: str,
    response_state: dict[str, object],
    requests: list[tuple[str, str, dict[str, str]]],
) -> None:
    def fulfill(route: Route) -> None:
        request = route.request
        requests.append((request.method, request.url, request.headers))
        route.fulfill(
            status=int(response_state["status"]),
            content_type="application/json",
            headers={
                "Access-Control-Allow-Origin": page_origin,
                "Access-Control-Allow-Credentials": "true",
                "Cache-Control": "no-store, max-age=0",
                "Vary": "Origin",
            },
            body=json.dumps(
                {
                    "authenticated": response_state["authenticated"],
                    "dashboard_url": f"{APP_ORIGIN}/dashboard",
                    "check_after_seconds": 3000,
                }
            ),
        )

    context.route(STATUS_URL, fulfill)


def assert_common_page_contract(page: Page) -> None:
    page.get_by_role("heading", name="Plan before publish").wait_for()

    cards = page.locator("a.repo[data-repo]")
    assert cards.count() == len(EXPECTED_REPOS)
    actual_repos = {
        cards.nth(index).get_attribute("data-repo") for index in range(cards.count())
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


def assert_anonymous_account(page: Page) -> None:
    account = page.get_by_role("group", name="Account")
    assert account.count() == 1

    login = account.get_by_role("link", name="Log in", exact=True)
    signup = account.get_by_role("link", name="Sign up", exact=True)
    login.wait_for()
    assert login.get_attribute("href") == f"{APP_ORIGIN}/login"
    assert signup.get_attribute("href") == f"{APP_ORIGIN}/signup"
    assert login.is_visible()
    assert signup.is_visible()
    assert page.locator("html").get_attribute("data-account-state") == "anonymous"


def assert_authenticated_account(page: Page) -> None:
    account = page.get_by_role("group", name="Account")
    assert account.count() == 1

    dashboard = account.get_by_role("link", name="User dashboard", exact=True)
    signup = account.locator("[data-account-signup]")
    dashboard.wait_for()
    assert dashboard.get_attribute("href") == f"{APP_ORIGIN}/dashboard"
    assert dashboard.is_visible()
    assert not signup.is_visible()
    assert page.locator("html").get_attribute("data-account-state") == "authenticated"


def static_security_contract() -> None:
    html = (DIST / "index.html").read_text(encoding="utf-8")
    page_client = (ROOT / "public" / "account-session.js").read_text(encoding="utf-8")

    assert "data-account-primary" in html
    assert "Account" in html
    assert "script-src 'self'" in html
    assert f"connect-src 'self' {APP_ORIGIN}" in html
    assert "/account-session.js" in html

    assert 'credentials: "include"' in page_client
    assert 'cache: "no-store"' in page_client
    assert "/auth/session/status" in page_client
    assert "check_after_seconds" in page_client
    assert "dashboard_url" in page_client
    assert "AbortController" in page_client
    assert "checkPromise" in page_client
    assert "Math.random" in page_client

    forbidden = (
        "/auth/session/refresh",
        "serviceWorker",
        "periodicSync",
        "localStorage",
        "sessionStorage",
        "access_token",
        "refresh_token",
        "Authorization",
        "Bearer",
    )
    for value in forbidden:
        assert value not in page_client, value
    assert not (ROOT / "public" / "account-session-sw.js").exists()


def attach_diagnostics(
    context: BrowserContext,
    page: Page,
    console_errors: list[str],
    page_errors: list[str],
    external_requests: list[str],
    base_url: str,
) -> None:
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
        if not request.url.startswith(base_url) and request.url != STATUS_URL
        else None,
    )
    context.tracing.start(screenshots=True, snapshots=True, sources=True)


def main() -> None:
    if not (DIST / "index.html").is_file():
        raise SystemExit("dist/index.html is missing; run npm run build first")

    static_security_contract()
    RESULTS.mkdir(parents=True, exist_ok=True)

    with server() as base_url, sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        for state_name, authenticated in (
            ("anonymous", False),
            ("authenticated", True),
        ):
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            requests: list[tuple[str, str, dict[str, str]]] = []
            response_state: dict[str, object] = {
                "status": 200,
                "authenticated": authenticated,
            }
            install_session_contract(
                context,
                page_origin=base_url,
                response_state=response_state,
                requests=requests,
            )
            page = context.new_page()
            console_errors: list[str] = []
            page_errors: list[str] = []
            external_requests: list[str] = []
            attach_diagnostics(
                context,
                page,
                console_errors,
                page_errors,
                external_requests,
                base_url,
            )

            try:
                with page.expect_request(STATUS_URL) as initial_status:
                    page.goto(base_url, wait_until="networkidle")
                assert initial_status.value.method == "GET"
                assert_common_page_contract(page)
                if authenticated:
                    assert_authenticated_account(page)
                else:
                    assert_anonymous_account(page)

                with page.expect_request(STATUS_URL) as foreground_status:
                    page.evaluate("window.dispatchEvent(new Event('focus'))")
                assert foreground_status.value.method == "GET"

                if authenticated:
                    response_state["status"] = 503
                    with page.expect_request(STATUS_URL) as failed_status:
                        page.evaluate("window.dispatchEvent(new Event('focus'))")
                    assert failed_status.value.method == "GET"
                    page.get_by_role("link", name="Log in", exact=True).wait_for()
                    assert_anonymous_account(page)
                else:
                    assert_anonymous_account(page)

                assert requests
                assert all(method == "GET" and url == STATUS_URL for method, url, _ in requests)
                assert all("authorization" not in headers for _, _, headers in requests)

                page.set_viewport_size({"width": 390, "height": 844})
                assert page.evaluate(
                    "document.documentElement.scrollWidth <= window.innerWidth"
                )
                assert page.locator("#repos").is_visible()
                assert page.locator("#review").is_visible()
                if authenticated:
                    assert_anonymous_account(page)
                else:
                    assert_anonymous_account(page)

                assert not external_requests, external_requests
                if authenticated:
                    assert console_errors, "the deliberate 503 was not observed"
                    assert all(
                        "503 (Service Unavailable)" in message
                        for message in console_errors
                    ), console_errors
                else:
                    assert not console_errors, console_errors
                assert not page_errors, page_errors
                context.tracing.stop()
            except BaseException:
                page.screenshot(
                    path=RESULTS / f"{state_name}-failure.png",
                    full_page=True,
                )
                context.tracing.stop(path=RESULTS / f"{state_name}-trace.zip")
                raise
            finally:
                context.close()
        browser.close()

    print("zed-pkg public-site session contract passed")


if __name__ == "__main__":
    main()
