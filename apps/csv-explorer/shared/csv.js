/* Csv: reads CSV text into typed rows, and writes rows back as CSV. Shared by importer (read) and
   export (line). Pure functions only: no DOM, no state. */
const Csv = Object.freeze({
  DELIMITERS: [',', ';', '\t', '|'],
  // Cells that mean "no value". They become NULL in the database.
  NULLS: Object.freeze(['', 'NA', 'N/A', 'n/a', 'null', 'NULL', 'NaN']),
  NUM: /^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/,
  NUM_GROUPED: /^[-+]?\d{1,3}(,\d{3})+(\.\d+)?$/,
  ISO_DATE: /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2})(\.\d+)?)?\s*(Z|[+-]\d{2}:?\d{2})?)?$/,
  SLASH_DATE: /^(\d{1,2})[/.](\d{1,2})[/.](\d{4})$/,

  // Splits text into rows of fields. Quoted fields may hold the delimiter, quotes ("") and line
  // breaks. Returns {rows, unclosed}: unclosed is true when the text ends inside a quoted field.
  parse(text, delim, maxRows = Infinity){
    const rows = [], n = text.length;
    let i = text.charCodeAt(0) === 0xFEFF ? 1 : 0, row = [];
    const plainEnd = k => { while (k < n){ const c = text[k]; if (c === delim || c === '\n' || c === '\r') break; k++; } return k; };
    while (i < n){
      let field = '';
      if (text[i] === '"'){
        i++;
        for (;;){
          const j = text.indexOf('"', i);
          if (j < 0){ row.push(field + text.slice(i)); rows.push(row); return {rows, unclosed: true}; }
          field += text.slice(i, j);
          if (text[j + 1] === '"'){ field += '"'; i = j + 2; continue; }
          i = j + 1; break;
        }
        const k = plainEnd(i); field += text.slice(i, k); i = k;   // text after the closing quote is kept
      } else {
        const k = plainEnd(i); field = text.slice(i, k); i = k;
      }
      row.push(field);
      if (i >= n) break;
      if (text[i] === delim){ i++; if (i >= n) row.push(''); continue; }
      if (text[i] === '\r' && text[i + 1] === '\n') i++;
      i++;
      rows.push(row); row = [];
      if (rows.length >= maxRows) return {rows, unclosed: false};
    }
    if (row.length) rows.push(row);
    return {rows, unclosed: false};
  },

  // The delimiter that splits the first lines into the same number of fields most often.
  detect(text){
    const sample = text.slice(0, 64 * 1024);
    let best = ',', score = -1;
    for (const d of Csv.DELIMITERS){
      const rows = Csv.parse(sample, d, 30).rows.filter(r => r.length > 1 || r[0] !== '');
      const counts = rows.slice(0, Math.max(1, rows.length - 1)).map(r => r.length);
      if (!counts.length || counts[0] < 2) continue;
      const s = counts.filter(c => c === counts[0]).length * 1000 + counts[0];
      if (s > score){ best = d; score = s; }
    }
    return best;
  },

  isNull: v => v == null || Csv.NULLS.includes(v.trim()),
  // A number cell, or null. "007" is not a number: leading zeros mark codes, which stay text.
  num(v){
    const s = v.trim();
    if (Csv.NUM.test(s)) return /^[-+]?0\d/.test(s) ? null : Number(s);
    if (Csv.NUM_GROUPED.test(s)) return Number(s.replace(/,/g, ''));
    return null;
  },
  pad: (x, w) => String(x).padStart(w, '0'),
  // An ISO date cell as YYYY-MM-DD[ HH:MM[:SS]], or null.
  iso(v){
    const m = v.trim().match(Csv.ISO_DATE);
    if (!m || +m[2] < 1 || +m[2] > 12 || +m[3] < 1 || +m[3] > 31) return null;
    let out = m[1] + '-' + Csv.pad(m[2], 2) + '-' + Csv.pad(m[3], 2);
    if (m[4] != null) out += ' ' + Csv.pad(m[4], 2) + ':' + m[5] + (m[6] ? ':' + m[6] : '') + (m[8] ? ' ' + m[8] : '');
    return out;
  },
  // A day/month/year or month/day/year cell as YYYY-MM-DD, or null. dayFirst picks the order.
  slash(v, dayFirst){
    const m = v.trim().match(Csv.SLASH_DATE);
    if (!m) return null;
    const [d, mo] = dayFirst ? [+m[1], +m[2]] : [+m[2], +m[1]];
    if (mo < 1 || mo > 12 || d < 1 || d > 31) return null;
    return m[3] + '-' + Csv.pad(mo, 2) + '-' + Csv.pad(d, 2);
  },

  // What each column holds: 'number' when every filled cell is a number, 'date' when every filled
  // cell is a date in one style, else 'text'. Slash dates are day-first when a first part passes 12.
  infer(rows, width){
    const cols = [];
    for (let c = 0; c < width; c++){
      let seen = 0, num = true, iso = true, slash = true, dayFirst = false, monthFirst = false;
      for (const r of rows){
        const v = r[c];
        if (Csv.isNull(v)) continue;
        seen++;
        if (num && Csv.num(v) === null) num = false;
        if (iso && Csv.iso(v) === null) iso = false;
        if (slash){
          const m = v.trim().match(Csv.SLASH_DATE);
          if (!m) slash = false;
          else { if (+m[1] > 12) dayFirst = true; if (+m[2] > 12) monthFirst = true; }
        }
        if (!num && !iso && !slash) break;
      }
      if (slash && dayFirst && monthFirst) slash = false;
      const type = !seen ? 'text' : num ? 'number' : iso || slash ? 'date' : 'text';
      cols.push({type, dayFirst: type === 'date' && !iso && dayFirst});
    }
    return cols;
  },

  // One cell as the database keeps it: a number, a date as text, text, or null.
  value(v, col){
    if (Csv.isNull(v)) return null;
    if (col.type === 'number') return Csv.num(v);
    if (col.type === 'date') return Csv.iso(v) || Csv.slash(v, col.dayFirst);
    return v;
  },

  // Header names: trimmed, never empty, never twice the same.
  fields(header, width){
    const out = [];
    for (let i = 0; i < width; i++){
      const base = String(header[i] == null ? '' : header[i]).trim() || 'Column ' + (i + 1);
      let name = base, k = 2;
      while (out.includes(name)) name = base + ' (' + k++ + ')';
      out.push(name);
    }
    return out;
  },

  // The whole file: {fields, types, rows, delimiter, ragged, unclosed}. Each row has one value per
  // field; ragged counts the lines whose number of fields differs from the header's.
  read(text){
    const delimiter = Csv.detect(text);
    const {rows, unclosed} = Csv.parse(text, delimiter);
    const lines = rows.filter(r => r.length > 1 || r[0].trim() !== '');
    if (!lines.length) return {fields: [], types: [], rows: [], delimiter, ragged: 0, unclosed};
    const width = lines[0].length, fields = Csv.fields(lines[0], width), body = lines.slice(1);
    const cols = Csv.infer(body, width);
    let ragged = 0;
    const out = body.map(r => {
      if (r.length !== width) ragged++;
      const row = new Array(width);
      for (let c = 0; c < width; c++) row[c] = Csv.value(r[c], cols[c]);
      return row;
    });
    return {fields, types: cols.map(c => c.type), rows: out, delimiter, ragged, unclosed};
  },

  // Writing: one cell, quoted when it holds a comma, a quote or a line break; then one line.
  cell(v){ const s = v == null ? '' : String(v); return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; },
  line: r => r.map(Csv.cell).join(','),
  DELIMITER_NAMES: Object.freeze({',': 'commas', ';': 'semicolons', '\t': 'tabs', '|': 'bars'}),
});
