/* Wardian state: the viewer's Arrange layouts, each suite app's saved data, and the latest message
   per channel, kept by the Wardian server in its data folder (ADR-2610071055).

   The server comes first. The browser keeps a copy as a backup, and is used when the server says no
   (a shared Wardian with ADMIN_TOKEN, when this viewer has not given it). When the server has
   nothing yet for an app, what this browser held is uploaded, so existing data moves over.
   This runs in Wardian's own pages (the app list and the suite kernel), never inside an app. */
const WardianState = (() => {
  'use strict';
  const token = () => { try { return sessionStorage.getItem('adminToken') || ''; } catch { return ''; } };
  const headers = json => {
    const h = {};
    if (json) h['Content-Type'] = 'application/json';
    if (token()) h['X-Admin-Token'] = token();
    return h;
  };
  const local = {
    get(k){ try { return JSON.parse(localStorage.getItem(k) || 'null'); } catch { return null; } },
    set(k, v){ try { if (v === null || v === undefined) localStorage.removeItem(k); else localStorage.setItem(k, JSON.stringify(v)); } catch { /* full or blocked */ } },
    keys(prefix){ const out = []; try { for (let i = 0; i < localStorage.length; i++){ const k = localStorage.key(i); if (k && k.startsWith(prefix)) out.push(k); } } catch {} return out; },
  };

  // Resolves to {ok, value}; ok is false when the server is away or says no.
  async function get(path){
    try {
      const r = await fetch('/api/state/' + path, {headers: headers(false), cache: 'no-store'});
      if (!r.ok) return {ok: false};
      return {ok: true, value: await r.json()};
    } catch { return {ok: false}; }
  }
  // Writes go one at a time per path, so they arrive in the order they were made.
  const queues = new Map();
  function post(path, body){
    const prev = queues.get(path) || Promise.resolve();
    const next = prev.then(() => fetch('/api/state/' + path, {method: 'POST', headers: headers(true), body: JSON.stringify(body)})
      .then(r => r.ok ? r.json() : null).catch(() => null));
    queues.set(path, next);
    return next;
  }

  const layout = {
    async load(pkg){
      const key = 'wardian-layout:' + pkg, mine = local.get(key);
      const r = await get('layout/' + encodeURIComponent(pkg));
      if (!r.ok) return mine;
      if (r.value) { local.set(key, r.value); return r.value; }
      // Once per app and browser: move the browser's copy over. After that the server is the record.
      const moved = 'wardian-state-moved:layout:' + pkg;
      if (mine && !local.get(moved)) { post('layout/' + encodeURIComponent(pkg), {layout: mine}); local.set(moved, true); return mine; }
      return null;
    },
    save(pkg, l){
      local.set('wardian-layout:' + pkg, l || null);
      return post('layout/' + encodeURIComponent(pkg), {layout: l || null});
    },
  };

  const apps = {
    // {app: {key: value}} for one suite. The kernel calls this once, before the apps start.
    async load(pkg){
      const prefix = 'kernel:' + pkg + ':';
      const mine = {};
      for (const k of local.keys(prefix)) { const v = local.get(k); if (v && typeof v === 'object') mine[k.slice(prefix.length)] = v; }
      const r = await get('apps/' + encodeURIComponent(pkg));
      if (!r.ok) return mine;
      let data = r.value || {};
      // Once per app and browser: upload what only this browser has; the server's copy wins where
      // both have a key. After that the server is the record, so a key removed elsewhere stays removed.
      const moved = 'wardian-state-moved:apps:' + pkg;
      if (Object.keys(mine).length && !local.get(moved)) {
        const merged = await post('apps/' + encodeURIComponent(pkg), {merge: mine});
        if (merged && merged.data) { data = merged.data; local.set(moved, true); }
      }
      for (const [app, values] of Object.entries(data)) local.set(prefix + app, values);
      return data;
    },
    save(pkg, app, key, value, all){
      local.set('kernel:' + pkg + ':' + app, all);
      return post('apps/' + encodeURIComponent(pkg), {app, key, value: value === undefined ? null : value});
    },
  };

  const channel = {
    async latest(name){
      const r = await get('channel/' + encodeURIComponent(name));
      return r.ok && r.value && r.value.message ? r.value.message : local.get('wardian-channel:' + name);
    },
    keep(name, msg){
      local.set('wardian-channel:' + name, msg);
      return post('channel/' + encodeURIComponent(name), {message: msg});
    },
  };

  return Object.freeze({layout, apps, channel});
})();
