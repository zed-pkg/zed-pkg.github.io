import assert from "node:assert/strict";
import test from "node:test";
import {
  accountPresentation,
  parseSessionHint,
  DEFAULT_CHECK_INTERVAL_MS,
} from "../public/account-state.js";

test("only a definite authenticated hint selects the fixed customer dashboard", () => {
  for (const state of ["unknown", "anonymous", "authenticated", "unavailable"]) {
    const presentation = accountPresentation(state);
    assert.equal(presentation.href.endsWith("/dashboard"), state === "authenticated");
    if (state === "unknown" || state === "unavailable") {
      assert.deepEqual(presentation, { label: "Account", href: "/account/" });
    }
  }
  assert.throws(() => accountPresentation("admin"), TypeError);
});

test("both boolean outcomes ignore every supplied navigation destination", () => {
  for (const authenticated of [false, true]) {
    for (const dashboard_url of [
      "https://evil.invalid/dashboard", "//evil.invalid", "javascript:alert(1)",
      "https://app.zpkg.net/admin", "https://app.zpkg.net/dashboard?next=evil",
      "https://app.zpkg.net/dashboard#fragment", "https://admin.zpkg.net/",
      "https://someone@app.zpkg.net/dashboard", null, {}, [],
    ]) {
      const hint = parseSessionHint({ authenticated, dashboard_url });
      assert.equal(hint.state, authenticated ? "authenticated" : "anonymous");
      assert.equal(accountPresentation(hint.state).href,
        `https://app.zpkg.net/${authenticated ? "dashboard" : "login"}`);
    }
  }
});

test("malformed and coercible authentication fields never become anonymous or authenticated", () => {
  for (const payload of [null, undefined, false, true, "true", 1, [], {},
    { authenticated: 0 }, { authenticated: 1 }, { authenticated: "false" },
    { authenticated: null }, { authenticated: [] }, { authenticated: {} }]) {
    assert.throws(() => parseSessionHint(payload), TypeError);
  }
});

test("server timing remains bounded without string, boolean, or null coercion", () => {
  for (const check_after_seconds of [undefined, null, false, true, "60", {}, [], -1, 0, NaN, Infinity]) {
    assert.equal(parseSessionHint({ authenticated: false, check_after_seconds }).checkAfterMs,
      DEFAULT_CHECK_INTERVAL_MS);
  }
  for (const [seconds, expected] of [[0.1, 60_000], [60, 60_000], [90, 90_000],
    [3000, DEFAULT_CHECK_INTERVAL_MS], [Number.MAX_VALUE, DEFAULT_CHECK_INTERVAL_MS]]) {
    assert.equal(parseSessionHint({ authenticated: true, check_after_seconds: seconds }).checkAfterMs, expected);
  }
});
