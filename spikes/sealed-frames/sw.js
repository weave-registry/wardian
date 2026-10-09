// Stands in for Wardian compiled to WebAssembly: it serves a page app that exists only here,
// never on the static server behind it, at the path Wardian uses.
const PKG = {
  '/apps/demo/index.html': ['text/html', `<!doctype html><meta charset="utf-8">
<script>
  // The app's own page: it loads its script by a relative URL and its data with fetch,
  // as SPEC.md 5.4 allows.
  addEventListener('load', async () => {
    let data = 'not tried';
    try { data = await (await fetch('./data.txt')).text(); } catch (e) { data = 'failed: ' + e.message; }
    let storage = 'readable';
    try { localStorage.getItem('x'); } catch (e) { storage = 'blocked'; }
    let outside = 'reached';
    try { await fetch('https://example.com/'); } catch (e) { outside = 'blocked'; }
    parent.postMessage({ script: self.appRan === true, data, storage, outside, origin: self.origin }, '*');
  });
</script>
<script src="./app.js"></script>`],
  '/apps/demo/app.js': ['text/javascript', 'self.appRan = true;'],
  '/apps/demo/data.txt': ['text/plain', 'from the package'],
};
const CSP = "default-src 'none'; script-src 'unsafe-inline' 'self'; connect-src 'self'";
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  const f = PKG[url.pathname];
  if (!f) return;
  e.respondWith(new Response(f[1], { headers: { 'Content-Type': f[0], 'Content-Security-Policy': CSP } }));
});
// The bridge's other end: the host page asks for a package file by message.
self.addEventListener('message', (e) => {
  const f = PKG[e.data.path];
  e.source.postMessage({ id: e.data.id, ok: !!f, type: f && f[0], body: f && f[1] });
});
