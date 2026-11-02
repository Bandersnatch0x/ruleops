#!/usr/bin/env bash
set -euo pipefail
TMP="${TMPDIR:-/tmp}/registry-admission-$$"; trap 'rm -rf "$TMP"' EXIT; mkdir -p "$TMP"
GOOD=docs/research/registry-fixtures/admission-dry-run.json
python3 scripts/registry-admission-dry-run-check.py "$GOOD" >/dev/null
python3 - "$GOOD" "$TMP/bad-digest.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text()); d['artifactDigest']='bad'
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/registry-admission-dry-run-check.py "$TMP/bad-digest.json" >/dev/null 2>&1; then echo 'FAIL bad digest accepted'; exit 1; fi
python3 - "$GOOD" "$TMP/bad-authority.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text()); d['authority']='PRODUCTION'
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/registry-admission-dry-run-check.py "$TMP/bad-authority.json" >/dev/null 2>&1; then echo 'FAIL production authority accepted'; exit 1; fi
echo 'registry-admission-dry-run-test=PASS'
