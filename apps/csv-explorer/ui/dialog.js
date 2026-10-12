/* Wardian UI dialog: buttons with data-w-close close their dialog, and a click on the backdrop closes
   it too. WardianUI.confirm(text, {title, ok, cancel, destructive}) resolves to true or false. */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  document.addEventListener('click', e => {
    const close = e.target.closest && e.target.closest('[data-w-close]');
    if (close){ const d = close.closest('dialog'); if (d) d.close(close.getAttribute('data-w-close') || ''); return; }
    // A click on the dialog element itself, outside its content box, is a click on the backdrop.
    const d = e.target;
    if (d instanceof HTMLDialogElement && d.classList.contains('w-dialog') && d.open){
      const r = d.getBoundingClientRect();
      if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) d.close('');
    }
  });
  UI.confirm = (text, opts = {}) => new Promise(resolve => {
    const d = document.createElement('dialog');
    d.className = 'w-dialog';
    const h = Object.assign(document.createElement('h2'), {className: 'w-dialog-title', textContent: opts.title || 'Are you sure?', id: 'w-confirm-' + Date.now()});
    const p = Object.assign(document.createElement('p'), {className: 'w-dialog-description', textContent: text || ''});
    const f = Object.assign(document.createElement('div'), {className: 'w-dialog-footer'});
    const no = Object.assign(document.createElement('button'), {className: 'w-button', textContent: opts.cancel || 'Cancel', type: 'button'});
    const yes = Object.assign(document.createElement('button'), {className: 'w-button', textContent: opts.ok || 'Continue', type: 'button'});
    no.dataset.variant = 'outline'; no.setAttribute('data-w-close', 'no');
    if (opts.destructive) yes.dataset.variant = 'destructive';
    yes.setAttribute('data-w-close', 'yes');
    d.setAttribute('aria-labelledby', h.id);
    f.append(no, yes); d.append(h, p, f); document.body.append(d);
    d.addEventListener('close', () => { resolve(d.returnValue === 'yes'); d.remove(); }, {once: true});
    d.showModal(); no.focus();
  });
})();
