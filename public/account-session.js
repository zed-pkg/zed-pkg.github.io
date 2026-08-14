const DEFAULT_CHECK_INTERVAL_MS = 50 * 60 * 1000;
const FAILURE_RETRY_INTERVAL_MS = 5 * 60 * 1000;
const MIN_CHECK_INTERVAL_MS = 60 * 1000;
const REQUEST_TIMEOUT_MS = 10 * 1000;
const STATUS_PATH = "/auth/session/status";

const root = document.documentElement;
const appOrigin = root.dataset.accountOrigin;
const primary = document.querySelector("[data-account-primary]");
const signup = document.querySelector("[data-account-signup]");

let checkTimer = null;
let checkPromise = null;

function linksAreAvailable() {
  return primary instanceof HTMLAnchorElement && signup instanceof HTMLAnchorElement;
}

function renderSessionState(nextAuthenticated, dashboardHref) {
  if (!linksAreAvailable()) {
    return;
  }

  if (nextAuthenticated) {
    primary.textContent = "User dashboard";
    primary.href = dashboardHref;
    primary.setAttribute("aria-label", "User dashboard");
    signup.hidden = true;
    root.dataset.accountState = "authenticated";
  } else {
    primary.textContent = "Log in";
    primary.href = primary.dataset.loginHref || `${appOrigin}/login`;
    primary.setAttribute("aria-label", "Log in");
    signup.hidden = false;
    root.dataset.accountState = "anonymous";
  }
}

function scheduleNextCheck(delayMs) {
  if (checkTimer !== null) {
    window.clearTimeout(checkTimer);
  }
  checkTimer = window.setTimeout(() => {
    void readSession();
  }, delayMs);
}

function jitteredDelay(baseDelayMs) {
  return Math.round(baseDelayMs * (1 + Math.random() * 0.1));
}

function safeCheckDelay(payload) {
  const seconds = Number(payload?.check_after_seconds);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return jitteredDelay(DEFAULT_CHECK_INTERVAL_MS);
  }
  const bounded = Math.min(
    Math.max(seconds * 1000, MIN_CHECK_INTERVAL_MS),
    DEFAULT_CHECK_INTERVAL_MS,
  );
  return jitteredDelay(bounded);
}

function safeDashboardHref(payload) {
  const fallback = primary.dataset.dashboardHref || `${appOrigin}/dashboard`;
  if (typeof payload?.dashboard_url !== "string") {
    return fallback;
  }

  try {
    const candidate = new URL(payload.dashboard_url);
    const expected = new URL(fallback);
    if (
      candidate.origin === expected.origin &&
      candidate.pathname === expected.pathname &&
      candidate.search === "" &&
      candidate.hash === "" &&
      candidate.username === "" &&
      candidate.password === ""
    ) {
      return candidate.href;
    }
  } catch {
    // A malformed or non-canonical destination falls back to static navigation.
  }
  return fallback;
}

async function performSessionCheck() {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

  try {
    const response = await fetch(`${appOrigin}${STATUS_PATH}`, {
      method: "GET",
      credentials: "include",
      mode: "cors",
      cache: "no-store",
      headers: {
        Accept: "application/json",
      },
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`session status returned ${response.status}`);
    }

    const payload = await response.json();
    if (typeof payload?.authenticated !== "boolean") {
      throw new TypeError("session status omitted authenticated state");
    }

    renderSessionState(payload.authenticated, safeDashboardHref(payload));
    scheduleNextCheck(safeCheckDelay(payload));
  } catch {
    // Network and server failures must not preserve stale authenticated UI.
    renderSessionState(false, `${appOrigin}/dashboard`);
    scheduleNextCheck(jitteredDelay(FAILURE_RETRY_INTERVAL_MS));
  } finally {
    window.clearTimeout(timeout);
  }
}

function readSession() {
  if (!appOrigin || !linksAreAvailable()) {
    return Promise.resolve();
  }
  if (checkPromise !== null) {
    return checkPromise;
  }

  checkPromise = performSessionCheck().finally(() => {
    checkPromise = null;
  });
  return checkPromise;
}

function recoverOnForeground() {
  if (document.visibilityState === "visible") {
    void readSession();
  }
}

window.addEventListener("focus", recoverOnForeground);
window.addEventListener("online", recoverOnForeground);
window.addEventListener("pageshow", recoverOnForeground);
document.addEventListener("visibilitychange", recoverOnForeground);

void readSession();
