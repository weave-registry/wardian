#!/usr/bin/env bash
# End-to-end test of Save as web page (ADR-2610080905): a suite (the loan planner), a page app
# (Mandelbrot), a module app (adder), and a suite made here whose own snapshot tries to put a script,
# an onerror and a javascript: link in the file.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir -p "$TMP/apps" "$TMP/out" "$TMP/data"
# A data folder that is not empty is not a first start, so the app list shows at once.
echo '{}' > "$TMP/data/layouts.json"
cp -R apps/loan-planner apps/mandelbrot apps/adder "$TMP/apps/"

# A suite whose parts try everything the cleaning must stop.
S="$TMP/apps/snap-test"
mkdir -p "$S/apps/plain" "$S/apps/evil" "$S/apps/slow"
cat > "$S/suite.json" <<'JSON'
{
  "format": 1,
  "title": "Snapshot test",
  "styles": ["style.css"],
  "apps": [
    { "name": "plain", "slot": "aside", "wrap": "<section class=\"card\">" },
    { "name": "evil", "slot": "main", "wrap": "<section class=\"card\">" },
    { "name": "slow", "slot": "main", "wrap": "<section class=\"card\">" }
  ]
}
JSON
cat > "$S/style.css" <<'CSS'
@import url("https://fonts.googleapis.com/css2?family=Inter");
body { font-family: Inter, system-ui, sans-serif; }
.card { padding: 12px; border: 1px solid #ccc; background: #fff url("https://example.com/bg.png"); }
.card h2 { color: rgb(1, 2, 3); }
CSS
cat > "$S/apps/plain/view.html" <<'HTML'
<h2>Plain part</h2>
<input id="t" type="text" aria-label="Words">
<select id="s" aria-label="Pick"><option>one</option><option>two</option></select>
<input id="c" type="checkbox" aria-label="Tick">
<textarea id="n" aria-label="Notes"></textarea>
HTML
echo "Kernel.register({ name: 'plain', init(ctx) {} });" > "$S/apps/plain/app.js"
echo '<h2>Evil part</h2><p>shown on screen</p>' > "$S/apps/evil/view.html"
# Split "<script>" so the frame can inline this file (SPEC.md 6.3); the HTML it returns is whole.
cat > "$S/apps/evil/app.js" <<'JS'
Kernel.register({
  name: 'evil',
  init(ctx) {},
  snapshot(ctx) {
    return '<h2>Evil part</h2><p id="evil-text">rendered by snapshot</p>'
      + '<scr' + 'ipt>window.ran = 1</scr' + 'ipt><img src="x" onerror="window.ran = 2"><img src="data:image/png;base64,iVBORw0KGgo=" onload="window.ran = 3" alt="dot">'
      + '<a href="javascript:alert(1)" id="bad">bad link</a><a href="#evil-text" id="good">good link</a>'
      + '<svg width="20" height="20"><scr' + 'ipt>1</scr' + 'ipt><foreignObject><div>fo</div></foreignObject><circle r="5" cx="10" cy="10" onclick="x()"/></svg>'
      + '<div id="styled" style="background: url(https://example.com/x.png); color: red">styled</div>'
      + '<form action="https://example.com/steal"><input name="q" value="kept"></form>'
      + '<iframe src="https://example.com/"></iframe><object data="https://example.com/x"></object><embed src="https://example.com/y">'
      + '<link rel="stylesheet" href="https://example.com/s.css"><meta http-equiv="refresh" content="0;url=https://example.com/">'
      + '<style>@import "https://example.com/i.css"; .x { background: url(https://example.com/z.png) }</style>'
      // More the cleaner must stop (claims audit B14): each is checked by name in snapshot-e2e.js.
      + '<a href="vbscript:msgbox(1)" id="vb">vb link</a><img srcset="https://example.com/s.png 1x" alt="ss">'
      + '<svg width="20" height="20"><a xlink:href="javascript:alert(1)"><text>x</text></a><image xlink:href="https://example.com/x.png"/>'
      + '<animate attributeName="href" to="javascript:alert(1)"/><set attributeName="href" to="javascript:alert(2)"/></svg>'
      + '<style>@font-face { font-family: Evil; src: url(https://example.com/f.woff) } .y { width: expression(alert(1)) } .z { background: u\\72l(https://example.com/esc.png) }</style>'
      + '<div id="esc" style="width: expression(alert(1)); background: u\\72l(https://example.com/esc2.png); color: blue">escaped</div>';
  }
});
JS
echo '<h2>Slow part</h2>' > "$S/apps/slow/view.html"
echo "Kernel.register({ name: 'slow', init(ctx) {}, snapshot() { return new Promise(() => {}); } });" > "$S/apps/slow/app.js"

DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PID=$!
disown "$PID"
BASE=
for _ in $(seq 100); do
  BASE=$(sed -n 's/^listening on \(http:[^ ]*\).*/\1/p' "$TMP/server.log" | head -1)
  [ -n "$BASE" ] && curl -sf "$BASE/api/status" >/dev/null && break
  sleep 0.1
done
[ -n "$BASE" ] || { cat "$TMP/server.log"; echo "the server did not start"; exit 1; }

BASE="$BASE" OUT="$TMP/out" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/snapshot-e2e.js
