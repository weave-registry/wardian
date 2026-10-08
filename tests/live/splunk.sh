#!/usr/bin/env bash
# Live: a Splunk search of at least 100,000 rows loaded into an app's table and paged
# (ADR-2610071219), against a real Splunk.
#
# Needs SPLUNK_URL (the management address, usually https://<host>:8089) and either SPLUNK_TOKEN or
# SPLUNK_USERNAME and SPLUNK_PASSWORD; SPLUNK_INSECURE_TLS=1 or SPLUNK_CA_FILE for a self-signed
# certificate. These are the variables Wardian itself reads (README, "Splunk"); they are handed to
# the Wardian under test and nothing else.
#
# LIVE_SPLUNK_SEARCH replaces the search. The default makes 125,000 numbered rows with
# makeresults, so it needs no index and no data:
#   | makeresults count=500 | streamstats count AS a | eval b=mvrange(0,250) | mvexpand b
#   | eval n=(a-1)*250+b, host="web-".(n%20), ms=n%997 | table n host ms
# LIVE_SPLUNK_MIN_ROWS (default 100000) is how many rows the search must give.
source "$(dirname "$0")/lib.sh"
[ -n "${SPLUNK_URL:-}" ] || skip "SPLUNK_URL is not set"
[ -n "${SPLUNK_TOKEN:-}" ] || { [ -n "${SPLUNK_USERNAME:-}" ] && [ -n "${SPLUNK_PASSWORD:-}" ]; } \
  || skip "neither SPLUNK_TOKEN nor SPLUNK_USERNAME and SPLUNK_PASSWORD is set"

DEFAULT_SEARCH='| makeresults count=500 | streamstats count AS a | eval b=mvrange(0,250) | mvexpand b | eval n=(a-1)*250+b, host="web-".(n%20), ms=n%997 | table n host ms'
SEARCH="${LIVE_SPLUNK_SEARCH:-$DEFAULT_SEARCH}"
MIN="${LIVE_SPLUNK_MIN_ROWS:-100000}"

mkdir -p "$TMP/apps"
cp -R "$ROOT/apps/splunk-table" "$TMP/apps/"   # declares splunk and db
PASS_ENV=()
for v in SPLUNK_URL SPLUNK_TOKEN SPLUNK_USERNAME SPLUNK_PASSWORD SPLUNK_INSECURE_TLS SPLUNK_CA_FILE; do
  [ -n "${!v:-}" ] && PASS_ENV+=("$v=${!v}")
done
start_wardian "$TMP/apps" "${PASS_ENV[@]}"

OUT=$(api GET /api/status)
[ "$(echo "$OUT" | json 'j["splunk"].get("ready", False)')" = True ] || fail "Wardian does not see a usable Splunk: $(echo "$OUT" | json 'j["splunk"]')"

allow splunk-table splunk
say "loading the search into the Splunk table app's database (this can take a few minutes)"
t0=$(date +%s)
BODY=$(SEARCH="$SEARCH" python3 -c 'import json,os; print(json.dumps({"package":"splunk-table","app":"search","table":"live","search":os.environ["SEARCH"],"earliest":"","latest":""}))')
OUT=$(api POST /api/db/search-into "$BODY")
[ "$(status)" = 200 ] || fail "search-into failed: $OUT"
TOTAL=$(echo "$OUT" | json 'j["total"]')
say "loaded $TOTAL rows in $(( $(date +%s) - t0 )) s; truncated: $(echo "$OUT" | json 'j.get("truncated")'); columns: $(echo "$OUT" | json 'j.get("columns")')"
[ "$TOTAL" -ge "$MIN" ] || fail "only $TOTAL rows, fewer than $MIN"

page() { api POST /api/db/page "{\"package\":\"splunk-table\",\"app\":\"search\",\"table\":\"live\",$1}"; }
OUT=$(page '"offset":0,"limit":100')
[ "$(status)" = 200 ] || fail "the first page failed: $OUT"
[ "$(echo "$OUT" | json 'len(j["rows"])')" = 100 ] || fail "the first page does not hold 100 rows"
[ "$(echo "$OUT" | json 'j["total"]')" = "$TOTAL" ] || fail "the page's total is not the loaded total"
LAST=$((TOTAL - 37))
OUT=$(page "\"offset\":$LAST,\"limit\":100")
[ "$(echo "$OUT" | json 'len(j["rows"])')" = 37 ] || fail "the last page should hold 37 rows: $OUT"

if [ "$SEARCH" = "$DEFAULT_SEARCH" ]; then
  # n runs 0..124999, one row each. Pages across the 50,000-row chunks Wardian reads in must line
  # up exactly: nothing lost, nothing read twice.
  for off in 0 49950 99950 124900; do
    OUT=$(page "\"offset\":$off,\"limit\":100,\"orderBy\":\"n\"")
    FIRST=$(echo "$OUT" | json 'int(float(j["rows"][0][j["columns"].index("n")]))')
    [ "$FIRST" = "$off" ] || fail "sorted by n, the page at $off starts at n=$FIRST"
  done
  OUT=$(page '"offset":0,"limit":5,"where":"host = ? AND ms > ?","params":["web-7",990]')
  [ "$(echo "$OUT" | json 'j["total"]')" -gt 0 ] || fail "a filtered page found nothing: $OUT"
  say "sorted pages line up across the 50,000-row chunks; a filtered page found $(echo "$OUT" | json 'j["total"]') rows"
fi
pass "$TOTAL rows loaded from Splunk and paged"
