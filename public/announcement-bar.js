const bars = document.querySelectorAll("[data-announcement-bar]");

for (const bar of bars) {
  const storageKey = bar.dataset.storageKey;
  let dismissed = false;

  if (storageKey) {
    try {
      dismissed = localStorage.getItem(storageKey) === "dismissed";
    } catch {
      // Storage can be unavailable; dismissal still works for this page view.
    }
  }

  if (dismissed) continue;

  bar.hidden = false;
  const dismissButton = bar.querySelector("[data-announcement-dismiss]");
  dismissButton?.addEventListener(
    "click",
    () => {
      bar.hidden = true;
      if (!storageKey) return;

      try {
        localStorage.setItem(storageKey, "dismissed");
      } catch {
        // The bar remains dismissed for this page view when storage is blocked.
      }
    },
    { once: true },
  );
}
