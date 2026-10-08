/* Data: the current dataset as every part sees it, and how to show its values. Shared by all parts.
   A dataset is the retained message on "dataset:ready":
     {table, name, fields, columns, types, total, loadedAt, delimiter}
   fields are the CSV's own column names, shown to people; columns are the database's names for them
   (c0, c1, ...), made by the importer, so no name from a file ever reaches SQL. The table also has a
   column "_all": every value of the row in lower case, for "Filter" words that match any column.
   Pure functions only: no DOM, no state. */
const Data = Object.freeze({
  TABLE: /^ds_[a-z0-9]{1,40}$/,
  COLUMN: /^c\d{1,4}$/,
  TYPES: Object.freeze(['number', 'date', 'text']),

  // The dataset from a message, or null when it is missing or not well formed.
  valid(d){
    if (!d || !Data.TABLE.test(d.table) || !Array.isArray(d.fields) || !Array.isArray(d.columns) || !Array.isArray(d.types)) return null;
    const n = d.columns.length;
    if (!n || d.fields.length !== n || d.types.length !== n) return null;
    if (!d.columns.every(c => Data.COLUMN.test(c)) || !d.types.every(t => Data.TYPES.includes(t))) return null;
    return d;
  },
  // A column for SQL. Only names that passed valid() are ever quoted.
  q: c => '"' + c + '"',
  // A filter condition joined to another one with AND.
  and: (...parts) => parts.filter(Boolean).map(p => '(' + p + ')').join(' AND '),

  count: n => Number(n || 0).toLocaleString(),
  rows: n => Data.count(n) + (n === 1 ? ' row' : ' rows'),
  // A number for reading: whole numbers with separators, others with up to 4 significant decimals.
  num(v){
    if (v == null || v === '') return '';
    const x = Number(v);
    if (!isFinite(x)) return String(v);
    if (Number.isInteger(x)) return x.toLocaleString();
    const a = Math.abs(x);
    return x.toLocaleString(undefined, {maximumFractionDigits: a >= 100 ? 1 : a >= 1 ? 2 : 4});
  },
  cell: (v, type) => v == null ? '' : type === 'number' ? Data.num(v) : String(v),
  when(t){ const d = new Date(t); return isNaN(d) ? '' : d.toLocaleString([], {dateStyle: 'medium', timeStyle: 'short'}); },
  errText: e => String(e && e.message || e),
  // A safe file name made from the dataset's name.
  fileStem: name => String(name || 'data').replace(/\.[a-z]{2,4}$/i, '').replace(/[^\w.\- ]+/g, ' ').trim().slice(0, 80) || 'data',
});
