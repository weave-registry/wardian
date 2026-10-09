# Splunk

Apps that declare the `splunk` capability can run Splunk searches. The app never sees the Splunk
address or credentials: Wardian runs each search with one account that you set up. The example
[`splunk-table`](/docs/examples#splunk-table) runs a search, keeps the rows in its database, and
hands them to the USL lab on a channel.

## Set it up

1. In Splunk, make a token (Settings → Tokens) for a user with a **read-only role** that sees only
   the indexes these apps need. That role is the real limit on what an app can read.
2. In Wardian, open **Settings → Splunk**. Type your Splunk's address as in your browser, or paste
   it from the browser's address bar, such as `https://splunk.example.com/en-US/app/search/search`.
   Then type the token (or a username and password).
3. Press **Test and save**. Wardian calls Splunk first and saves only settings that work.

Wardian finds Splunk's API itself (ADR-2610091500). The address in your browser is Splunk Web,
while Wardian needs Splunk's management API, usually on port 8089 of the same host:

- Wardian keeps the scheme (`https` when none is given), the host and the port, and drops the path.
- It tries the address as given, port included, then the same host on port 8089. The first that
  answers as Splunk's API is saved, and the result says which, as in "Splunk's API is on port 8089;
  saved https://splunk.example.com:8089."
- When Splunk's API refuses the account, Wardian stops there and says so: the address is right,
  the token or password is not.
- No other host or port is tried. A Splunk whose API is on another port needs that port typed, as
  in `https://splunk.example.com:9089`.

### Splunk's own certificate

Most Splunk servers protect the API with Splunk's built-in certificate, `SplunkServerDefaultCert`,
issued by `SplunkCommonCA`. No computer trusts it by itself. When Wardian meets it, the result names
it and offers one button, **Trust Splunk's own certificate and try again**, which ticks **Allow a
self-signed certificate** and saves again. Nothing is trusted without that click.

A certificate from another authority, such as your company's, is named with its issuer. Put that
authority's certificate, as a PEM file on the Wardian server, in **CA file**.

The environment variables `SPLUNK_URL`, `SPLUNK_TOKEN` (or `SPLUNK_USERNAME` and
`SPLUNK_PASSWORD`), `SPLUNK_INSECURE_TLS` and `SPLUNK_CA_FILE` do the same, used only when nothing
is saved in Settings. They are used as given: Wardian does not look for the API port for them.
`WARDIAN_SPLUNK_API_PORT` changes the port setup tries after the one given (8089 by default).

## Who may search

- The first time an app searches, Wardian asks you. The answer is listed under
  **App permissions**, where you can revoke it.
- The server checks that answer again on every search, and checks that the part declares `splunk`
  in `suite.json`.
- Splunk searches run only for an admin: from a browser on this machine, or with `ADMIN_TOKEN`.
  See [Run Wardian for others](/docs/operate).

## Search

```js
const splunk = await ctx.cap('splunk');          // null in a host without Splunk
const { ready } = await splunk.status();         // false until Settings → Splunk is done
const { fields, rows, truncated } = await splunk.search({
  search: 'index=loadtest | stats avg(tput) AS x BY concurrency | table concurrency x',
  earliest: '-7d',                               // Splunk time modifiers; '' means all time
});
```

- `fields` is in the order the search names them. Fields that start with `_` are left out.
- Each row is a list of values in that order.
- At most 10,000 rows come back; `truncated` says when there were more.
- A search that does not start with `|` or `search` gets `search` added.

## More than 10,000 rows

Load the search into the app's own database, then page it. This needs `db` as well as `splunk`:

```js
const db = await ctx.cap('db');
const big = await db.searchInto({ search: 'index=web | table host status ms', earliest: '-24h', table: 'search' });
// up to 1,000,000 rows, read in chunks of 50,000
// big = { table, columns, fields, total, truncated }
const first = await db.page({ table: 'search', offset: 0, limit: 100 });
```

## Long searches run in the background

A search over weeks of data can take minutes. Wardian runs Splunk searches, loads into a table and
Claude requests as background jobs on the server (ADR-2610072118). The promise works as before, but
no browser connection waits on it. You can open other apps meanwhile, and leaving the app does not
stop the search.

The **Jobs** button next to **Make an app** shows how many are running. Its list shows each one's
app, search, rows loaded and time, with **Cancel**. When a job finishes while its app is closed, the
button says so.

An app can pick up its own job when it opens again:

```js
const jobs = await splunk.jobs();                // this app's jobs of the last hour, newest first
const running = jobs.find(j => j.state === 'running' && j.kind === 'splunk.into');
if (running) {
  const big = await splunk.wait(running.id);     // the same answer searchInto gives
}
await splunk.cancel(id);                         // also cancels the search on the Splunk server
```

| Field of a job | Meaning |
|---|---|
| `kind` | `splunk.search`, `splunk.into` or `ai.sample` |
| `state` | `running`, `done`, `failed` or `cancelled` |
| `progress` | the rows loaded so far, or `null` |
| `started`, `ended`, `elapsed` | timing |

Jobs are kept in memory for an hour, at most 50 per app. Restarting Wardian forgets them. Rows
already loaded into a table stay when a job is cancelled.
