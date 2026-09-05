import { accountPresentation, parseSessionHint, FAILURE_RETRY_INTERVAL_MS } from "./account-state.js";

const REQUEST_TIMEOUT_MS = 10 * 1000;
const MAX_RESPONSE_BYTES = 2048;
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

function renderSessionState(state) {
  if (!linksAreAvailable()) {
    return;
  }

  const presentation = accountPresentation(state);
  primary.textContent = presentation.label;
  primary.href = presentation.href;
  primary.setAttribute("aria-label", presentation.label);
  // Both onboarding journeys remain discoverable for existing accounts too.
  signup.hidden = false;
  root.dataset.accountState = state;
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

async function boundedPayload(response) {
  if (!response.body || response.headers.get("content-type")?.split(";")[0].trim() !== "application/json") {
    throw new TypeError("Session status is not JSON");
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let bytes = 0;
  let json = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > MAX_RESPONSE_BYTES) throw new RangeError("Session status exceeded its bound");
      json += decoder.decode(value, { stream: true });
    }
    return JSON.parse(json + decoder.decode());
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
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
      redirect: "error",
      headers: {
        Accept: "application/json",
      },
      signal: controller.signal,
    });
    if (response.status !== 200) {
      throw new Error(`session status returned ${response.status}`);
    }

    const hint = parseSessionHint(await boundedPayload(response));
    renderSessionState(hint.state);
    scheduleNextCheck(jitteredDelay(hint.checkAfterMs));
  } catch {
    // An outage is not a logout. Remove stale authenticated presentation without
    // pretending the customer authority returned a definite anonymous result.
    renderSessionState("unavailable");
    scheduleNextCheck(jitteredDelay(FAILURE_RETRY_INTERVAL_MS));
  } finally {
    window.clearTimeout(timeout);
    controller.abort();
  }
}

function readSession() {
  if (appOrigin !== "https://app.zpkg.net" || !linksAreAvailable()) {
    return Promise.resolve();
  }
  if (checkPromise !== null) {
    return checkPromise;
  }

  renderSessionState("unknown");
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
