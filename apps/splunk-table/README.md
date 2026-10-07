# Splunk table

Runs a Splunk search on the Wardian server and shows every row it returns, so you can check the
data before an app uses it. The latest table goes out on the channel `splunk.table`; the USL lab
receives it and lets you choose which columns are load, throughput and response time.

| Part | Does | Contract |
|---|---|---|
| table | search box, ready-made searches, AI-written search (`claude:sample`), the table (sort by any column), CSV download (`claude:downloads`) | caps `storage`, `splunk`, `claude:sample`, `claude:downloads`; sends `splunk.table` |

The message on `splunk.table` holds `title`, `fields`, `rows`, `search`, `range`, `rangeLabel`, `at`,
`seconds`, `truncated` and `cut` (rows left out to fit one 256 KB message). A ready-made search also
sends `about` (what each column means), `units` and `use` (which columns are load, throughput and
response time).

The live-traffic searches keep a `minutes` column: how many minutes of traffic each row averages. A row
that rests on few minutes deserves less trust than one that rests on hundreds.
