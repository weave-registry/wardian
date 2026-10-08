/* Wardian UI toast: WardianUI.toast(text, {variant: 'default'|'success'|'destructive', title, ms})
   shows a short message in the corner and returns a function that closes it. Screen readers hear it
   (politely; a destructive one interrupts). It closes after ms (default 5000; 0 keeps it). */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  let box = null;
  function region(){
    if (box && box.isConnected) return box;
    box = document.createElement('section');
    box.className = 'w-toasts'; box.setAttribute('aria-label', 'Notifications');
    document.body.append(box);
    return box;
  }
  UI.toast = (text, opts = {}) => {
    const t = document.createElement('div');
    t.className = 'w-toast';
    if (opts.variant) t.dataset.variant = opts.variant;
    t.setAttribute('role', opts.variant === 'destructive' ? 'alert' : 'status');
    const body = document.createElement('span');
    if (opts.title) body.append(Object.assign(document.createElement('b'), {textContent: opts.title}));
    body.append(document.createTextNode(String(text)));
    const x = Object.assign(document.createElement('button'), {type: 'button', textContent: '×'});
    x.setAttribute('aria-label', 'Close');
    const close = () => t.remove();
    x.onclick = close;
    t.append(body, x); region().append(t);
    const ms = opts.ms === undefined ? 5000 : opts.ms;
    if (ms > 0) setTimeout(close, ms);
    return close;
  };
  window.wardianToast = UI.toast;
})();
