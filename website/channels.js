/* Wardian channels: named message channels between separate apps, with the user's permission.

   The same answers cover host capabilities such as "splunk" (mode "use"): see permit().

   This runs in Wardian's own pages (the app list and the suite kernel), never inside an app.
   - An app may only use a channel its package declares ("channels": {"send": [...], "receive": [...]}).
   - The first use asks the user, like a phone app asking for the camera. The answer is saved on the
     server (Settings → App permissions lists every answer and can take it back).
   - Messages go between Wardian tabs in this browser through a BroadcastChannel. The host stamps each
     message with the sending package's name, so an app cannot pretend to be another.
   - The latest message on each channel is kept by the server, so an app that starts later still gets it. */
const WardianChannels = (() => {
  'use strict';
  const NAME = /^[a-z0-9][a-z0-9._-]{0,63}$/;
  const MAX_BYTES = 256 * 1024;
  const RATE = { max: 100, perMs: 10000 };          // sends per package per 10 s
  const bus = new BroadcastChannel('wardian-channels');
  const grantBus = new BroadcastChannel('wardian-grants');
  const subs = new Map();                           // channel -> Set({pkg, fn})
  const asking = new Map();                         // grant key -> Promise<boolean>
  const sent = new Map();                           // pkg -> recent send times
  let grants = null;                                // Map grant key -> allow (boolean)
  let prompt = null;                                // ({pkg, channel, mode}) -> Promise<'allow'|'deny'|null>

  const key = (pkg, channel, mode) => `${pkg}|${channel}|${mode}`;
  const token = () => { try { return sessionStorage.getItem('adminToken') || ''; } catch { return ''; } };

  async function loadGrants(){
    try {
      const r = await fetch('/api/grants', {cache: 'no-store'});
      const j = await r.json();
      grants = new Map((j.grants || []).map(g => [key(g.app, g.channel, g.mode), !!g.allow]));
    } catch { grants = grants || new Map(); }
  }
  // Another tab (or Settings) changed an answer: forget ours and read the saved ones again.
  grantBus.onmessage = () => { loadGrants(); };

  function checkName(channel){
    if (typeof channel !== 'string' || !NAME.test(channel)) throw new Error(`"${channel}" is not a channel name (lowercase letters, digits, '.', '-', '_')`);
  }

  async function allowed(pkg, channel, mode){
    if (!grants) await loadGrants();
    const k = key(pkg, channel, mode);
    if (grants.has(k)) return grants.get(k);
    if (!asking.has(k)) asking.set(k, ask(pkg, channel, mode).finally(() => asking.delete(k)));
    return asking.get(k);
  }

  async function ask(pkg, channel, mode){
    const answer = prompt ? await prompt({pkg, channel, mode}) : null;
    if (answer !== 'allow' && answer !== 'deny') return false;      // closed without an answer: ask again next time
    const headers = {'Content-Type': 'application/json'};
    if (token()) headers['X-Admin-Token'] = token();
    const r = await fetch('/api/grants', {method: 'POST', headers, body: JSON.stringify({app: pkg, channel, mode, decision: answer})});
    if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || 'could not save the permission');
    grants.set(key(pkg, channel, mode), answer === 'allow');
    grantBus.postMessage('changed');
    return answer === 'allow';
  }

  function deliver(msg){
    for (const s of subs.get(msg.channel) || []) {
      if (s.pkg === msg.from) continue;                               // a package does not hear itself
      if (grants && grants.get(key(s.pkg, msg.channel, 'receive')) !== true) continue;   // taken back meanwhile
      try { s.fn(structuredClone(msg.data), {from: msg.from, at: msg.at}); } catch (e) { console.error(e); }
    }
  }
  bus.onmessage = e => { if (e.data && typeof e.data.channel === 'string') deliver(e.data); };

  // The latest message, kept by the Wardian server (state.js), so an app that starts later, in any
  // browser, still gets it.
  const latest = channel => WardianState.channel.latest(channel);

  /** Sends `data` (JSON-compatible) on `channel` for package `pkg`. Rejects if the user said no. */
  async function send(pkg, channel, data){
    checkName(channel);
    const json = JSON.stringify(data ?? null);
    if (json.length > MAX_BYTES) throw new Error(`a channel message may be at most ${MAX_BYTES / 1024} KB`);
    const now = Date.now();
    const times = (sent.get(pkg) || []).filter(t => now - t < RATE.perMs);
    if (times.length >= RATE.max) throw new Error('too many channel messages; slow down');
    if (!(await allowed(pkg, channel, 'send'))) throw new Error(`not allowed to send on "${channel}"`);
    times.push(now); sent.set(pkg, times);
    const msg = {channel, from: pkg, at: now, data: JSON.parse(json)};
    WardianState.channel.keep(channel, msg);   // live delivery below does not wait for it
    bus.postMessage(msg);
    deliver(msg);
  }

  /** Calls fn(data, {from, at}) for each message on `channel`, starting with the latest one kept.
      Resolves to a function that stops listening. Rejects if the user said no. */
  async function receive(pkg, channel, fn){
    checkName(channel);
    if (!(await allowed(pkg, channel, 'receive'))) throw new Error(`not allowed to receive on "${channel}"`);
    const s = {pkg, fn};
    if (!subs.has(channel)) subs.set(channel, new Set());
    subs.get(channel).add(s);
    const last = await latest(channel);
    if (last && last.from !== pkg) setTimeout(() => { try { fn(last.data, {from: last.from, at: last.at}); } catch (e) { console.error(e); } }, 0);
    return () => subs.get(channel).delete(s);
  }

  /** Asks (once) whether package `pkg` may use the host capability `cap`, such as "splunk". */
  async function permit(pkg, cap){
    checkName(cap);
    return allowed(pkg, cap, 'use');
  }
  const USE_WORDS = { splunk: 'Splunk searches', ai: 'Claude AI requests' };
  const USE_HINT = {
    splunk: "Wardian runs them with this server's Splunk account, and the app sees the results.",
    ai: "Wardian sends what the app writes to Claude, billed to this server's Claude account (Anthropic or Amazon Bedrock).",
  };

  /** Shows the permission question in a bar at the top of `container`. The bar stays in view
      while the page scrolls, since the button that caused the question may be far down the page. */
  function promptBar(container){
    if (!document.getElementById('wardian-perm-css')) {
      document.head.append(Object.assign(document.createElement('style'), {id: 'wardian-perm-css', textContent: `
        .wardian-perm { display: flex; flex-wrap: wrap; gap: .5rem; align-items: center; margin: 0 0 .75rem; padding: .6rem .8rem;
          position: sticky; top: 8px; z-index: 50; box-shadow: 0 4px 16px rgba(0,0,0,.15);
          border: 1px solid #e6b89f; border-radius: 8px; background: #fbe9de; color: #3b1d0e; font: 14px/1.4 system-ui, sans-serif; }
        .wardian-perm span { flex: 1; min-width: 14rem; }
        .wardian-perm button { font: inherit; border-radius: 6px; padding: .3rem .7rem; cursor: pointer; border: 1px solid #c9a28c; background: #fff; color: #3b1d0e; }
        .wardian-perm button.yes { background: #b9471f; border-color: #b9471f; color: #fff; }
        @media (prefers-color-scheme: dark) { .wardian-perm { background: #3a2418; border-color: #6b3b22; color: #f6e3d6; }
          .wardian-perm button { background: #1f1b16; color: #f6e3d6; border-color: #6b3b22; } .wardian-perm button.yes { background: #f08a5d; border-color: #f08a5d; color: #1a0f08; } }`}));
    }
    return ({pkg, channel, mode}) => new Promise(resolve => {
      const bar = document.createElement('div');
      bar.className = 'wardian-perm';
      bar.setAttribute('role', 'alertdialog');
      const text = document.createElement('span');
      const b = s => Object.assign(document.createElement('strong'), {textContent: s});
      if (mode === 'use' && channel.startsWith('tables.'))
        text.append('🍂 ', b(pkg), ' wants to read the tables of ', b(channel.slice(7)), '. It can only read them, not change them.');
      else if (mode === 'use') text.append('🍂 ', b(pkg), ' wants to run ', b(USE_WORDS[channel] || channel), '. ', USE_HINT[channel] || '');
      else text.append('🍂 ', b(pkg), mode === 'send' ? ' wants to send messages on the channel ' : ' wants to read messages on the channel ', b(channel), '. ',
        mode === 'send' ? 'Other apps you allow can read them.' : 'They come from other apps you allow.');
      const btn = (label, cls, answer) => Object.assign(document.createElement('button'), {textContent: label, className: cls, onclick: () => { bar.remove(); resolve(answer); }});
      bar.append(text, btn('Allow', 'yes', 'allow'), btn("Don't allow", '', 'deny'), btn('Not now', '', null));
      container.prepend(bar);
    });
  }

  return Object.freeze({
    send, receive, permit, promptBar,
    setPrompt(fn){ prompt = fn; },
    /** Call after changing answers elsewhere (Settings): every Wardian tab reads them again. */
    async refresh(){ await loadGrants(); grantBus.postMessage('changed'); },
    validName: c => typeof c === 'string' && NAME.test(c),
  });
})();
