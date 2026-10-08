/* Text: small helpers every part uses (suite.json "scripts"), and the one shape of a reading of the
   notes. Pure functions only: no DOM, no state.

   A reading:  { summary: "…", decisions: ["…"], action_items: [{ owner, task, due }], open_questions: ["…"] } */
const Text = Object.freeze({
  /** A short fingerprint of a text (FNV-1a, 32 bits, as 8 hex digits): equal texts, equal hashes. */
  hash(s) {
    let h = 0x811c9dc5;
    for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
    return (h >>> 0).toString(16).padStart(8, '0');
  },
  plural(n, one, many = one + 's') { return n + ' ' + (n === 1 ? one : many); },
  clip(s, n) { s = String(s == null ? '' : s).replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; },
  /** "Weekly product sync!" -> "weekly-product-sync" (safe for a file name). */
  slug(s) { return String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'meeting'; },
  /** Today as YYYY-MM-DD, in local time. */
  today() { const d = new Date(); return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0'); },
  /** Checks a reading and returns a clean copy with only the expected fields. Throws when it does not
      look like a reading at all. Missing lists become empty lists; long text is cut. */
  reading(r) {
    if (!r || typeof r !== 'object' || Array.isArray(r)) throw new Error('The answer is not an object.');
    const known = ['summary', 'decisions', 'action_items', 'open_questions'].filter(k => k in r);
    if (!known.length) throw new Error('The answer has none of the expected fields.');
    const list = (v, n = 50) => (Array.isArray(v) ? v : []).map(x => Text.clip(x, 500)).filter(Boolean).slice(0, n);
    const items = (Array.isArray(r.action_items) ? r.action_items : []).slice(0, 100).map(a => {
      if (typeof a === 'string') return { owner: '', task: Text.clip(a, 500), due: '', done: false };
      a = a && typeof a === 'object' ? a : {};
      return { owner: Text.clip(a.owner, 80), task: Text.clip(a.task, 500), due: Text.clip(a.due, 80), done: a.done === true };
    }).filter(a => a.task);
    return { summary: String(r.summary || '').trim().slice(0, 3000), decisions: list(r.decisions), action_items: items, open_questions: list(r.open_questions) };
  },
});
