#!/usr/bin/env bash
set -euo pipefail
TMP="${TMPDIR:-/tmp}/trusted-ci-attestation-$$"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP"
GOOD=docs/research/trusted-ci-fixtures/attestation-dry-run.json
python3 scripts/trusted-ci-attestation-check.py "$GOOD" >/dev/null
python3 - "$GOOD" "$TMP/bad-sha.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text()); d['commitSha']='bad'
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/trusted-ci-attestation-check.py "$TMP/bad-sha.json" >/dev/null 2>&1; then echo 'FAIL bad commit accepted'; exit 1; fi
python3 - "$GOOD" "$TMP/bad-expiry.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text()); d['expiresAt']=d['issuedAt']
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/trusted-ci-attestation-check.py "$TMP/bad-expiry.json" >/dev/null 2>&1; then echo 'FAIL bad expiry accepted'; exit 1; fi
echo 'trusted-ci-attestation-test=PASS'
