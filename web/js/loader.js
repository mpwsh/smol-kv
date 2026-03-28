// ── View Loader ───────────────────────────────────────────────────────────
// Fetches HTML partials into data-view slots, then imports + starts Alpine.
// This file must be loaded as type="module" so we can use top-level await.

const slots = document.querySelectorAll("[data-view]");
await Promise.all(
  Array.from(slots).map(async (el) => {
    try {
      const res = await fetch(`views/${el.dataset.view}.html`);
      if (res.ok) el.innerHTML = await res.text();
    } catch (e) {
      console.error(`View load failed: ${el.dataset.view}`, e);
    }
  }),
);

// All partials in the DOM — now boot Alpine
const Alpine = (await import("https://cdn.jsdelivr.net/npm/alpinejs@3.15.8/dist/module.esm.js")).default;
window.Alpine = Alpine;
Alpine.start();
