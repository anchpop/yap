#!/usr/bin/env bash
# Smoke test the yap-ai-backend server. Probes every externally-dependent
# endpoint end to end, catching runtime-only breakage that compiles and passes
# unit tests: missing crypto providers, glibc mismatches, missing env vars,
# expired API keys.
#
# Usage:
#   ./scripts/smoke_test_server.sh                          # prod (fly.io)
#   ./scripts/smoke_test_server.sh http://localhost:8080    # local server
#
# Run automatically by .github/workflows/deploy.yml after each fly deploy.
set -euo pipefail

BASE="${1:-https://yap-ai-backend.fly.dev}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl_smoke() {
  curl --silent --show-error --fail-with-body --max-time 120 \
    --retry 3 --retry-delay 5 --retry-all-errors "$@"
}

expect_audio() {
  local size
  size=$(wc -c < "$1")
  if [ "$size" -lt 5000 ]; then
    echo "$2 returned suspiciously small audio ($size bytes)"
    exit 1
  fi
  echo "$2 OK ($size bytes of base64 audio)"
}

echo "Smoke testing $BASE"

# JWT encode/decode roundtrip inside the running binary
curl_smoke --output /dev/null "$BASE/health"
echo "/health OK"

# rkyv language data serving (both halves of the split pack)
for part in core sentences; do
  curl_smoke --output /dev/null -X POST "$BASE/language-data" \
    -H 'content-type: application/json' \
    -d '{"course":{"nativeLanguage":"English","targetLanguage":"French"},"part":"'"$part"'","chunk_index":0,"chunk_size":1024}'
done
echo "/language-data OK"

# All four TTS providers end-to-end, asserting we got real audio back (auth
# is verified but not enforced on these endpoints, so a placeholder bearer
# token works)
for endpoint in tts tts/google tts/openai tts/gemini; do
  out="$TMP/smoke-${endpoint//\//-}.b64"
  curl_smoke --output "$out" -X POST "$BASE/$endpoint" \
    -H 'authorization: Bearer smoke-test' \
    -H 'content-type: application/json' \
    -d '{"text":"Bonjour, comment allez-vous ?","language":"French"}'
  expect_audio "$out" "/$endpoint"
done

# Pronunciation feedback: feed the Google TTS audio back as both the user and
# the reference recording — exercises the Modal phoneme service and the Gemini
# feedback model end to end.
python3 -c "import json, pathlib; a = pathlib.Path('$TMP/smoke-tts-google.b64').read_text().strip(); pathlib.Path('$TMP/smoke-pronunciation.json').write_text(json.dumps({'sentence': 'Bonjour, comment allez-vous ?', 'language': 'French', 'user_audio': a, 'reference_audio': a}))"
curl_smoke --output "$TMP/smoke-pronunciation-out.json" -X POST "$BASE/pronunciation-feedback" \
  -H 'authorization: Bearer smoke-test' \
  -H 'content-type: application/json' \
  --data @"$TMP/smoke-pronunciation.json"
test -s "$TMP/smoke-pronunciation-out.json"
echo "/pronunciation-feedback OK"

# Supabase-backed reads (validates SUPABASE_URL + service role key)
curl_smoke --output /dev/null "$BASE/profile?id=16f21575-e0f6-49c0-96d3-957438f167c2"
curl_smoke --output /dev/null "$BASE/user-language-stats?id=16f21575-e0f6-49c0-96d3-957438f167c2"
echo "supabase reads OK"

echo "All smoke tests passed"
