#!/usr/bin/env bash
# Live: one real Amazon Bedrock call through Wardian (ADR-2610071106), signed the way MODE says.
#
#   tests/live/bedrock.sh api-key   # needs AWS_BEARER_TOKEN_BEDROCK
#   tests/live/bedrock.sh sigv4     # needs AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY
#                                   #   (and AWS_SESSION_TOKEN for temporary keys)
# Both use AWS_REGION (or AWS_DEFAULT_REGION, else us-east-1), and WARDIAN_BEDROCK_MODEL /
# WARDIAN_BEDROCK_QUICK_MODEL when set. The model must be enabled for the account in that region.
#
# What it proves: Settings -> Claude -> Amazon Bedrock accepts the sign-in (Wardian makes a
# one-token call before saving), an app's claude:sample request is answered by Bedrock, and the
# secret never comes back in /api/status.
MODE="${1:-api-key}"
source "$(dirname "$0")/lib.sh"
CHECK="bedrock-$MODE"
REGION="${AWS_REGION:-${AWS_DEFAULT_REGION:-us-east-1}}"

case "$MODE" in
  api-key)
    [ -n "${AWS_BEARER_TOKEN_BEDROCK:-}" ] || skip "AWS_BEARER_TOKEN_BEDROCK is not set"
    BODY=$(REGION="$REGION" python3 -c 'import json,os; print(json.dumps({"provider":"bedrock","auth":"api-key","region":os.environ["REGION"],"token":os.environ["AWS_BEARER_TOKEN_BEDROCK"]}))')
    SECRETS=("$AWS_BEARER_TOKEN_BEDROCK")
    ;;
  sigv4)
    [ -n "${AWS_ACCESS_KEY_ID:-}" ] && [ -n "${AWS_SECRET_ACCESS_KEY:-}" ] || skip "AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY are not both set"
    BODY=$(REGION="$REGION" python3 -c 'import json,os; e=os.environ; print(json.dumps({"provider":"bedrock","auth":"access-keys","region":e["REGION"],"access_key_id":e["AWS_ACCESS_KEY_ID"],"secret_access_key":e["AWS_SECRET_ACCESS_KEY"],"session_token":e.get("AWS_SESSION_TOKEN","")}))')
    SECRETS=("$AWS_SECRET_ACCESS_KEY")
    [ -n "${AWS_SESSION_TOKEN:-}" ] && SECRETS+=("$AWS_SESSION_TOKEN")
    ;;
  *) echo "usage: $0 api-key|sigv4" >&2; exit 2 ;;
esac

mkdir -p "$TMP/apps"
cp -R "$ROOT/apps/splunk-table" "$TMP/apps/"   # declares claude:sample
MODELS=()
[ -n "${WARDIAN_BEDROCK_MODEL:-}" ] && MODELS+=("WARDIAN_BEDROCK_MODEL=$WARDIAN_BEDROCK_MODEL")
[ -n "${WARDIAN_BEDROCK_QUICK_MODEL:-}" ] && MODELS+=("WARDIAN_BEDROCK_QUICK_MODEL=$WARDIAN_BEDROCK_QUICK_MODEL")
# Only for checking this script against tests/fixtures/fake-bedrock.py; never set for a real run.
[ -n "${LIVE_BEDROCK_BASE_URL:-}" ] && MODELS+=("WARDIAN_BEDROCK_BASE_URL=$LIVE_BEDROCK_BASE_URL")
# The sign-in goes through Settings, not the environment, so the check sees exactly one of them.
start_wardian "$TMP/apps" ${MODELS[@]+"${MODELS[@]}"}

say "saving the Bedrock settings ($MODE, $REGION): Wardian tests them with a real call first"
OUT=$(api POST /api/ai/provider "$BODY")
[ "$(status)" = 200 ] || fail "Settings refused the sign-in: $OUT"
[ "$(echo "$OUT" | json 'j["provider"]')" = bedrock ] || fail "the provider is not bedrock: $OUT"
WANT=$([ "$MODE" = sigv4 ] && echo access-keys || echo api-key)
[ "$(echo "$OUT" | json 'j["bedrock"]["settings"]["auth"]')" = "$WANT" ] || fail "saved with the wrong sign-in: $OUT"

say "asking Claude through claude:sample, as an app would"
allow splunk-table ai
OUT=$(api POST /api/ai/sample '{"package":"splunk-table","app":"ask","tier":"quick","prompt":"Reply with exactly the one word: pong"}')
[ "$(status)" = 200 ] || fail "claude:sample failed: $OUT"
TEXT=$(echo "$OUT" | json 'j.get("text","")')
MODEL=$(echo "$OUT" | json 'j.get("model","")')
echo "$TEXT" | grep -qi pong || fail "unexpected answer from $MODEL: $TEXT"

STATUS=$(api GET /api/status)
for s in "${SECRETS[@]}"; do
  case "$STATUS$(cat "$TMP/server.log")" in *"$s"*) fail "a secret came back in /api/status or the server log" ;; esac
done
pass "Bedrock answered \"$TEXT\" ($MODEL, $REGION, $MODE); no secret in /api/status or the log"
