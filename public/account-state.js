// @ts-check
// Presentation hints only. The customer app always rechecks actual authority.
/** @typedef {"unknown" | "anonymous" | "authenticated" | "unavailable"} AccountState */

export const DEFAULT_CHECK_INTERVAL_MS = 50 * 60 * 1000;
export const FAILURE_RETRY_INTERVAL_MS = 5 * 60 * 1000;
const MIN_CHECK_INTERVAL_MS = 60 * 1000;

/** @param {AccountState} state */
export function accountPresentation(state) {
  switch (state) {
    case "authenticated":
      return { label: "User dashboard", href: "https://app.zpkg.net/dashboard" };
    case "anonymous":
      return { label: "Log in", href: "https://app.zpkg.net/login" };
    case "unknown":
    case "unavailable":
      return { label: "Account", href: "/account/" };
    default: {
      /** @type {never} */
      const unreachable = state;
      throw new TypeError(`Unknown account state: ${unreachable}`);
    }
  }
}

/** @param {unknown} payload
 * @returns {{ state: AccountState, checkAfterMs: number }}
 */
export function parseSessionHint(payload) {
  if (
    payload === null ||
    typeof payload !== "object" ||
    Array.isArray(payload) ||
    !("authenticated" in payload) ||
    typeof payload.authenticated !== "boolean"
  ) {
    throw new TypeError("Session status omitted its boolean state");
  }
  const seconds = "check_after_seconds" in payload ? payload.check_after_seconds : undefined;
  const checkAfterMs = typeof seconds === "number" && Number.isFinite(seconds) && seconds > 0
    ? Math.min(Math.max(seconds * 1000, MIN_CHECK_INTERVAL_MS), DEFAULT_CHECK_INTERVAL_MS)
    : DEFAULT_CHECK_INTERVAL_MS;

  // No destination, identity, role, or other response field becomes navigation
  // or authority. All destinations come from the exhaustive presentation map.
  return {
    state: payload.authenticated ? "authenticated" : "anonymous",
    checkAfterMs,
  };
}
