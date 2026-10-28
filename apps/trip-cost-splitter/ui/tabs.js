/* Wardian UI tabs: connects each tab to its panel (in order), shows one panel at a time, and moves
   between tabs with the arrow keys, Home and End. The element fires "w-tab-change" with {index}.
   Call WardianUI.tabs(root) after adding tabs to the page later. */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  let n = 0;
  function wire(box){
    const tabs = [...box.querySelectorAll('[role="tab"]')], panels = [...box.querySelectorAll('[role="tabpanel"]')];
    const id = 'w-tabs-' + (++n);
    const select = (i, focus) => {
      tabs.forEach((t, j) => { const on = i === j; t.setAttribute('aria-selected', String(on)); t.tabIndex = on ? 0 : -1; if (panels[j]) panels[j].hidden = !on; });
      if (focus) tabs[i].focus();
      box.dispatchEvent(new CustomEvent('w-tab-change', {detail: {index: i}, bubbles: true}));
    };
    tabs.forEach((t, i) => {
      t.id = t.id || id + '-tab-' + i;
      if (t.tagName === 'BUTTON') t.type = 'button';
      const p = panels[i];
      if (p){ p.id = p.id || id + '-panel-' + i; p.tabIndex = 0; t.setAttribute('aria-controls', p.id); p.setAttribute('aria-labelledby', t.id); }
      t.addEventListener('click', () => select(i));
      t.addEventListener('keydown', e => {
        const k = {ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1}[e.key];
        if (k === undefined) return;
        e.preventDefault(); select((k + tabs.length) % tabs.length, true);
      });
    });
    const start = tabs.findIndex(t => t.getAttribute('aria-selected') === 'true');
    select(start < 0 ? 0 : start);
    box.setAttribute('data-w-wired', '');
  }
  UI.tabs = (root = document) => root.querySelectorAll('.w-tabs:not([data-w-wired])').forEach(wire);
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => UI.tabs());
  else UI.tabs();
})();
