/* Filter: turns what someone types under "Filter" into a WHERE condition with bound parameters.
   Shared by table, stats, chart and export, so the same text keeps the same rows everywhere.
   Every value typed goes into params, never into the SQL text; columns come from the dataset.
     word             rows that contain "word" in any column
     column=text      text: contains; number: equals; date: starts with (founded=1907)
     column!=text     the opposite
     column>10        also <, >=, <=; numbers compare as numbers, dates and text as text
     column=          rows where the column is empty (column!= : where it is filled)
   Put spaces inside "quotes": city="new port" or "unit price">5. Pure: no DOM, no state. */
const Filter = Object.freeze({
  HINT: 'Words match any column. Use column=text, column!=text, or column>value (also <, >=, <=) to look in one column; column= finds empty cells. Put spaces inside "quotes".',
  tokens: text => String(text || '').match(/(?:[^\s"]+|"[^"]*")+/g) || [],
  rule: tok => tok.match(/^("[^"]*"|[^=<>!"]+)(!=|>=|<=|=|>|<)(.*)$/),
  unquote: s => s.replace(/"/g, ''),
  like: v => '%' + String(v).replace(/[\\%_]/g, c => '\\' + c) + '%',
  // The column a name in a rule means: the field with that name, ignoring case, spaces and _.
  column(name, d){
    const key = s => String(s).toLowerCase().replace(/[\s_]+/g, '');
    const i = d.fields.findIndex(f => key(f) === key(name));
    return i < 0 ? null : {i, col: Data.q(d.columns[i]), type: d.types[i]};
  },

  // {where, params, problems: [short sentences], active}. A view is {find} or a plain string.
  where(text, d){
    const parts = [], params = [], problems = [];
    const p = v => { params.push(v); return '?' + params.length; };
    for (const tok of Filter.tokens(text)){
      const m = Filter.rule(tok);
      if (!m){
        const w = Filter.unquote(tok).toLowerCase();
        if (w) parts.push('"_all" LIKE ' + p(Filter.like(w)) + " ESCAPE '\\'");
        continue;
      }
      const name = Filter.unquote(m[1]).trim(), op = m[2], val = Filter.unquote(m[3]);
      const c = Filter.column(name, d);
      if (!c){ problems.push('There is no column called ' + name + '.'); continue; }
      if (val === '' && (op === '=' || op === '!=')){ parts.push(c.col + (op === '=' ? ' IS NULL' : ' IS NOT NULL')); continue; }
      if (c.type === 'number'){
        const n = Csv.num(val);
        if (n === null){ problems.push(name + op + ' needs a number after it.'); continue; }
        parts.push(op === '!=' ? '(' + c.col + ' IS NULL OR ' + c.col + ' != ' + p(n) + ')' : c.col + ' ' + op + ' ' + p(n));
      } else if (op === '=' || op === '!='){
        const pat = c.type === 'date' ? String(val).replace(/[\\%_]/g, x => '\\' + x) + '%' : Filter.like(val);
        parts.push(op === '=' ? c.col + ' LIKE ' + p(pat) + " ESCAPE '\\'" : '(' + c.col + ' IS NULL OR ' + c.col + ' NOT LIKE ' + p(pat) + " ESCAPE '\\')");
      } else parts.push(c.col + ' ' + op + ' ' + p(val));
    }
    if (params.length > 100) return {where: '', params: [], problems: ['That is too many words for one filter (at most 100).'], active: false};
    return {where: Data.and(...parts), params, problems, active: parts.length > 0};
  },

  // A view as the table part sends it: {table, find, sort: {col: index or -1, desc}}.
  view: (v, d) => ({
    find: v && d && v.table === d.table && typeof v.find === 'string' ? v.find : '',
    sort: v && d && v.table === d.table && v.sort && Number.isInteger(v.sort.col) && v.sort.col < d.columns.length ? {col: v.sort.col, desc: !!v.sort.desc} : {col: -1, desc: false},
  }),
  // One page request for the dataset, with the view's filter and sort.
  pageRequest(d, view, offset, limit){
    const v = Filter.view(view, d), f = Filter.where(v.find, d);
    return {req: {table: d.table, offset, limit, orderBy: v.sort.col >= 0 ? d.columns[v.sort.col] : '', desc: v.sort.desc, where: f.where, params: f.params}, filter: f};
  },
});
