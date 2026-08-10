const APP_ORIGIN = "https://app.zpkg.net";
const REFRESH_PATH = "/auth/session/refresh";
const PERIODIC_SYNC_TAG = "account-session-refresh";

async function refreshSession() {
  try {
    await fetch(`${APP_ORIGIN}${REFRESH_PATH}`, {
      method: "POST",
      credentials: "include",
      mode: "cors",
      cache: "no-store",
      headers: {
        Accept: "application/json",
      },
    });
  } catch {
    // Background delivery is best effort. The page refreshes again on focus,
    // visibility, online recovery, and its foreground timer.
  }
}

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("periodicsync", (event) => {
  if (event.tag === PERIODIC_SYNC_TAG) {
    event.waitUntil(refreshSession());
  }
});

self.addEventListener("sync", (event) => {
  if (event.tag === PERIODIC_SYNC_TAG) {
    event.waitUntil(refreshSession());
  }
});

self.addEventListener("message", (event) => {
  if (event.data?.type === "refresh-account-session") {
    event.waitUntil(refreshSession());
  }
});
