#!/usr/bin/env bash
# Live: an export of about 400 MB (ADR-2610071248 allows up to 500 MB in one .wardian file).
#
# Needs no credentials, but about 1.5 GB of free disk and memory: the app, its database, the file,
# and Wardian building the file in memory. Set LIVE_EXPORT=0 to skip it; LIVE_EXPORT_MB (default
# 400) sets the size.
#
# What it proves: an app holding ~400 MB of incompressible files and table rows, with its saved
# data, downloads as one valid .wardian file with every byte intact, in reasonable time. It also
# records what happens when that file is offered back to Wardian's import, which accepts at most
# 100 MB.
source "$(dirname "$0")/lib.sh"
[ "${LIVE_EXPORT:-1}" != 0 ] || skip "LIVE_EXPORT=0"
MB="${LIVE_EXPORT_MB:-400}"
FREE_MB=$(df -Pm "$TMP" | awk 'NR==2 {print $4}')
[ "$FREE_MB" -ge $((MB * 3 + 200)) ] || skip "needs about $((MB * 3 + 200)) MB free in $TMP, has $FREE_MB MB"

# A package may hold 64 MB per file and 256 MB in all (`wardian check`), so the size comes from two
# places, as it would for a real app: up to 240 MB of media files, and the rest as rows in the app's
# own database (the `db` capability), which goes in the file as .wardian/data/tables.sqlite.
FILES_MB=$(( MB < 240 ? MB : 240 ))
TABLE_MB=$(( MB - FILES_MB ))
mkdir -p "$TMP/apps"
cp -R "$ROOT/apps/splunk-table" "$TMP/apps/"
mkdir -p "$TMP/apps/splunk-table/media"
say "writing $FILES_MB MB of random bytes into the app's files"
# Random, so deflate cannot shrink them. Four files of at most 60 MB each.
for i in 1 2 3 4; do
  head -c $((FILES_MB * 1024 * 1024 / 4)) /dev/urandom >"$TMP/apps/splunk-table/media/part-$i.bin"
done
start_wardian "$TMP/apps"

api POST /api/state/apps/splunk-table '{"app":"table","key":"state","value":{"s":"index=web | stats count","name":"live export"}}' >/dev/null
[ "$(status)" = 200 ] || fail "saving app data failed: $(cat "$TMP/body")"
say "loading about $TABLE_MB MB of rows into the app's database, 1,000 rows a call"
python3 - "$BASE" "$TABLE_MB" <<'PY' || fail "loading the table failed"
import base64, json, os, sys, urllib.request
base, mb = sys.argv[1], int(sys.argv[2])
row_bytes = 1800                      # 1,000 rows of this fit in one 2 MB request
# Base64 text deflates to about 3/4 of its size, so write 4/3 as much to land near `mb` in the file.
calls = (mb * 1024 * 1024 * 4 // 3) // (row_bytes * 1000) + 1 if mb > 0 else 0
n = 0
for c in range(calls):
    rows = []
    for _ in range(1000):
        rows.append([n, "web-%d" % (n % 20), base64.b64encode(os.urandom(row_bytes * 3 // 4)).decode()])
        n += 1
    body = json.dumps({"package": "splunk-table", "app": "table", "table": "big", "columns": ["n", "host", "blob"], "rows": rows, "create": True}).encode()
    req = urllib.request.Request(base + "/api/db/insert", body, {"Content-Type": "application/json"})
    out = json.load(urllib.request.urlopen(req, timeout=120))
    assert out.get("inserted") == 1000, out
print("  %d rows in %d calls" % (n, calls))
PY

OUT=$(api GET "/api/apps/splunk-table/export?data=1&preview=1")
[ "$(status)" = 200 ] || fail "the preview failed: $OUT"
say "preview: $(echo "$OUT" | json 'len(j["files"])') files, $(echo "$OUT" | json 'j["bytes"] // (1024*1024)') MB of app files; data: $(echo "$OUT" | json 'j["data"]')"

say "downloading splunk-table.wardian with its data"
t0=$(date +%s)
R=$(curl -sS -o "$TMP/splunk-table.wardian" -D "$TMP/headers" -w '%{http_code} %{size_download}' "$BASE/api/apps/splunk-table/export?data=1")
SECS=$(( $(date +%s) - t0 ))
CODE=${R%% *}; SIZE=${R##* }
[ "$CODE" = 200 ] || fail "the export answered $CODE: $(head -c 600 "$TMP/splunk-table.wardian")"
grep -qi '^content-type: application/vnd.wardian+zip' "$TMP/headers" || fail "not served as application/vnd.wardian+zip"
say "$((SIZE / 1024 / 1024)) MB in $SECS s"
[ "$SIZE" -ge $((MB * 1024 * 1024 * 95 / 100)) ] || fail "the file is $((SIZE / 1024 / 1024)) MB, well short of $MB MB"

say "checking every entry of the file"
python3 - "$TMP/splunk-table.wardian" "$TMP/apps/splunk-table" <<'PY' || fail "the file does not hold the app intact"
import hashlib, json, sys, zipfile
path, src = sys.argv[1], sys.argv[2]
z = zipfile.ZipFile(path)
bad = z.testzip()
assert bad is None, f"bad CRC in {bad}"
names = z.namelist()
assert all(n.startswith("splunk-table/") for n in names), "an entry outside the package folder"
for need in ["export.json", "data/storage.json", "data/tables.sqlite"]:
    assert "splunk-table/.wardian/" + need in names, f"missing .wardian/{need}"
for i in range(1, 5):
    rel = f"media/part-{i}.bin"
    h1 = hashlib.sha256(z.read("splunk-table/" + rel)).hexdigest()
    h2 = hashlib.sha256(open(f"{src}/{rel}", "rb").read()).hexdigest()
    assert h1 == h2, f"{rel} differs"
tables = z.getinfo("splunk-table/.wardian/data/tables.sqlite").file_size
print(f"  {len(names)} entries, CRCs good, media identical, tables.sqlite {tables // (1024 * 1024)} MB")
PY

# Import accepts at most 100 MB (MAX_ZIP_BYTES), so a file this size cannot go back in. Recorded,
# not failed: ADR-2610072033 asks for the export only.
R=$(curl -sS -o "$TMP/import" -w '%{http_code}' -X POST -H 'Content-Type: application/zip' --data-binary "@$TMP/splunk-table.wardian" "$BASE/api/import/preview" || true)
say "NOTE: offering the same file to import answers $R: $(head -c 200 "$TMP/import" 2>/dev/null)"

grep -q "export: splunk-table with its data" "$TMP/server.log" || fail "the server did not log the export"
pass "exported $((SIZE / 1024 / 1024)) MB with data in $SECS s; every entry intact"
