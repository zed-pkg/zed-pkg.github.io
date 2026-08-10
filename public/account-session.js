const REFRESH_INTERVAL_MS = 50 * 60 * 1000;
const STATUS_PATH = "/auth/session/status";
const REFRESH_PATH = "/auth/session/refresh";
const PERIODIC_SYNC_TAG = "account-session-refresh";

const root = document.documentElement;
const appOrigin = root.dataset.accountOrigin;
const primary = document.querySelector("[data-account-primary]");
const signup = document.querySelector("[data-account-signup]");

let authenticated = null;
let refreshTimer = null;

function renderSessionState(nextAuthenticated) {
  if (!(primary instanceof HTMLAnchorElement) || !(signup instanceof HTMLAnchorElement)) {
    return;
  }

  authenticated = nextAuthenticated;
  if (nextAuthenticated) {
    primary.textContent = "User dashboard";
    primary.href = primary.dataset.dashboardHref || `${appOrigin}/dashboard`;
    primary.setAttribute("aria-label", "Open user dashboard");
    signup.hidden = true;
    root.dataset.accountState = "authenticated";
  } else {
    primary.textContent = "Log in";
    primary.href = primary.dataset.loginHref || `${appOrigin}/login`;
    primary.setAttribute("aria-label", "Log in to your account");
    signup.hidden = false;
    root.dataset.accountState = "anonymous";
  }
}

function scheduleNextCheck(delayMs = REFRESH_INTERVAL_MS) {
  if (refreshTimer !== null) {
    window.clearTimeout(refreshTimer);
  }
  refreshTimer = window.setTimeout(() => {
    void readSession({ refresh: authenticated === true });
  }, delayMs);
}

function safeRefreshDelay(payload) {
  const seconds = Number(payload?.refreshAfterSeconds);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return REFRESH_INTERVAL_MS;
  }
  return Math.min(Math.max(seconds * 1000, 60_000), REFRESH_INTERVAL_MS);
}

async function readSession({ refresh = false } = {}) {
  if (!appOrigin || !(primary instanceof HTMLAnchorElement)) {
    return;
  }

  try {
    const response = await fetch(`${appOrigin}${refresh ? REFRESH_PATH : STATUS_PATH}`, {
      method: refresh ? "POST" : "GET",
      credentials: "include",
      mode: "cors",
      cache: "no-store",
      headers: {
        Accept: "application/json",
      },
    });
    if (!response.ok) {
      scheduleNextCheck();
      return;
    }

    const payload = await response.json();
    if (typeof payload?.authenticated !== "boolean") {
      scheduleNextCheck();
      return;
    }

    renderSessionState(payload.authenticated);
    scheduleNextCheck(safeRefreshDelay(payload));
  } catch {
    // Keep the neutral "Account" state on first-load failures and preserve the
    // last known state thereafter. Authentication must never fail open.
    scheduleNextCheck();
  }
}

async function registerRefreshWorker() {
  if (!("serviceWorker" in navigator)) {
    return;
  }

  try {
    const registration = await navigator.serviceWorker.register("/account-session-sw.js", {
      scope: "/",
    });
    const ready = await navigator.serviceWorker.ready;
    ready.active?.postMessage({ type: "configure-account-session", appOrigin });

    if ("periodicSync" in registration) {
      await registration.periodicSync.register(PERIODIC_SYNC_TAG, {
        minInterval: REFRESH_INTERVAL_MS,
      });
    }
  } catch {
    // Periodic Background Sync is optional. Foreground, focus, visibility, and
    // online recovery remain the guaranteed refresh path.
  }
}

function recoverOnForeground() {
  if (document.visibilityState === "visible") {
    void readSession({ refresh: authenticated === true });
  }
}

window.addEventListener("focus", recoverOnForeground);
window.addEventListener("online", recoverOnForeground);
document.addEventListener("visibilitychange", recoverOnForeground);

void readSession();
void registerRefreshWorker();
