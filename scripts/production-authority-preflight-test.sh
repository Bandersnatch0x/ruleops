#!/usr/bin/env bash
set -euo pipefail
OUT="${TMPDIR:-/tmp}/production-authority-preflight-$$"
trap 'rm -rf "$OUT"' EXIT
./scripts/production-authority-preflight.sh "$OUT"
test "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["summary"])' "$OUT/production-authority-preflight.json")" = BLOCKED
if ./scripts/production-authority-preflight.sh "$OUT/strict" --strict >/dev/null 2>&1; then
  echo "FAIL strict preflight unexpectedly passed"; exit 1
fi
echo "production-authority-preflight-test=PASS"
