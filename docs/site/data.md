# Keeping data

A suite app keeps data in one of two places, kept by Wardian in its data folder. Choose by size and
shape.

| Use | For | Capability | Example |
|---|---|---|---|
| `ctx.store` | small settings and state: the last inputs, a list of habits | `storage` | [`habit-tracker`](/docs/examples#habit-tracker) |
| `ctx.cap('db')` | data: rows to page, sort, filter or share | `db` | [`csv-explorer`](/docs/examples#csv-explorer) |

A page app has neither. It is sandboxed and keeps nothing between visits. To keep data in a page,
let the user download it and load it again. Or make it a suite.

## Small state: `storage`

Declare `storage` in the part's `caps`, in `suite.json` and in `Kernel.register`:

```json
{ "name": "inputs", "slot": "aside", "caps": ["storage"] }
```

Then read and write by key:

```js
const saved = ctx.store.get('loan') || { amount: 300000, rate: 6, years: 30 };
ctx.store.set('loan', { amount, rate, years });
```

- `get` is synchronous. It returns `null` for a missing key.
- Values must be JSON-compatible: no `Date`, `Map` or class instances. Store a date as a string.
- Each part has its own store. Two parts never see each other's keys.
- A part may keep 1 MB this way, and a whole package 5 MB.

Wardian keeps the data in `DATA_DIR/state/`, so a restart, another browser or cleared site data does
not lose it (ADR-2610071055).

## Tables: `db`

A part that declares `db` gets the package's own SQLite database, kept in
`DATA_DIR/db/<app>.sqlite` (ADR-2610071219).

```js
const db = await ctx.cap('db');

await db.insertRows({
  table: 'runs', columns: ['n', 'x'],
  rows: [[1, 980], [2, 1900]],
  create: true,                 // make the table if it is missing
});

const page = await db.page({
  table: 'runs', offset: 0, limit: 100,
  orderBy: 'x', desc: true,
  where: 'n > ?', params: [1],
});
// page = { columns, rows, total, offset }

const { columns, rows } = await db.query({ sql: 'SELECT n, avg(x) FROM runs GROUP BY n' });
```

| Method | Does |
|---|---|
| `insertRows({table, columns, rows, create, replace})` | adds rows; `create` makes the table if it is missing, `replace` drops it and makes it again with these columns |
| `page({table, offset, limit, orderBy, desc, where, params})` | one page of rows and the total; `limit` at most 1,000; read-only |
| `query({sql, params})` | one SQL statement; at most 1,000 rows back |
| `tables()` | the tables in this database |
| `readPage({package, ...page})` | a page of another package's table, read-only, after the user allows it |
| `searchInto({search, earliest, latest, table})` | loads a Splunk search into a table, up to 1,000,000 rows; needs `splunk` too |

### Show large tables a page at a time

Never load every row into the frame. Ask for the page the viewer sees:

```js
async function show(offset) {
  const { rows, total } = await db.page({ table: 'data', offset, limit: 100, orderBy: sortCol, desc });
  render(rows);
  status.textContent = `${offset + 1}–${offset + rows.length} of ${total.toLocaleString()}`;
}
```

`csv-explorer` pages 100,000 rows this way, and its sort, filter and export all run in SQL.

### Always use placeholders

Put user input in `params`, never in the SQL text:

```js
// Right
db.page({ table: 'data', where: 'city LIKE ?', params: [`%${text}%`] });
// Wrong: the user's text becomes SQL
db.page({ table: 'data', where: `city LIKE '%${text}%'` });
```

Table and column names may hold only letters, digits and `_`, and cannot be placeholders. Check a column name against `columns` from the table before
you use it in `orderBy` or in SQL.

### What Wardian refuses

Apps write their own SQL, so Wardian keeps each one inside its own file:

- no `ATTACH` or `DETACH`, no loading extensions, no pragma that sets anything;
- another app's tables can be read only, and only with the user's permission;
- a database may hold 1 GB;
- a statement that runs longer than 10 seconds is stopped.

## Share a large table with another package

A channel message holds at most 256 KB. To hand a large table to another package, send a
**dataset reference** instead of the rows:

```js
ctx.channel('splunk.table').send({
  dataset: { package: 'splunk-table', table: 'search', total, columns, fields },
});
```

The receiver declares `db` and reads the pages it needs:

```js
const page = await db.readPage({ package: msg.dataset.package, table: msg.dataset.table, offset: 0, limit: 1000 });
```

The user is asked once whether the receiver may read that package's tables. The Splunk table app
and the USL lab work this way. See [Apps that talk to each other](/docs/channels).

## Take the data with you

**Download** on an app's page saves the app as a `.wardian` file. Tick **Include my data** to add
its `storage`, its layout and its tables. Wardian lists what goes in first. Keys, accounts,
permission answers and history never go in. See [Share, import and remove apps](/docs/sharing).
