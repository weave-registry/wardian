// Adds the two headers that make the page cross-origin isolated (COOP and COEP), which a
// static host may not send. Isolation is what gives a worker SharedArrayBuffer, and with it
// Atomics.wait: a sleep that blocks.
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', (e) => {
  if (new URL(e.request.url).origin !== location.origin) return;
  e.respondWith((async () => {
    const r = await fetch(e.request);
    const h = new Headers(r.headers);
    h.set('Cross-Origin-Opener-Policy', 'same-origin');
    h.set('Cross-Origin-Embedder-Policy', 'require-corp');
    return new Response(r.body, { status: r.status, statusText: r.statusText, headers: h });
  })());
});
