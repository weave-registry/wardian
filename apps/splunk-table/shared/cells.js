/* Cells: what a table cell holds, shared by every part (inlined into each frame by suite.json "scripts").
   Pure functions only: no DOM, no state. */
const Cells = Object.freeze({
  // "1,234.5" counts as a number; empty cells and lists do not.
  isNum: v => v !== null && v !== '' && !Array.isArray(v) && isFinite(Number(String(v).replace(/,/g, ''))),
  num: v => Number(String(v).replace(/,/g, '')),
  // A column is numeric when every cell in it is a number or empty.
  numeric(fields, rows){
    return fields.map((_, i) => rows.every(r => r[i] == null || r[i] === '' || Cells.isNum(r[i])));
  },
  // One CSV cell: quoted when it holds a comma, a quote or a line break.
  csv(v){ const s = v == null ? '' : String(v); return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; },
  csvLine: r => r.map(Cells.csv).join(','),
  // A safe file name from the table's title.
  fileName: title => (title || 'splunk-table').replace(/[^\w.\- ]+/g, ' ').trim().slice(0, 80) || 'splunk-table',
  // How many rows a table has, wherever they are kept.
  count: t => t ? (t.dataset ? Number(t.dataset.total) || 0 : (t.rows || []).length) : 0,
  rowsText: n => n.toLocaleString() + (n === 1 ? ' row' : ' rows'),
  when(t){ const d = new Date(t); return isNaN(d) ? '' : d.toLocaleString([], {dateStyle: 'medium', timeStyle: 'short'}); },
  errText: e => String(e && e.message || e),
});
