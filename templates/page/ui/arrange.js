/* arrange.js: Arrange, the same in every Wardian app. Each viewer may change the order of an app's
   panels, move them between columns, hide them, or use one column. Only that viewer sees it.

   In a page app, mark the panels and load this file (wardian add arrange <package>):

     <main data-arrange-grid>                         <!-- optional: lets two columns swap or join -->
       <div data-arrange-column="side">  <section data-panel="inputs" data-panel-label="Inputs">…</section> </div>
       <div data-arrange-column="main">  <section data-panel="chart">…</section> … </div>
     </main>

   With no data-arrange-column, the parent of the panels is the one column. Wardian keeps the layout
   for the page (outside Wardian it lasts until the page closes). Wardian's own pages call
   WardianArrange.init({...}) directly: the suite kernel for its frames, the app list for a module.
   A hidden panel is only out of sight: its code still runs. */
(() => {
  'use strict';
  if (window.WardianArrange) return;

  const CSS = `
  [data-arrange-hidden] { display: none !important; }
  .w-arrange-one { grid-template-columns: minmax(0, 1fr) !important; }
  .w-arrange-swap { direction: rtl; } .w-arrange-swap > * { direction: ltr; }
  /* Bottom left: apps keep their own actions top right, and toasts use the bottom right. */
  .w-arrange-btn { position: fixed; bottom: 12px; left: 12px; z-index: 2147483000; font: 600 12px system-ui, sans-serif; cursor: pointer;
    padding: 5px 10px; border-radius: 999px; border: 1px solid rgba(127,127,127,.35); background: rgba(127,127,127,.12); color: inherit;
    opacity: .55; backdrop-filter: blur(6px); }
  .w-arrange-btn:hover, .w-arrange-btn:focus-visible { opacity: 1; }
  .w-arrange-btn.inline { position: static; opacity: 1; backdrop-filter: none; }
  .w-arrange-on .w-arrange-btn { display: none; }
  .w-arrange-bar { position: sticky; top: 0; z-index: 2147483001; display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center;
    padding: 10px clamp(16px, 3vw, 40px); background: #1f3a35; color: #eef6f3; font: 13px system-ui, sans-serif; box-shadow: 0 4px 16px rgba(0,0,0,.18); }
  .w-arrange-bar[hidden] { display: none; }
  .w-arrange-bar strong { font-size: 14px; }
  .w-arrange-bar .grow { flex: 1; min-width: 12rem; opacity: .85; }
  .w-arrange-bar button, .w-arrange-bar select { font: inherit; border-radius: 6px; padding: 4px 10px; cursor: pointer;
    border: 1px solid rgba(255,255,255,.35); background: transparent; color: inherit; }
  .w-arrange-bar select option { color: #111; }
  .w-arrange-bar button.yes { background: #6cbfad; border-color: #6cbfad; color: #0f1f1c; font-weight: 600; }
  .w-arrange-bar .hidden-list button { margin-left: 4px; }
  .w-arrange-cover { display: none; }
  .w-arrange-on [data-arrange-panel] { position: relative; min-height: 56px; }
  .w-arrange-on .w-arrange-cover { display: flex; position: absolute; inset: 0; z-index: 2147482000; min-height: 52px; align-items: flex-start;
    justify-content: space-between; gap: 8px; padding: 8px; border: 2px dashed #2f6f63; border-radius: 10px; cursor: grab;
    background: color-mix(in srgb, Canvas 72%, transparent); }
  .w-arrange-cover.over-before { box-shadow: inset 0 4px 0 #2f6f63; } .w-arrange-cover.over-after { box-shadow: inset 0 -4px 0 #2f6f63; }
  .w-arrange-cover .name { font: 600 13px system-ui, sans-serif; padding: 4px 10px; border-radius: 6px; background: #2f6f63; color: #fff; }
  .w-arrange-cover .tools { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 4px; }
  .w-arrange-cover button { font: 13px system-ui, sans-serif; white-space: nowrap; min-width: 30px; padding: 4px 8px; border-radius: 6px; cursor: pointer;
    border: 1px solid #2f6f63; background: #fff; color: #1f3a35; }
  .w-arrange-cover button:disabled { opacity: .35; cursor: default; }
  @media (prefers-color-scheme: dark) {
    .w-arrange-on .w-arrange-cover { border-color: #6cbfad; }
    .w-arrange-cover .name { background: #6cbfad; color: #0f1f1c; }
    .w-arrange-cover button { background: #1a1f25; color: #e8ecef; border-color: #6cbfad; }
    .w-arrange-cover.over-before { box-shadow: inset 0 4px 0 #6cbfad; } .w-arrange-cover.over-after { box-shadow: inset 0 -4px 0 #6cbfad; } }
  @media (prefers-reduced-motion: no-preference) { .w-arrange-cover { transition: box-shadow .1s; } }`;

  const mk = (tag, props) => Object.assign(document.createElement(tag), props || {});
  const pretty = n => String(n).replace(/[-_.]+/g, ' ').replace(/^./, c => c.toUpperCase());

  /**
   * opts: {
   *   columns: [{id, el, label}]          one or two boxes the panels sit in
   *   panels: [{id, el, home, label}]     home: the column id the package puts it in
   *   grid: element                       with two columns: lets them swap or join (classes w-arrange-swap, w-arrange-one)
   *   load(): layout | null | Promise     the viewer's saved layout
   *   save(layout)                        keep it (null: forget it)
   *   place(box, node): boolean           move a panel; false when a move would restart it (then reload())
   *   reload()                            reload the page, coming back in Arrange mode
   *   startOpen: boolean                  open in Arrange mode (after reload())
   *   buttonIn, barIn: elements           put the button and the bar there instead of on the page corner and top
   * }
   * Resolves to {open, close, layout, destroy}, or null when there are no panels.
   */
  async function init(opts){
    if (!document.getElementById('w-arrange-css')) document.head.append(mk('style', {id: 'w-arrange-css', textContent: CSS}));
    const cols = opts.columns.filter(c => c && c.el);
    const panels = new Map(opts.panels.map(p => [p.id, p]));
    if (!panels.size) return null;
    const colIds = cols.map(c => c.id);
    const grid = cols.length === 2 ? opts.grid : null;
    const place = opts.place || ((box, node) => { if (box.moveBefore && node.isConnected) box.moveBefore(node, null); else box.append(node); return true; });
    const label = id => (panels.get(id) && panels.get(id).label) || pretty(id);
    const colOf = id => { const p = panels.get(id).el.parentElement; const c = cols.find(c => c.el === p); return c ? c.id : panels.get(id).home; };
    for (const p of panels.values()) p.el.setAttribute('data-arrange-panel', p.id);

    // The columns and order a layout asks for. Panels it does not know (new in the package) keep their home.
    function plan(l){
      const out = Object.fromEntries(colIds.map(c => [c, []])), seen = new Set();
      const lists = l && l.columns || {};
      for (const c of colIds) for (const n of Array.isArray(lists[c]) ? lists[c] : []) if (panels.has(n) && !seen.has(n)){ out[c].push(n); seen.add(n); }
      for (const [n, p] of panels) if (!seen.has(n)) out[colIds.includes(p.home) ? p.home : colIds[0]].push(n);
      return out;
    }
    function current(){
      return {v: 1, mode: modeSel ? modeSel.value : 'two',
        columns: Object.fromEntries(cols.map(c => [c.id, [...c.el.children].map(e => e.getAttribute('data-arrange-panel')).filter(n => panels.has(n))])),
        hidden: [...panels].filter(([, p]) => p.el.hasAttribute('data-arrange-hidden')).map(([n]) => n)};
    }
    // Returns false when applying it would restart a panel (a frame without moveBefore).
    function apply(l, live){
      const want = plan(l);
      if (live && JSON.stringify(want) !== JSON.stringify(current().columns) && opts.canMoveLive === false) return false;
      for (const c of cols) for (const n of want[c.id]) place(c.el, panels.get(n).el);
      const hide = new Set(l && Array.isArray(l.hidden) ? l.hidden : []);
      for (const [n, p] of panels) p.el.toggleAttribute('data-arrange-hidden', hide.has(n));
      if (grid){
        // A column left with no panels disappears, and the other one takes the whole width.
        const mode = l && ['one', 'swap'].includes(l.mode) ? l.mode : 'two';
        const empty = cols.filter(c => ![...c.el.children].some(e => e.hasAttribute('data-arrange-panel')));
        grid.classList.toggle('w-arrange-one', mode === 'one' || empty.length > 0);
        grid.classList.toggle('w-arrange-swap', mode === 'swap' && !empty.length);
        for (const c of cols) c.el.hidden = empty.includes(c);
      }
      return true;
    }

    let layout = null;
    try { layout = await opts.load(); } catch { layout = null; }
    if (layout && layout.v === 1 && !layout.columns && (layout.aside || layout.main)) layout = {v: 1, mode: layout.cols, columns: {aside: layout.aside, main: layout.main}, hidden: layout.hidden};
    if (!(layout && layout.v === 1)) layout = null;
    apply(layout, false);

    // ---------- the bar, the button and a cover over each panel ----------
    const bar = mk('div', {className: 'w-arrange-bar', hidden: true});
    bar.setAttribute('role', 'region'); bar.setAttribute('aria-label', 'Arrange this page');
    const hiddenList = mk('span', {className: 'hidden-list'});
    let modeSel = null;
    bar.append(mk('strong', {textContent: 'Arrange this page'}),
      mk('span', {className: 'grow', textContent: 'Drag a panel, or use its buttons. Only you see this layout.'}));
    if (grid){
      modeSel = mk('select');
      [['two', 'Two columns'], ['swap', 'Two, side panel on the right'], ['one', 'One column']].forEach(([v, t]) => modeSel.append(mk('option', {value: v, textContent: t})));
      modeSel.setAttribute('aria-label', 'Columns');
      const lab = mk('label', {textContent: 'Columns '}); lab.append(modeSel); bar.append(lab);
      modeSel.addEventListener('change', () => change(l => { l.mode = modeSel.value; }));
    }
    const resetBtn = mk('button', {type: 'button', textContent: 'Reset'});
    const doneBtn = mk('button', {type: 'button', className: 'yes', textContent: 'Done'});
    bar.append(hiddenList, resetBtn, doneBtn);
    const btn = mk('button', {type: 'button', className: 'w-arrange-btn' + (opts.buttonIn ? ' inline' : ''), textContent: 'Arrange', title: 'Change the layout of this page, for you only'});
    (opts.barIn || document.body).prepend(bar);
    (opts.buttonIn || document.body).append(btn);

    function save(l){ layout = l; try { opts.save(l); } catch { /* the layout lasts until the page closes */ } }
    function change(fn){
      const l = current(); fn(l);
      save(l);
      if (!apply(l, true)){ opts.reload(); return; }
      refresh();
    }
    function move(n, to, before){
      change(l => {
        for (const c of colIds) l.columns[c] = l.columns[c].filter(x => x !== n);
        const i = before ? l.columns[to].indexOf(before) : -1;
        if (i < 0) l.columns[to].push(n); else l.columns[to].splice(i, 0, n);
      });
    }
    const other = id => colIds.find(c => c !== colOf(id));
    function refresh(){
      if (modeSel) modeSel.value = layout && ['one', 'swap'].includes(layout.mode) ? layout.mode : 'two';
      hiddenList.replaceChildren();
      const hid = [...panels].filter(([, p]) => p.el.hasAttribute('data-arrange-hidden')).map(([n]) => n);
      if (hid.length) hiddenList.append('Hidden:');
      for (const n of hid) hiddenList.append(mk('button', {type: 'button', textContent: 'Show ' + label(n),
        onclick: () => change(l => { l.hidden = l.hidden.filter(x => x !== n); })}));
      const swapped = grid && grid.classList.contains('w-arrange-swap');
      for (const [n, p] of panels){
        const sibs = [...p.el.parentElement.children].filter(e => e.hasAttribute('data-arrange-panel'));
        const i = sibs.indexOf(p.el), c = p.el.querySelector(':scope > .w-arrange-cover');
        c.querySelector('[data-a=up]').disabled = i <= 0;
        c.querySelector('[data-a=down]').disabled = i === sibs.length - 1;
        const side = c.querySelector('[data-a=side]');
        if (side){
          const to = other(n), toCol = cols.find(x => x.id === to), first = cols[0].id === to;
          side.textContent = (first !== !!swapped) ? '← ' + toCol.label : toCol.label + ' →';
          side.title = 'Move to the ' + toCol.label;
          side.setAttribute('aria-label', side.title + ': ' + label(n));
        }
      }
    }
    // A cover over each panel: frames swallow mouse events, so dragging happens on the cover.
    for (const [n, p] of panels){
      const c = mk('div', {className: 'w-arrange-cover', draggable: true});
      c.setAttribute('aria-label', 'Panel ' + label(n));
      const tools = mk('span', {className: 'tools'});
      const tool = (a, text, title, fn) => { const b = mk('button', {type: 'button', textContent: text, title}); b.dataset.a = a; b.setAttribute('aria-label', title + ': ' + label(n)); b.onclick = fn; tools.append(b); };
      const sibs = () => [...p.el.parentElement.children].filter(e => e.hasAttribute('data-arrange-panel'));
      tool('up', '↑', 'Move up', () => { const s = sibs(); move(n, colOf(n), s[s.indexOf(p.el) - 1].getAttribute('data-arrange-panel')); });
      tool('down', '↓', 'Move down', () => { const s = sibs(), nx = s[s.indexOf(p.el) + 2]; move(n, colOf(n), nx ? nx.getAttribute('data-arrange-panel') : null); });
      if (cols.length > 1) tool('side', '', 'Move to the other column', () => move(n, other(n), null));
      tool('hide', 'Hide', 'Hide', () => change(l => { l.hidden.push(n); }));
      c.append(mk('span', {className: 'name', textContent: '⠿ ' + label(n)}), tools);
      c.addEventListener('dragstart', e => { e.dataTransfer.setData('text/plain', n); e.dataTransfer.effectAllowed = 'move'; });
      const where = e => { const r = c.getBoundingClientRect(); return e.clientY < r.top + r.height / 2 ? 'before' : 'after'; };
      c.addEventListener('dragover', e => { e.preventDefault(); c.classList.toggle('over-before', where(e) === 'before'); c.classList.toggle('over-after', where(e) === 'after'); });
      c.addEventListener('dragleave', () => c.classList.remove('over-before', 'over-after'));
      c.addEventListener('drop', e => {
        e.preventDefault(); c.classList.remove('over-before', 'over-after');
        const from = e.dataTransfer.getData('text/plain');
        if (!panels.has(from) || from === n) return;
        const s = sibs().map(x => x.getAttribute('data-arrange-panel')).filter(x => x !== from);
        const i = s.indexOf(n) + (where(e) === 'after' ? 1 : 0);
        move(from, colOf(n), s[i] || null);
      });
      p.el.append(c);
    }
    // Dropping on the empty part of a column puts the panel at its end.
    for (const col of cols){
      col.el.addEventListener('dragover', e => { if (e.target === col.el) e.preventDefault(); });
      col.el.addEventListener('drop', e => { if (e.target !== col.el) return; e.preventDefault(); const from = e.dataTransfer.getData('text/plain'); if (panels.has(from)) move(from, col.id, null); });
    }
    resetBtn.addEventListener('click', () => {
      save(null);
      if (!apply(null, true)){ opts.reload(); return; }
      refresh();
    });
    const open = on => {
      document.documentElement.classList.toggle('w-arrange-on', on); bar.hidden = !on;
      if (on){ refresh(); (modeSel || doneBtn).focus(); } else btn.focus();
    };
    btn.addEventListener('click', () => open(true));
    doneBtn.addEventListener('click', () => open(false));
    const onKey = e => { if (e.key === 'Escape' && !bar.hidden) open(false); };
    addEventListener('keydown', onKey);
    if (opts.startOpen) open(true);
    function destroy(){
      removeEventListener('keydown', onKey);
      document.documentElement.classList.remove('w-arrange-on');
      bar.remove(); btn.remove();
      for (const p of panels.values()){ p.el.querySelector(':scope > .w-arrange-cover')?.remove(); }
    }
    return {open: () => open(true), close: () => open(false), layout: () => layout, destroy};
  }

  // ---------- page apps: find the marked panels, keep the layout with Wardian ----------
  // A page app is sandboxed and cannot keep anything itself, so it asks the Wardian page around it.
  let nextId = 1;
  function ask(m){
    if (parent === window) return Promise.resolve(null);
    return new Promise(res => {
      const id = 'arrange' + nextId++;
      const on = e => { if (e.source === parent && e.data && e.data.wardian === 'layout' && e.data.id === id){ removeEventListener('message', on); res(e.data.layout || null); } };
      addEventListener('message', on);
      parent.postMessage(Object.assign({wardian: 'layout', id}, m), '*');
      setTimeout(() => { removeEventListener('message', on); res(null); }, 1500);
    });
  }
  function auto(){
    const els = [...document.querySelectorAll('[data-panel]')].filter(e => !e.parentElement.closest('[data-panel]'));
    if (!els.length) return null;
    let cols = [...document.querySelectorAll('[data-arrange-column]')];
    if (!cols.length) cols = [...new Set(els.map(e => e.parentElement))].slice(0, 1);
    const colId = el => el.getAttribute('data-arrange-column') || 'main';
    const columns = cols.slice(0, 2).map(el => ({id: colId(el), el, label: el.getAttribute('data-arrange-label') || (colId(el) === 'side' ? 'Side' : pretty(colId(el)))}));
    const panels = els.filter(e => columns.some(c => c.el === e.parentElement)).map(e => ({id: e.getAttribute('data-panel'), el: e,
      home: colId(e.parentElement), label: e.getAttribute('data-panel-label') || null}));
    let memory = null;
    return init({columns, panels, grid: document.querySelector('[data-arrange-grid]'),
      load: async () => (await ask({k: 'get'})) || memory,
      save: l => { memory = l; ask({k: 'set', layout: l}); }});
  }

  window.WardianArrange = Object.freeze({init, auto});
  if (document.currentScript && !document.currentScript.hasAttribute('data-manual')){
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', auto, {once: true});
    else auto();
  }
})();
