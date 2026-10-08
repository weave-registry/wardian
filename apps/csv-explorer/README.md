# CSV explorer

A Wardian **suite**: five sealed apps that open a CSV file and let you page, sort, filter, summarise,
chart and export it. The rows live in the app's own SQLite database (the `db` capability), so
100,000 rows stay fast and are still there after a reload. It needs no WebAssembly. Every part is
plain JavaScript, and the database does the heavy work.

| App | Slot | Job | Contract |
|---|---|---|---|
| `importer` | aside | Opens a file (choose, drop, or **Load sample**), reads it, and loads it into a new table in batches, with a progress bar. On start it shows what is already loaded. | emits `dataset:ready` (retained); `db`, `asset` |
| `table` | main | Shows 100 rows at a time with `db.page`. Click a column to sort; type under **Filter** to keep rows. | listens `dataset:ready`; emits `view:changed` (retained); `db` |
| `stats` | main | One line per column: filled, empty, distinct, then min/mean/max, a date range, or the most common values. | listens `dataset:ready`, `view:changed`; `db` |
| `chart` | main | A bar chart of one column, counted with `GROUP BY`: number ranges, months, years or decades, or the top 15 text values. | listens `dataset:ready`, `view:changed`; `db` |
| `export` | main | Downloads the rows the table keeps, in its order, as CSV. | listens `dataset:ready`, `view:changed`; `db`, `claude:downloads` |

How a change flows: **importer** loads a file and emits `dataset:ready`. This message names the
table, its columns and their types. **table** draws the first page and emits `view:changed` (the
filter and the sort). **stats** and **chart** compute again when the filter changes. **export**
saves what the view describes. Only the importer writes to the database. The other parts only read.

## Files

| File | What it is |
|---|---|
| `shared/csv.js` | Reads CSV (quotes, delimiters inside quotes, line breaks, the delimiter itself), finds the column types, and writes CSV lines. |
| `shared/filter.js` | Turns the **Filter** text into a `WHERE` condition with bound parameters. table, stats, chart and export all use it, so they all keep the same rows. |
| `shared/data.js` | The shape of a dataset message, checks on it, and number and date formats. |
| `apps/chart/draw.js` | The SVG bar chart. Only the chart part loads it. |
| `sample/cities.csv` | 320 made-up cities, for **Load sample**. |
| `ui/` | The Wardian component library: theme, button, field, card, table, badge. |

`suite.json` lists the three files in `shared/` under "scripts", so every frame gets its own copy.

## Reading a file

- **Delimiter.** The importer tries commas, semicolons, tabs and bars. It picks the one that splits
  the first lines into the same number of values most often.
- **Header.** The first line names the columns. An empty name becomes "Column N". A repeated name
  gets " (2)" added.
- **Types.** A column is `number` when every filled cell is a number (`1234.5`, `-3e2`, `1,234`). It is
  `date` when every filled cell is a date in one style (`2024-03-01`, `2024-03-01 14:05`,
  `01/03/2024`). Every other column is `text`. Dates are stored as `YYYY-MM-DD`. A slash date counts as
  day-first when a first part is more than 12, and month-first otherwise. A value with a leading zero,
  such as `007`, stays text, so codes keep their zeros.
- **Empty cells.** An empty cell, `NA`, `N/A`, `null` and `NaN` are all stored as empty (NULL).
- **Bad lines.** A line with too few values gets empty cells. Extra values are left out. The
  importer says how many lines were like that.

## Safe SQL

No text from a file or from the **Filter** box is ever put into SQL. The database columns are
called `c0`, `c1`, … and the tables `ds_<time>`. The importer makes these names, and every part
checks them (`Data.valid`) before it uses one. Every value you type becomes a bound parameter
(`?1`, `?2`, …). The table also has a column `_all`, with every value of the row in lower case. A
filter word then needs only one `LIKE`, however many columns the file has.

## Filter

| You type | Rows kept |
|---|---|
| `europe` | rows with "europe" in any column (case does not matter) |
| `continent=asia` | text: the column contains it; number: equals it; date: starts with it (`founded=19`) |
| `country!=norland` | the opposite |
| `population>1000000` | also `<`, `>=`, `<=`. Numbers compare as numbers. Dates and text compare as text. |
| `rainfall_mm=` | rows where the column is empty (`rainfall_mm!=`: where it is filled) |
| `"has metro"=yes` | put a name or value with spaces in quotes |

Words and rules combine with AND.

## Saved state

The dataset is kept in the database, so it survives a reload, and it survives a restart of
Wardian. The table `csv_meta` holds the current dataset's description. Loading a new file builds a
new table first, then drops the old one. A load that fails or that you stop leaves the previous
data in place. When the importer starts, it drops any table that an interrupted load left behind.
**Remove data** drops the table.

## Limits

- One dataset at a time. A new file replaces the previous one.
- Files up to 150 MB. The browser reads the whole file before it loads it.
- The database may hold 1 GB, and one query may run for 10 seconds (Wardian's limits for `db`).
- Sorting uses one column at a time. Rows with an empty cell come first when the order is smallest
  first.
- The file must be UTF-8 (plain ASCII is fine too).
- Text matching ignores case for the letters A–Z only. This is how SQLite's `LIKE` works.

Measured on a laptop with a 100,000-row, 7-column file (5 MB): the load took about a second,
sorting and the last page took about 0.2 s, and a filter took about 0.3 s.
