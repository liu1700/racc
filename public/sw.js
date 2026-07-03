// Racc PWA service worker.
//
// Intentionally cache-free: it exists so the app meets the PWA installability
// criteria (a registered SW with a fetch handler), but it does pure network
// passthrough. Racc is useless offline anyway — it needs a live WebSocket to
// racc-server — and not caching means a rebuilt/redeployed frontend is never
// served stale (Vite already content-hashes JS/CSS, but the HTML shell would be
// the stale-trap, so we skip caching entirely).

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", (event) => {
  // Network passthrough. If the network is down this rejects, which yields the
  // browser's normal offline behaviour — same as having no service worker.
  event.respondWith(fetch(event.request));
});
