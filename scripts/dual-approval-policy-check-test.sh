#!/usr/bin/env bash
set -euo pipefail
TMP="${TMPDIR:-/tmp}/dual-approval-policy-$$"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP"
GOOD=docs/research/dual-approval-fixtures/root-rotation.json
python3 scripts/dual-approval-policy-check.py "$GOOD" >/dev/null
python3 - "$GOOD" "$TMP/bad.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text())
d["approvals"][1]["actor"] = d["approvals"][0]["actor"]
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/dual-approval-policy-check.py "$TMP/bad.json" >/dev/null 2>&1; then echo 'FAIL duplicate approver accepted'; exit 1; fi
python3 - "$GOOD" "$TMP/revoked.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text())
d["approvals"][1]["revoked"] = True
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/dual-approval-policy-check.py "$TMP/revoked.json" >/dev/null 2>&1; then echo 'FAIL revoked approval accepted'; exit 1; fi
for field in operationId target requestedBy; do
  python3 - "$GOOD" "$TMP/empty-$field.json" "$field" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text())
d[sys.argv[3]] = ""
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
  if python3 scripts/dual-approval-policy-check.py "$TMP/empty-$field.json" >/dev/null 2>&1; then
    echo "FAIL empty $field accepted"; exit 1
  fi
done
python3 - "$GOOD" "$TMP/empty-actor.json" <<'PY'
import json, pathlib, sys
d=json.loads(pathlib.Path(sys.argv[1]).read_text())
d["approvals"][0]["actor"] = ""
pathlib.Path(sys.argv[2]).write_text(json.dumps(d))
PY
if python3 scripts/dual-approval-policy-check.py "$TMP/empty-actor.json" >/dev/null 2>&1; then echo 'FAIL empty approval actor accepted'; exit 1; fi
echo 'dual-approval-policy-test=PASS'
