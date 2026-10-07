/* Kernel shim: runs inside one sandboxed app frame and plays the single-file Kernel.
   An app sees the same Kernel.register(def) and the same ctx as in the one-page build, but every
   message, call and capability goes to the host kernel through postMessage, which copies the data.
   The host enforces the contract from suite.json; the checks here only fail fast with a clear message.
   The frame itself can reach nothing else: the browser blocks the network, the host page and storage. */
const Kernel = (() => {
  'use strict';
  let def = null, contract = null, nextId = 1, root = null, booted = false;
  const pending = new Map(), handlers = new Map(), methods = new Map(), chans = new Map();
  let saved = {};
  const HOST_CAPS = ['splunk'];   // capabilities the host provides under their own name (not "claude:…")

  const post = m => parent.postMessage(m, '*');
  const fault = e => { post({k: 'fault', message: String(e && e.message || e)}); console.error(e); };
  const safe = fn => { try { const r = fn(); if (r && r.catch) r.catch(fault); } catch (e) { fault(e); } };
  // Errors outside the kernel's own calls (a typo in app.js, a broken event handler) are faults too.
  // The browser's ResizeObserver loop notice is not a failure: it says some size notices moved to
  // the next frame (ADR-2610071110). Every other error is the app's.
  const NOTICE = /^ResizeObserver loop (completed with undelivered notifications|limit exceeded)/;
  addEventListener('error', e => { if (!e.error && NOTICE.test(String(e.message || ''))) return; fault(e.error || e.message); });
  addEventListener('unhandledrejection', e => fault(e.reason));
  const request = m => new Promise((res, rej) => { const id = nextId++; pending.set(id, {res, rej}); post(Object.assign({id}, m)); });

  function register(d){
    if (!d || !d.name) throw new Error('app needs a name');
    if (def) throw new Error('one app per frame');
    def = d;
  }

  // The contract an app's code declares must match suite.json, so the two can never drift apart unseen.
  const shape = c => JSON.stringify({
    emits: Object.keys(c.emits || {}).sort().map(k => [k, !!(c.emits[k] && c.emits[k].retain)]),
    listens: (c.listens || []).slice().sort(), provides: (c.provides || []).slice().sort(),
    needs: (c.needs || []).slice().sort(), caps: (c.caps || []).slice().sort(),
    channels: [((c.channels || {}).send || []).slice().sort(), ((c.channels || {}).receive || []).slice().sort()]
  });

  function makeCtx(){
    const allow = cap => { if (!contract.caps.includes(cap)) throw new Error(def.name + ' did not declare capability "' + cap + '"'); };
    function emit(topic, payload){
      if (!contract.emits[topic]) throw new Error(def.name + ' may not emit "' + topic + '" (not in its contract)');
      post({k: 'emit', topic, payload});
    }
    function on(topic, fn){
      if (!contract.listens.includes(topic)) throw new Error(def.name + ' may not listen to "' + topic + '" (not in its contract)');
      if (!handlers.has(topic)) handlers.set(topic, []);
      handlers.get(topic).push(fn);
      post({k: 'on', topic});
    }
    function provide(map){
      Object.keys(map).forEach(m => {
        if (!contract.provides.includes(m)) throw new Error(def.name + ' may not provide "' + m + '" (not in its contract)');
        methods.set(m, map[m]);
      });
      post({k: 'provide', methods: Object.keys(map)});
    }
    function call(app, method, args){
      const key = app + '.' + method;
      if (!contract.needs.includes(key)) return Promise.reject(new Error(def.name + ' may not call "' + key + '" (not in its contract)'));
      return request({k: 'call', app, method, args});
    }
    const store = {
      get(k){ allow('storage'); return Object.prototype.hasOwnProperty.call(saved, k) ? structuredClone(saved[k]) : null; },
      set(k, v){ allow('storage'); saved[k] = structuredClone(v); post({k: 'store', key: k, value: v}); }
    };
    // Resolves to the capability, or null when this host cannot provide it (the app is told, not broken).
    async function cap(name){
      allow(HOST_CAPS.includes(name) ? name : 'claude:' + name);
      const ops = await request({k: 'cap', name});
      if (!ops) return null;
      const out = {};
      ops.forEach(op => { out[op] = args => request({k: 'capop', name, op, args}); });
      return name === 'sample' ? sampler(out) : out;
    }
    // claude:sample has the Claude viewer's shape: sample(prompt, {signal, onText, modelTier}) resolves
    // to {text, truncated}; sample.json(prompt, opts) to the parsed JSON. Errors carry e.code.
    // There is no streaming here: onText is called once, with the whole answer.
    function sampler(ops){
      const coded = e => { const m = /^([a-z_]+): ([\s\S]*)$/.exec(String(e && e.message || e)); const err = new Error(m ? m[2] : String(e && e.message || e)); err.code = m ? m[1] : 'error'; return err; };
      const run = (op, prompt, opts) => new Promise((res, rej) => {
        const signal = opts && opts.signal;
        if (signal && signal.aborted) return rej(Object.assign(new Error('cancelled'), {code: 'cancelled'}));
        if (signal) signal.addEventListener('abort', () => rej(Object.assign(new Error('cancelled'), {code: 'cancelled'})), {once: true});
        ops[op]({prompt: String(prompt), tier: opts && opts.modelTier === 'quick' ? 'quick' : ''}).then(res, e => rej(coded(e)));
      });
      const sample = (prompt, opts) => run('text', prompt, opts).then(r => {
        if (opts && typeof opts.onText === 'function') try { opts.onText({text: r.text}); } catch (e) { fault(e); }
        return {text: r.text, truncated: !!r.truncated};
      });
      sample.json = (prompt, opts) => run('json', prompt, opts);
      return sample;
    }
    // A channel to other packages. The kernel asks the user the first time; send() and on()
    // reject if the answer is no. on(fn) calls fn(data, {from, at}).
    function channel(name){
      const decl = contract.channels || {send: [], receive: []};
      return Object.freeze({
        send(data){
          if (!decl.send.includes(name)) return Promise.reject(new Error(def.name + ' may not send on channel "' + name + '" (not in its contract)'));
          return request({k: 'chsend', channel: name, data});
        },
        on(fn){
          if (!decl.receive.includes(name)) return Promise.reject(new Error(def.name + ' may not receive on channel "' + name + '" (not in its contract)'));
          if (!chans.has(name)) chans.set(name, []);
          chans.get(name).push(fn);
          return request({k: 'chon', channel: name});
        },
      });
    }
    // cb runs on the next animation frame, at most once per frame, so a redraw that changes the
    // size cannot feed back into the same frame's notices.
    function observe(el, cb){
      if (typeof ResizeObserver === 'undefined') return;
      let queued = false;
      new ResizeObserver(entries => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => { queued = false; safe(() => cb(entries)); });
      }).observe(el);
    }
    // The bytes of a file in this suite's package, fetched by the kernel (the frame itself has no network).
    function asset(path){ allow('asset'); return request({k: 'asset', path}); }
    function source(id){ allow('source'); const el = document.getElementById(id); return el ? el.textContent : ''; }
    function spawn(code){
      allow('worker');
      try { return new Worker(URL.createObjectURL(new Blob([code], {type: 'text/javascript'}))); } catch (e) { return null; }
    }
    return Object.freeze({
      name: def.name, root,
      $: s => root.querySelector(s), $$: s => Array.from(root.querySelectorAll(s)),
      el: tag => document.createElement(tag), text: s => document.createTextNode(s),
      emit, on, provide, call, store, cap, asset, channel, observe, source, spawn
    });
  }

  addEventListener('message', e => {
    if (e.source !== parent) return;
    const m = e.data || {};
    if (m.k === 'boot' && !booted){
      booted = true;
      contract = m.contract; saved = m.store || {};
      if (!def) return fault('no app registered in this frame');
      if (shape(def) !== shape(contract)) return fault(def.name + ': the contract in app.js differs from suite.json, so the app was not started');
      safe(() => def.init(makeCtx()));
    } else if (m.k === 'msg'){
      (handlers.get(m.topic) || []).forEach(fn => safe(() => fn(m.payload)));
    } else if (m.k === 'invoke'){
      const fn = methods.get(m.method);
      Promise.resolve()
        .then(() => { if (!fn) throw new Error(m.method + ' is not provided'); return fn(m.args); })
        .then(value => post({k: 'result', id: m.id, ok: true, value}),
              err => post({k: 'result', id: m.id, ok: false, error: String(err && err.message || err)}));
    } else if (m.k === 'chmsg'){
      (chans.get(m.channel) || []).forEach(fn => safe(() => fn(m.data, {from: m.from, at: m.at})));
    } else if (m.k === 'reply'){
      const p = pending.get(m.id); if (!p) return;
      pending.delete(m.id);
      if (m.ok) p.res(m.value); else p.rej(new Error(m.error));
    }
  });

  // The host sizes each frame to its content; a hidden root collapses the frame.
  function reportSize(){
    const h = root && root.hidden ? 0 : Math.ceil(document.body.getBoundingClientRect().height);
    post({k: 'size', h, bg: getComputedStyle(document.body).backgroundColor});
  }

  function start(){
    root = document.querySelector('[data-app]') || document.body;
    new ResizeObserver(reportSize).observe(document.body);
    new MutationObserver(reportSize).observe(root, {attributes: true, attributeFilter: ['hidden']});
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', reportSize);
    post({k: 'hello', app: def ? def.name : null});
  }
  return Object.freeze({register, start});
})();
