#!/usr/bin/env bash
# Live: an export just under the 100 MB limit, imported again into a fresh Wardian
# (ADR-2610071248; ADR-2610072033, "Real services"). The limit is measured on the finished
# .wardian file and equals what import accepts, so every export can be imported again.
#
# Needs no credentials, only about 600 MB of free disk. LIVE_EXPORT=0 skips it; LIVE_EXPORT_MB
# (default 95) sets the size aimed at.
#
# What it proves: an app with ~50 MB of incompressible files and ~45 MB of table rows, with its
# saved data, downloads as one valid .wardian file under 100 MB with every byte intact; a second
# Wardian with an empty data folder imports it with its data, and has the same files, the same
# saved data and the same table. Then one file more takes the app over the limit, and the export
# is refused with a message, not cut short.
source "$(dirname "$0")/lib.sh"
[ "${LIVE_EXPORT:-1}" != 0 ] || skip "LIVE_EXPORT=0"
MB="${LIVE_EXPORT_MB:-95}"
FREE_MB=$(df -Pm "$TMP" | awk 'NR==2 {print $4}')
[ "$FREE_MB" -ge 600 ] || skip "needs about 600 MB free in $TMP, has $FREE_MB MB"
LIMIT=$((100 * 1024 * 1024))

# About half the size as the app's own files (two of them; a file may hold 64 MB), the rest as rows
# in its database (the `db` capability), which goes in the file as .wardian/data/tables.sqlite.
FILES_MB=$((MB / 2))
TABLE_MB=$((MB - FILES_MB))
mkdir -p "$TMP/a-apps" "$TMP/b-apps"
cp -R "$ROOT/apps/splunk-table" "$TMP/a-apps/"
mkdir -p "$TMP/a-apps/splunk-table/media"
say "writing $FILES_MB MB of random bytes into the app's files"
# Random, so deflate cannot shrink them.
for i in 1 2; do
  head -c $((FILES_MB * 1024 * 1024 / 2)) /dev/urandom >"$TMP/a-apps/splunk-table/media/part-$i.bin"
done
start_wardian "$TMP/a-apps"
A=$BASE

api POST /api/state/apps/splunk-table '{"app":"search","key":"state","value":{"s":"index=web | stats count","name":"live export"}}' >/dev/null
[ "$(status)" = 200 ] || fail "saving app data failed: $(cat "$TMP/body")"
say "loading about $TABLE_MB MB of rows into the app's database, 1,000 rows a call"
ROWS=$(python3 - "$A" "$TABLE_MB" <<'PY'
import base64, json, os, sys, urllib.request
base, mb = sys.argv[1], int(sys.argv[2])
row_bytes = 1800                      # 1,000 rows of this fit in one 2 MB request
# Base64 text deflates to about 3/4 of its size, so write 4/3 as much to land near `mb` in the file.
calls = (mb * 1024 * 1024 * 4 // 3) // (row_bytes * 1000) if mb > 0 else 0
n = 0
for c in range(calls):
    rows = []
    for _ in range(1000):
        rows.append([n, "web-%d" % (n % 20), base64.b64encode(os.urandom(row_bytes * 3 // 4)).decode()])
        n += 1
    body = json.dumps({"package": "splunk-table", "app": "keep", "table": "big", "columns": ["n", "host", "blob"], "rows": rows, "create": True}).encode()
    req = urllib.request.Request(base + "/api/db/insert", body, {"Content-Type": "application/json"})
    out = json.load(urllib.request.urlopen(req, timeout=120))
    assert out.get("inserted") == 1000, out
print(n)
PY
) || fail "loading the table failed"
say "$ROWS rows"

say "downloading splunk-table.wardian with its data"
FILE="$TMP/splunk-table.wardian"
t0=$(date +%s)
R=$(curl -sS -o "$FILE" -D "$TMP/headers" -w '%{http_code} %{size_download}' "$A/api/apps/splunk-table/export?data=1")
SECS=$(( $(date +%s) - t0 ))
CODE=${R%% *}; SIZE=${R##* }
[ "$CODE" = 200 ] || fail "the export answered $CODE: $(head -c 600 "$FILE")"
grep -qi '^content-type: application/vnd.wardian+zip' "$TMP/headers" || fail "not served as application/vnd.wardian+zip"
say "$((SIZE / 1024 / 1024)) MB ($SIZE bytes) in $SECS s"
[ "$SIZE" -le "$LIMIT" ] || fail "the file is over the 100 MB limit"
[ "$SIZE" -ge $((MB * 1024 * 1024 * 85 / 100)) ] || fail "the file is $((SIZE / 1024 / 1024)) MB, well short of the $MB MB aimed at"

say "checking every entry of the file"
python3 - "$FILE" "$TMP/a-apps/splunk-table" <<'PY' || fail "the file does not hold the app intact"
import hashlib, sys, zipfile
path, src = sys.argv[1], sys.argv[2]
z = zipfile.ZipFile(path)
bad = z.testzip()
assert bad is None, f"bad CRC in {bad}"
names = z.namelist()
assert all(n.startswith("splunk-table/") for n in names), "an entry outside the package folder"
for need in ["export.json", "data/storage.json", "data/tables.sqlite"]:
    assert "splunk-table/.wardian/" + need in names, f"missing .wardian/{need}"
for i in (1, 2):
    rel = f"media/part-{i}.bin"
    assert hashlib.sha256(z.read("splunk-table/" + rel)).digest() == hashlib.sha256(open(f"{src}/{rel}", "rb").read()).digest(), f"{rel} differs"
tables = z.getinfo("splunk-table/.wardian/data/tables.sqlite").file_size
print(f"  {len(names)} entries, CRCs good, media identical, tables.sqlite {tables // (1024 * 1024)} MB")
PY

say "importing it, with its data, into a second Wardian that starts empty"
INSTANCE=b start_wardian "$TMP/b-apps"
B=$BASE
R=$(curl -sS -o "$TMP/import" -w '%{http_code}' -X POST -H 'Content-Type: application/zip' --data-binary "@$FILE" "$B/api/import?name=splunk-table.wardian&data=1")
[ "$R" = 200 ] || fail "import answered $R: $(head -c 400 "$TMP/import")"
say "import: $(head -c 300 "$TMP/import")"
for i in 1 2; do
  cmp -s "$TMP/a-apps/splunk-table/media/part-$i.bin" "$TMP/b-apps/splunk-table/media/part-$i.bin" || fail "media/part-$i.bin differs after import"
done
OUT=$(api GET /api/state/apps/splunk-table)
[ "$(echo "$OUT" | json 'j["search"]["state"]["name"]')" = "live export" ] || fail "the saved data did not come across: $OUT"
OUT=$(api POST /api/db/page '{"package":"splunk-table","app":"keep","table":"big","offset":0,"limit":1,"orderBy":"n","desc":true}')
[ "$(status)" = 200 ] || fail "reading the imported table failed: $OUT"
[ "$(echo "$OUT" | json 'j["total"]')" = "$ROWS" ] || fail "the imported table has $(echo "$OUT" | json 'j["total"]') rows, not $ROWS"
[ "$(echo "$OUT" | json 'int(j["rows"][0][0])')" = $((ROWS - 1)) ] || fail "the imported table's last row is wrong: $OUT"
say "B has the same files, saved data and $ROWS rows"

say "one 8 MB file more: the export must be refused, not cut short"
head -c $((8 * 1024 * 1024)) /dev/urandom >"$TMP/a-apps/splunk-table/media/part-3.bin"
R=$(curl -sS -o "$TMP/over" -w '%{http_code}' "$A/api/apps/splunk-table/export?data=1")
[ "$R" = 400 ] && grep -q "100 MB" "$TMP/over" || fail "an app over the limit answered $R: $(head -c 300 "$TMP/over")"
say "refused: $(head -c 200 "$TMP/over")"

pass "exported $((SIZE / 1024 / 1024)) MB with data in $SECS s, imported it into an empty Wardian intact; over the limit is refused"
