# Run Wardian for others

Wardian is built for one person on one computer. You can run it for a small team, but first decide
who counts as an **admin**: the person who can change settings, import and export apps, browse
Drive and run Splunk searches.

## The default: this machine only

With no `ADMIN_TOKEN`, every program and browser on the same machine is an admin. That is fine for
one person on a laptop. So Wardian then listens on this machine only: `ADDR` must be a loopback
address (`127.0.0.0/8`, `::1` or `localhost`). Asked to listen anywhere else without a token,
Wardian refuses to start and says to set `ADMIN_TOKEN`.

## With an admin token

Set `ADMIN_TOKEN` to a long random string to listen on other addresses:

```
ADMIN_TOKEN=$(openssl rand -hex 32) ADDR=0.0.0.0:8000 wardian
```

Then whoever sends the token is an admin, and nobody else is, this machine included. Settings asks
for the token, and **Unlock** opens it. At start, Wardian prints who counts as an admin.

Everyone else can open the apps, but the server does less for them:

| A viewer who is not an admin… | Because |
|---|---|
| cannot change settings, import, export or browse Drive | these change the server |
| cannot run Splunk searches, use `db`, or see background jobs | these reach data the server holds |
| cannot see an app's history | it may hold old data |
| keeps layouts and `storage` data in their own browser | the server does not let them write its state |

So apps that need `db`, `splunk` or the history are for admins. Plan for that before you share a
Wardian with a team.

## Docker

```
ADMIN_TOKEN=<long random string> docker compose up -d --build
```

Docker needs the token, because it listens on `0.0.0.0`, and its requests do not come from the same
machine.

## Put it behind HTTPS

Wardian speaks plain HTTP. To reach it across a network, put it behind a reverse proxy that ends
TLS, such as Caddy or nginx, and send the admin token only over HTTPS.

## Keep the data folder safe

Everything Wardian knows is in `DATA_DIR` (by default `./data`): the apps, their history, their
data, your permission answers and the keys for Claude, Splunk and Drive. Key files are written
readable by their owner only.

- **Back it up** like any folder. Stop Wardian first, or copy the SQLite files in `db/` with a tool
  that understands SQLite.
- **Do not commit it.** Git ignores it in the repository.
- **Read `wardian.log`** after a surprise stop. It records every start and stop with the time in
  UTC. A start with no stop before it means the previous run was killed.

[Settings and environment](/docs/config) lists every file and variable.

## What a viewer's browser keeps

Layouts, each app's `storage` data and the latest channel messages live in `DATA_DIR/state/`
(ADR-2610071055), so they follow the server, not the browser. For a viewer the host does not let
write, the browser keeps its own copy.

## Before you rely on it

Wardian is version 0.4. ADR-2610072033 lists what must be true before version 1.0, and
[Security model](/docs/security) says exactly what is and is not protected. Read both before you
let people you do not know use your Wardian.
