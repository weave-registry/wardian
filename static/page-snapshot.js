/* Wardian adds this to the end of every page app's HTML it serves (ADR-2610080905). It answers
   Wardian's page when the viewer saves the app as a web page, with a rendering of what this page
   shows; or, if the page sets window.wardianSnapshot to a function, with what that returns (HTML
   text or an element, or a promise of one). Only Wardian's own page around this one may ask. */
addEventListener('message', async e => {
  const m = e.data;
  // location.origin is the address this page came from: Wardian's, even in a sandbox.
  if (e.source !== parent || parent === window || e.origin !== location.origin || !m || m.wardian !== 'snapshot' || m.k !== 'ask') return;
  let out;
  try {
    let custom = null;
    if (typeof window.wardianSnapshot === 'function'){
      try { custom = await window.wardianSnapshot(); } catch (err) { console.error(err); custom = null; }
    }
    out = await WardianSnapshot.answer(document.body, custom, true);
  } catch (err) { out = {error: String(err && err.message || err)}; }
  parent.postMessage(Object.assign({wardian: 'snapshot', k: 'answer', id: m.id}, out), location.origin);
});
