#!/usr/bin/env bash
set -euo pipefail

FIXTURE="${1:-docs/research/roundtable-fixtures/execute-complete-accounting.json}"
python3 - "$FIXTURE" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
try:
    data = json.loads(p.read_text())
except Exception as exc:
    raise SystemExit(f"FAIL invalid-json: {exc}")
required = ["schema", "decisionId", "topic", "options", "recommendation", "blockingConditions", "evidence", "boundary", "reviewStatus", "seats", "seatInputs"]
missing = [k for k in required if k not in data]
if missing:
    raise SystemExit(f"FAIL missing-fields: {','.join(missing)}")
if data["schema"] != "roundtable-replay-v1":
    raise SystemExit("FAIL schema")
if data["reviewStatus"] not in {"PROPOSED", "REVIEWED"}:
    raise SystemExit("FAIL review-status")
expected_seats = {"runtime-security", "platform", "governance-audit"}
if set(data["seats"]) != expected_seats or set(data["seatInputs"]) != expected_seats:
    raise SystemExit("FAIL seats")
options = data["options"]
ids = {o.get("id") for o in options}
if len(ids) != len(options) or None in ids:
    raise SystemExit("FAIL option-ids")
rec = next((o for o in options if o.get("id") == data["recommendation"]), None)
if rec is None or rec.get("status") != "RECOMMENDED":
    raise SystemExit("FAIL recommendation")
if not isinstance(data["blockingConditions"], list) or not data["blockingConditions"]:
    raise SystemExit("FAIL blocking-conditions")
if not isinstance(data["evidence"], list) or not data["evidence"]:
    raise SystemExit("FAIL evidence")
for e in data["evidence"]:
    for k in ("source", "command", "commitSha", "evidenceLevel"):
        if not e.get(k):
            raise SystemExit(f"FAIL evidence-{k}")
    import re
    if not re.fullmatch(r"[^:]+:[0-9]+", e["source"]):
        raise SystemExit("FAIL evidence-source-location")
    if not re.fullmatch(r"[0-9a-fA-F]{7,40}", e["commitSha"]):
        raise SystemExit("FAIL evidence-commit-sha")
    if "grep" in e["command"] and e.get("evidenceLevel") == "BEHAVIOR_PASS":
        raise SystemExit("FAIL count-only-evidence")
if data["boundary"] not in {"LOCAL_UNATTESTED", "SHADOW", "RDFC_NOT_ADMITTED"}:
    raise SystemExit("FAIL authority-boundary")
print(f"roundtable-replay=PASS decision={data['decisionId']} recommendation={data['recommendation']} evidence={len(data['evidence'])}")
PY
