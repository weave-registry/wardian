/* SearchForm: reads and fills the search form (only the search part loads this file). It holds what
   the search in the box means (`meta`: about, units, use), which is cleared when the search is edited.
   It never stores anything; the part does. */
function SearchForm($){
  let meta = null;
  // Default dates for "Between two dates": from the start of yesterday until now.
  const dayAgo = new Date(); dayAgo.setDate(dayAgo.getDate() - 1); dayAgo.setHours(0, 0, 0, 0);
  $('#absFrom').value = TimeRange.localInput(dayAgo);

  // Puts a search into the form: when the app starts (the last search) and when one is picked.
  // Every field is checked as it is read, since older versions may have stored less.
  function apply(st){
    $('#spl').value = typeof st.s === 'string' ? st.s : '';
    const tr = st.tr && typeof st.tr === 'object' ? st.tr : {};
    if (typeof tr.n === 'string' || typeof tr.n === 'number') $('#relN').value = tr.n;
    if (TimeRange.UNITS[tr.u]) $('#relUnit').value = tr.u;
    if (typeof tr.from === 'string' && tr.from) $('#absFrom').value = tr.from;
    if (typeof tr.to === 'string') $('#absTo').value = tr.to;
    if (typeof tr.e === 'string') $('#advE').value = tr.e;
    if (typeof tr.l === 'string') $('#advL').value = tr.l;
    if (typeof st.t === 'string'){
      // Older versions saved only the earliest code, such as "-24h".
      const t = st.t.includes('|') || TimeRange.KINDS.includes(st.t) ? st.t : st.t + '|';
      $('#range').value = t;
      if ($('#range').value !== t) $('#range').value = '-24h|';
    }
    $('#title').value = typeof st.name === 'string' ? st.name : '';
    if (typeof st.words === 'string') $('#words').value = st.words;
    if (typeof st.wordsIndex === 'string') $('#wordsIndex').value = st.wordsIndex;
    $('#preset').value = st.preset === 'find' ? 'find' : '';
    $('#findWords').hidden = st.preset !== 'find';
    meta = st.meta && typeof st.meta === 'object' ? st.meta : null;
    showRange();
  }
  // The form as a plain object, the same shape apply reads.
  const state = () => ({s: $('#spl').value, t: $('#range').value, name: $('#title').value, meta,
    preset: $('#preset').value, words: $('#words').value, wordsIndex: $('#wordsIndex').value,
    tr: {n: $('#relN').value, u: $('#relUnit').value, from: $('#absFrom').value, to: $('#absTo').value, e: $('#advE').value, l: $('#advL').value}});

  const range = () => TimeRange.read({range: $('#range').value, label: ($('#range').selectedOptions[0] || {}).textContent,
    relN: $('#relN').value, relUnit: $('#relUnit').value, absFrom: $('#absFrom').value, absTo: $('#absTo').value, advE: $('#advE').value, advL: $('#advL').value});
  // Shows the extra fields for the chosen kind of range, and what it means or what is wrong.
  function showRange(){
    const v = $('#range').value, hint = $('#rangeHint');
    $('#rangeRel').hidden = v !== 'rel'; $('#rangeAbs').hidden = v !== 'abs'; $('#rangeAdv').hidden = v !== 'adv';
    if (!TimeRange.KINDS.includes(v)){ hint.textContent = ''; hint.className = 'w-hint range-hint'; return; }
    const r = range();
    hint.className = 'w-hint range-hint' + (r.error ? ' warn' : '');
    hint.textContent = r.error || ('Searches: ' + r.label + '.');
  }
  // Picks the time range for a Splunk "earliest" code (from a ready-made search or from Claude).
  function setEarliest(e){
    e = typeof e === 'string' ? e.trim() : '-7d';
    if ([...$('#range').options].some(o => o.value === e + '|')) $('#range').value = e + '|';
    else if (TimeRange.CODE.test(e)){ $('#range').value = 'adv'; $('#advE').value = e; $('#advL').value = ''; }
    else $('#range').value = '-7d|';
    showRange();
  }
  // A ready-made search, or one Claude wrote: {search, range, about, units, use}.
  function use(p){
    $('#spl').value = p.search;
    if (p.range !== undefined) setEarliest(p.range);
    if (p.about && p.about.title) $('#title').value = p.about.title;
    meta = {about: p.about || null, units: p.units || null, use: p.use || null};
  }
  // "Find words in all my data": every word (or "quoted phrase") must appear in the event.
  const words = () => ($('#words').value.match(/"[^"]*"|[^\s"]+/g) || []).map(w => w.replace(/["\\]/g, '').trim()).filter(Boolean);
  function fillFind(){
    const w = words(), idx = $('#wordsIndex').value.trim().replace(/[^\w\-*.]/g, '') || '*';
    $('#spl').value = 'index=' + idx + (w.length ? ' ' + w.map(x => '"' + x + '"').join(' ') : '') + ' | head 1000 | table _time index sourcetype host source _raw';
    $('#title').value = w.length ? 'Events with: ' + w.join(', ').slice(0, 80) : 'Events with words';
    meta = null;
  }
  return Object.freeze({apply, state, range, showRange, use, words, fillFind,
    meta: () => meta || {}, forget(){ meta = null; }});
}
