#!/usr/bin/env bash
set -euo pipefail
TMP_DIR="${TMPDIR:-/tmp}/roundtable-replay-test-$$"
trap 'rm -rf "$TMP_DIR"' EXIT
mkdir -p "$TMP_DIR"
cp docs/research/roundtable-fixtures/execute-complete-accounting.json "$TMP_DIR/good.json"
./scripts/roundtable-replay-check.sh "$TMP_DIR/good.json" >/dev/null
python3 - "$TMP_DIR/good.json" "$TMP_DIR/bad.json" <<'PY'
import json, pathlib, sys
data = json.loads(pathlib.Path(sys.argv[1]).read_text())
data["recommendation"] = "B"
pathlib.Path(sys.argv[2]).write_text(json.dumps(data))
PY
if ./scripts/roundtable-replay-check.sh "$TMP_DIR/bad.json" >/dev/null 2>&1; then
  echo "FAIL negative fixture accepted"
  exit 1
fi
echo "roundtable-replay-test=PASS"
