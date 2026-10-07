/* Wardian UI tooltip: gives each [data-tooltip] element its text as a description for screen readers.
   Call WardianUI.tooltips(root) after adding tooltips to the page later. */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  let n = 0;
  UI.tooltips = (root = document) => {
    for (const el of root.querySelectorAll('[data-tooltip]:not([data-w-tip])')) {
      const tip = document.createElement('span');
      tip.className = 'w-sr'; tip.id = 'w-tip-' + (++n); tip.textContent = el.getAttribute('data-tooltip');
      el.after(tip);
      el.setAttribute('aria-describedby', ((el.getAttribute('aria-describedby') || '') + ' ' + tip.id).trim());
      el.setAttribute('data-w-tip', '');
    }
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => UI.tooltips());
  else UI.tooltips();
})();
