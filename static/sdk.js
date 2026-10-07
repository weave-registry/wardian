/* wardian.js: channels for page apps. Load it in your page with
     <script src="/sdk/wardian.js"></script>
   then:
     const budget = wardian.channel('budget');
     budget.on((data, {from}) => ...);     // resolves once the user allows it
     budget.send({monthly: 1798.65});      // resolves once delivered
   Declare each channel in app.json ("channels": {"send": [...], "receive": [...]}, "format": 2).
   Wardian asks the user the first time. A page opened on its own, outside Wardian, has no channels. */
window.wardian = (() => {
  'use strict';
  let nextId = 1;
  const pending = new Map(), handlers = new Map();
  addEventListener('message', e => {
    if (e.source !== parent || !e.data || e.data.wardian !== 'ch') return;
    const m = e.data;
    if (m.k === 'reply') {
      const p = pending.get(m.id); if (!p) return;
      pending.delete(m.id);
      if (m.ok) p.res(m.value); else p.rej(new Error(m.error));
    } else if (m.k === 'msg') {
      (handlers.get(m.channel) || []).forEach(fn => { try { fn(m.data, {from: m.from, at: m.at}); } catch (err) { console.error(err); } });
    }
  });
  function request(m){
    if (parent === window) return Promise.reject(new Error('open this app inside Wardian to use channels'));
    return new Promise((res, rej) => {
      const id = nextId++;
      pending.set(id, {res, rej});
      parent.postMessage(Object.assign({wardian: 'ch', id}, m), '*');
      // Long enough for the user to answer the permission question.
      setTimeout(() => { if (pending.delete(id)) rej(new Error('Wardian did not answer')); }, 120000);
    });
  }
  return Object.freeze({
    channel(name){
      return Object.freeze({
        send: data => request({k: 'send', channel: name, data}),
        on(fn){
          if (!handlers.has(name)) handlers.set(name, []);
          handlers.get(name).push(fn);
          return request({k: 'on', channel: name});
        },
      });
    },
  });
})();
window.rustle = window.wardian;   // the name before the rename, so older pages keep working
