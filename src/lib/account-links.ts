// These are customer-app routes, not authentication or membership decisions.
// Contract: zed-web-server.rs@0251593, src/views/onboarding.rs. The /login and
// /signup aliases currently discard return_to; use the canonical PKCE entry.
export const APP_ORIGIN = "https://app.zpkg.net";

// Promote only after docs/account-rollout.md is satisfied against the deployed
// customer app. A merged source change is not evidence of a live login service.
export const HOSTED_ONBOARDING_ENABLED = false;

export const accountJourneys = [
  {
    id: "individual",
    title: "For individuals",
    audience: "Your next project",
    description: "Explore packages on your own, then set up a personal namespace when you want to publish.",
    signInLabel: "Individual sign-in",
    setupLabel: "Continue individual setup",
    signInHref: `${APP_ORIGIN}/auth/sign-in?return_to=%2Fonboarding%2Findividual`,
    setupHref: `${APP_ORIGIN}/onboarding/individual`,
    details: [
      "Use your own identity; no shared team credentials.",
      "A personal publishing namespace is optional.",
      "Trying the CLI does not require creating a hosted workspace.",
    ],
  },
  {
    id: "organization",
    title: "For organizations",
    audience: "A place for your team",
    description: "Create or choose a workspace for company packages, with a separate identity and explicit membership for each person.",
    signInLabel: "Organization sign-in",
    setupLabel: "Continue organization setup",
    signInHref: `${APP_ORIGIN}/auth/sign-in?return_to=%2Fonboarding%2Forganization`,
    setupHref: `${APP_ORIGIN}/onboarding/organization`,
    details: [
      "Workspace owners manage product membership and roles.",
      "An existing employee needs membership granted by a workspace owner.",
      "A company email domain alone never grants access.",
    ],
  },
] as const;
