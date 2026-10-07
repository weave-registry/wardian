#!/usr/bin/env bash
# Live: a Google Drive folder listed and an app opened from it, against real Google Drive.
#
# Needs GDRIVE_SA_KEY, the path of a service account's JSON key, and LIVE_GDRIVE_FOLDER_ID (or
# GDRIVE_FOLDER_ID), the id of a folder shared with that service account that holds at least one
# app (one subfolder per app). LIVE_GDRIVE_APP names the app to open; by default the first one.
# This is how Wardian itself is set up for Drive (README, "Connect Google Drive"): the key is the
# one Settings would upload, the folder the one Settings would choose.
#
# What it proves: the key signs in, Settings' folder browser lists the shared folders, the folder's
# apps are listed, Wardian serves from the folder, and the chosen app's files come from Drive.
source "$(dirname "$0")/lib.sh"
FOLDER="${LIVE_GDRIVE_FOLDER_ID:-${GDRIVE_FOLDER_ID:-}}"
[ -n "${GDRIVE_SA_KEY:-}" ] || skip "GDRIVE_SA_KEY (the path of a service account key) is not set"
[ -f "$GDRIVE_SA_KEY" ] || skip "GDRIVE_SA_KEY does not name a file"
[ -n "$FOLDER" ] || skip "LIVE_GDRIVE_FOLDER_ID (or GDRIVE_FOLDER_ID) is not set"

mkdir -p "$TMP/apps"
start_wardian "$TMP/apps" GDRIVE_SA_KEY="$GDRIVE_SA_KEY"

OUT=$(api GET /api/status)
EMAIL=$(echo "$OUT" | json 'j.get("client_email") or ""')
[ -n "$EMAIL" ] || fail "Wardian did not load the key: $OUT"
say "signed in as $EMAIL"

OUT=$(api GET /api/drive/browse)
[ "$(status)" = 200 ] || fail "browsing Drive failed: $OUT"
say "folders shared with the account: $(echo "$OUT" | json 'len(j["folders"])')"

OUT=$(api GET "/api/drive/preview?folder=$FOLDER")
[ "$(status)" = 200 ] || fail "listing the folder failed: $OUT"
N=$(echo "$OUT" | json 'len(j["apps"])')
[ "$N" -ge 1 ] || fail "the folder holds no apps: $OUT"
say "the folder holds $N apps: $(echo "$OUT" | json '[a.get("name", a) if isinstance(a, dict) else a for a in j["apps"]]')"

OUT=$(api POST /api/source "{\"kind\":\"drive\",\"folder_id\":\"$FOLDER\",\"folder_name\":\"live check\"}")
[ "$(status)" = 200 ] || fail "serving from the folder failed: $OUT"
[ "$(echo "$OUT" | json 'j["source"]')" = drive ] || fail "Wardian is not serving from Drive: $OUT"

api GET /api/app-list >"$TMP/apps.json"
[ "$(status)" = 200 ] || fail "the app list failed"
APP=$(LIVE_GDRIVE_APP="${LIVE_GDRIVE_APP:-}" python3 -c '
import json, os, sys
apps = json.load(open(sys.argv[1]))
want = os.environ["LIVE_GDRIVE_APP"]
ok = [a for a in apps if not a.get("error") and (not want or a["name"] == want)]
print(json.dumps(ok[0]) if ok else "")' "$TMP/apps.json")
[ -n "$APP" ] || fail "no app to open${LIVE_GDRIVE_APP:+ named $LIVE_GDRIVE_APP}: $(cat "$TMP/apps.json")"
NAME=$(echo "$APP" | json 'j["name"]')
KIND=$(echo "$APP" | json '"suite" if j.get("suite") else "page" if j.get("page") else "module"')
say "opening $NAME ($KIND)"

get() { curl -sS -o "$TMP/file" -w '%{http_code} %{content_type} %{size_download}' "$BASE$1"; }
case "$KIND" in
  suite)
    R=$(get "/run/$NAME/"); [ "${R%% *}" = 200 ] || fail "/run/$NAME/ answered $R"
    api GET "/apps/$NAME/suite.json" >"$TMP/suite.json"
    [ "$(status)" = 200 ] || fail "suite.json did not come from Drive"
    for a in $(python3 -c 'import json,sys; print(" ".join(x["name"] for x in json.load(open(sys.argv[1]))["apps"]))' "$TMP/suite.json"); do
      R=$(get "/frame/$NAME/$a"); [ "${R%% *}" = 200 ] || fail "the frame of $NAME/$a answered $R: $(head -c 300 "$TMP/file")"
      say "frame $a: $R"
    done
    ;;
  page)
    PAGE=$(echo "$APP" | json 'j["page"]')
    R=$(get "/apps/$NAME/$PAGE"); [ "${R%% *}" = 200 ] || fail "/apps/$NAME/$PAGE answered $R"
    grep -qi '<html\|<!doctype\|<body\|<script' "$TMP/file" || fail "$PAGE does not look like a page"
    say "page $PAGE: $R"
    ;;
  module)
    R=$(get "/apps/$NAME/app.wasm"); [ "${R%% *}" = 200 ] || fail "/apps/$NAME/app.wasm answered $R"
    [ "$(head -c 4 "$TMP/file" | od -An -tx1 | tr -d ' \n')" = 0061736d ] || fail "app.wasm is not WebAssembly"
    say "app.wasm: $R"
    ;;
esac
pass "listed the Drive folder ($N apps) and opened $NAME from it"
