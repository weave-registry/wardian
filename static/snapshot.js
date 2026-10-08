/* snapshot.js: the default rendering of a frame for "Save as web page" (ADR-2610080905).
   It copies what the frame shows, as HTML and CSS: field values are written into the copy, a canvas
   or an inline SVG becomes a data: image, and an image from a blob: or the package becomes a data:
   image. Suite frames get it with the shim, page apps with the small script Wardian adds to them,
   and Wardian's own page uses it for a module app's cards.
   The result is not trusted: the host page cleans it before anything goes into the file. */
const WardianSnapshot = (() => {
  'use strict';
  // Never in a copy: code, Arrange's own controls, and panels the viewer hid with Arrange.
  const DROP = 'script, noscript, template, style, link, meta, iframe, .w-arrange-bar, .w-arrange-btn, .w-arrange-cover, [data-arrange-hidden]';
  // What an SVG needs to look the same once it is an image and the page's CSS no longer reaches it.
  const SVG_CSS = ['fill', 'fill-opacity', 'stroke', 'stroke-width', 'stroke-opacity', 'stroke-dasharray', 'stroke-linecap',
    'stroke-linejoin', 'opacity', 'font-family', 'font-size', 'font-weight', 'text-anchor', 'dominant-baseline', 'visibility', 'display', 'color'];

  const asDataUrl = blob => new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsDataURL(blob); });
  // An image read into a data: URL: by drawing it (works for blob: images in a frame with no
  // network), else by fetching it (a page app may fetch its own files). null if neither works.
  async function imageData(img){
    try {
      if (img.complete && img.naturalWidth){
        const c = document.createElement('canvas');
        c.width = img.naturalWidth; c.height = img.naturalHeight;
        c.getContext('2d').drawImage(img, 0, 0);
        return c.toDataURL('image/png');
      }
    } catch { /* a tainted canvas: try fetching instead */ }
    try { const r = await fetch(img.currentSrc || img.src); if (r.ok) return await asDataUrl(await r.blob()); } catch { /* no network here */ }
    return null;
  }
  function size(el){ const r = el.getBoundingClientRect(); return {w: Math.round(r.width), h: Math.round(r.height)}; }
  function imageFor(src, el, alt){
    const img = document.createElement('img');
    const {w, h} = size(el);
    img.src = src;
    img.alt = alt || '';
    if (el.getAttribute('class')) img.setAttribute('class', el.getAttribute('class'));
    // A drawing as wide as its box stays as wide as its box, as the window that opens the file may
    // be wider; a smaller one keeps its size.
    const box = el.parentElement, full = box && w >= box.clientWidth - parseFloat(getComputedStyle(box).paddingLeft) - parseFloat(getComputedStyle(box).paddingRight) - 2;
    img.setAttribute('style', `display:block;width:${full ? '100%' : w + 'px'};max-width:100%;height:auto`);
    if (w) img.width = w;
    if (h) img.height = h;
    return img;
  }
  function svgImage(live, copy){
    const lives = [live, ...live.querySelectorAll('*')], copies = [copy, ...copy.querySelectorAll('*')];
    lives.forEach((l, i) => {
      const cs = getComputedStyle(l);
      copies[i].setAttribute('style', SVG_CSS.map(p => `${p}:${cs.getPropertyValue(p)}`).join(';'));
    });
    const {w, h} = size(live);
    copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    if (w) copy.setAttribute('width', w);
    if (h) copy.setAttribute('height', h);
    const text = new XMLSerializer().serializeToString(copy);
    return imageFor('data:image/svg+xml;charset=utf-8,' + encodeURIComponent(text), live, live.getAttribute('aria-label') || '');
  }

  /** A copy of `root` with what the viewer sees written in. Resolves to the copied element. */
  async function copy(root){
    const out = root.cloneNode(true);
    const lives = [...root.querySelectorAll('*')], copies = [...out.querySelectorAll('*')];
    const later = [];
    lives.forEach((l, i) => {
      const c = copies[i], tag = l.localName;
      if (tag === 'input'){
        const t = (l.type || '').toLowerCase();
        if (t === 'checkbox' || t === 'radio') c.toggleAttribute('checked', l.checked);
        else if (t === 'password' || t === 'file') c.removeAttribute('value');
        else c.setAttribute('value', l.value);
      } else if (tag === 'textarea') c.textContent = l.value;
      else if (tag === 'select') [...l.options].forEach((o, k) => c.options[k] && c.options[k].toggleAttribute('selected', o.selected));
      else if (tag === 'details') c.toggleAttribute('open', l.open);
      else if (tag === 'canvas'){
        let src = null;
        try { src = l.toDataURL('image/png'); } catch { /* drawn from another origin */ }
        if (src) c.replaceWith(imageFor(src, l, l.getAttribute('aria-label') || ''));
        else c.replaceWith(Object.assign(document.createElement('p'), {textContent: '(a drawing that could not be copied)'}));
      } else if (tag === 'svg' && !(l.parentElement && l.parentElement.closest('svg'))){
        if (!size(l).w) c.remove();
        else later.push(() => c.replaceWith(svgImage(l, c)));
      } else if (tag === 'img' && !/^data:image\//i.test(l.getAttribute('src') || '')){
        later.push(async () => { const d = await imageData(l); if (d) { c.setAttribute('src', d); c.removeAttribute('srcset'); } else c.remove(); });
      }
    });
    // SVGs read their live styles through `l`, so they are swapped after the walk above.
    for (const fn of later) await fn();
    out.querySelectorAll(DROP).forEach(e => e.remove());
    return out;
  }

  /** The frame's CSS: each <style>, and each stylesheet it can read. Web fonts are left out. */
  async function styles(){
    const out = [];
    for (const s of document.querySelectorAll('style, link[rel~=stylesheet]')){
      if (s.localName === 'style'){ if (s.id !== 'w-arrange-css') out.push(s.textContent); continue; }
      if (/^https:\/\/fonts\.(googleapis|gstatic)\.com\//.test(s.href)) continue;
      try { out.push([...s.sheet.cssRules].map(r => r.cssText).join('\n')); continue; } catch { /* another origin: fetch it */ }
      try { const r = await fetch(s.href); if (r.ok) out.push(await r.text()); } catch { /* no network here */ }
    }
    return out.join('\n');
  }

  /** The answer for the host: {html, css}. `custom` is an app's own snapshot result, if any:
      HTML text or an element, placed inside `root` in place of what it shows. */
  async function answer(root, custom, withCss){
    root = root || document.body;
    let html;
    if (custom !== null && custom !== undefined){
      const inner = custom instanceof Element ? (await copy(custom)).outerHTML : String(custom);
      // Keep the part's root and what is around it (its card, <main>), so its CSS still applies.
      // Parsed in a <template>, so nothing in it loads or runs here.
      const t = document.createElement('template');
      t.innerHTML = inner;
      let shell = root.cloneNode(false);
      shell.append(t.content);
      for (let p = root.parentElement; p && p !== document.body && p !== document.documentElement; p = p.parentElement){
        const up = p.cloneNode(false); up.append(shell); shell = up;
      }
      html = root === document.body ? inner : shell.outerHTML;
    } else {
      html = (await copy(document.body)).innerHTML;
    }
    return {html, css: withCss ? await styles() : ''};
  }

  return Object.freeze({copy, styles, answer});
})();
