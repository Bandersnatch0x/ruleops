#!/usr/bin/env bash
set -euo pipefail
OUT="${TMPDIR:-/tmp}/production-authority-preflight-$$"
trap 'rm -rf "$OUT"' EXIT
./scripts/production-authority-preflight.sh "$OUT"
python3 - "$OUT/production-authority-preflight.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
assert report["schema"] == "production-authority-preflight-v1"
assert report["summary"] == "BLOCKED"
assert report["diagnosticOnly"] is True
assert report["gate"] == "NOT_RUN"
assert report["trust"] == "NOT_EVALUATED"
checks = {item["name"]: item for item in report["checks"]}
expected = {
    "rollout_plan", "docs/adr/0179-execute-complete-attempt-accounting.md",
    "docs/adr/0180-root-threshold-hsm-implementation-plan.md",
    "docs/adr/0181-rdfc-admission-implementation-plan.md", "java", "javac",
    "cargo", "mvn", "protoc", "github_workflow", "production_identity",
    "root_hsm_custody", "dual_approval_recovery", "registry_admission",
}
assert expected <= checks.keys(), sorted(expected - checks.keys())
for name in ["production_identity", "root_hsm_custody", "dual_approval_recovery", "registry_admission"]:
    assert checks[name]["status"] == "BLOCKED", (name, checks[name])
assert not any(item["status"] == "READY" for item in checks.values() if item["name"] in {
    "production_identity", "root_hsm_custody", "dual_approval_recovery", "registry_admission"
})
PY
set +e
./scripts/production-authority-preflight.sh "$OUT/strict" --strict >/dev/null 2>&1
rc=$?
set -e
test "$rc" -eq 3
echo "production-authority-preflight-test=PASS"
