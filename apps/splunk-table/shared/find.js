/* Find: "Find in results" and the sort, shared by the parts that read rows (rows, keep).
   The same text means the same rows whether the table is in memory or in the database.
   A view is {find: 'text', sort: {col, dir}}: what the rows part shows. Pure: no DOM, no state. */
const Find = Object.freeze({
  HINT: 'Words keep rows that contain them anywhere. Use column=text, column!=text, or column>number (also <, >=, <=) to look in one column. Put spaces inside "quotes".',
  tokens: text => String(text || '').match(/(?:[^\s"]+|"[^"]*")+/g) || [],
  rule: tok => tok.match(/^([^=<>!"]+)(!=|>=|<=|=|>|<)(.*)$/),

  // Splits the text into words and rules. A word matches any cell; a rule looks in one column.
  // Returns {test(row), problems: [short messages], active}.
  parse(text, fields){
    const lower = fields.map(f => String(f).toLowerCase());
    const tests = [], problems = [];
    for (const tok of Find.tokens(text)){
      const m = Find.rule(tok);
      if (!m){
        const w = tok.replace(/"/g, '').toLowerCase();
        if (w) tests.push(r => r.some(v => v != null && String(v).toLowerCase().includes(w)));
        continue;
      }
      const i = lower.indexOf(m[1].toLowerCase());
      if (i < 0){ problems.push('There is no column called ' + m[1] + '.'); continue; }
      const op = m[2], val = m[3].replace(/"/g, '');
      const cell = r => (r[i] == null ? '' : String(r[i]));
      if (op === '=') tests.push(r => cell(r).toLowerCase().includes(val.toLowerCase()));
      else if (op === '!=') tests.push(r => !cell(r).toLowerCase().includes(val.toLowerCase()));
      else {
        if (!Cells.isNum(val)){ problems.push(m[1] + op + ' needs a number after it.'); continue; }
        const n = Cells.num(val);
        const cmp = {'>': (a, b) => a > b, '<': (a, b) => a < b, '>=': (a, b) => a >= b, '<=': (a, b) => a <= b}[op];
        tests.push(r => Cells.isNum(r[i]) && cmp(Cells.num(r[i]), n));
      }
    }
    return {test: r => tests.every(t => t(r)), problems, active: tests.length > 0};
  },

  // The same rules as a condition for the database: {where, params, problems, active}. Each value is
  // a bound parameter; columns are the table's own names, matched from the fields people see.
  where(text, fields, columns){
    const lower = fields.map(f => String(f).toLowerCase());
    const parts = [], params = [], problems = [];
    const like = v => '%' + String(v).replace(/[\\%_]/g, c => '\\' + c) + '%';
    for (const tok of Find.tokens(text)){
      const m = Find.rule(tok);
      if (!m){
        const w = tok.replace(/"/g, '');
        if (!w) continue;
        params.push(like(w));
        const n = params.length;
        parts.push('(' + columns.map(c => 'CAST("' + c + '" AS TEXT) LIKE ?' + n + " ESCAPE '\\'").join(' OR ') + ')');
        continue;
      }
      const i = lower.indexOf(m[1].toLowerCase());
      if (i < 0){ problems.push('There is no column called ' + m[1] + '.'); continue; }
      const op = m[2], val = m[3].replace(/"/g, ''), col = '"' + columns[i] + '"';
      if (op === '=' || op === '!='){
        params.push(like(val));
        parts.push(op === '=' ? 'CAST(' + col + ' AS TEXT) LIKE ?' + params.length + " ESCAPE '\\'" : '(' + col + ' IS NULL OR CAST(' + col + ' AS TEXT) NOT LIKE ?' + params.length + " ESCAPE '\\')");
      } else {
        if (!Cells.isNum(val)){ problems.push(m[1] + op + ' needs a number after it.'); continue; }
        params.push(Cells.num(val));
        parts.push('CAST(' + col + ' AS REAL) ' + op + ' ?' + params.length);
      }
    }
    return {where: parts.join(' AND '), params, problems, active: parts.length > 0};
  },

  // A view that is safe to use whatever was stored or sent.
  view: v => ({find: v && typeof v.find === 'string' ? v.find : '',
    sort: v && v.sort && Number.isInteger(v.sort.col) ? {col: v.sort.col, dir: v.sort.dir < 0 ? -1 : 1} : {col: -1, dir: 1}}),

  // One page request for a table in the database, with the view's sort and filter.
  pageRequest(table, view, offset, limit){
    const d = table.dataset, v = Find.view(view), f = Find.where(v.find.trim(), table.fields, d.columns);
    return {req: {table: d.table, offset, limit, orderBy: v.sort.col >= 0 ? d.columns[v.sort.col] || '' : '', desc: v.sort.dir < 0, where: f.where, params: f.params}, find: f};
  },

  // A table in memory: the rows that match the view, in its order. Returns {rows, find, numeric}.
  rows(table, view){
    const v = Find.view(view), numeric = Cells.numeric(table.fields, table.rows);
    const find = Find.parse(v.find.trim(), table.fields);
    let rows = find.active ? table.rows.filter(r => { try { return find.test(r); } catch { return false; } }) : table.rows;
    const i = v.sort.col;
    if (i >= 0) rows = rows.slice().sort((a, b) => v.sort.dir * (numeric[i] ? Cells.num(a[i]) - Cells.num(b[i]) : String(a[i] ?? '').localeCompare(String(b[i] ?? ''))));
    return {rows, find, numeric};
  },
});
