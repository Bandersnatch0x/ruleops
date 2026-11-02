#!/usr/bin/env bash
set -euo pipefail
OUT="${1:-}"
STRICT=0
if [ "${2:-}" = "--strict" ]; then STRICT=1; fi
[ -n "$OUT" ] || { echo "usage: $0 OUT_DIR [--strict]" >&2; exit 2; }
mkdir -p "$OUT"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" "$OUT" "$STRICT" <<'PY'
import json, pathlib, sys, shutil, subprocess
root, out, strict = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), int(sys.argv[3])
checks = []
def check(name, ok, detail): checks.append({"name":name,"status":"READY" if ok else "BLOCKED","detail":detail})
plan = root / "docs/research/production-authority-rollout-plan-20261027.md"
check("rollout_plan", plan.is_file(), str(plan.relative_to(root)) if plan.is_file() else "missing")
for path in ["docs/adr/0179-execute-complete-attempt-accounting.md", "docs/adr/0180-root-threshold-hsm-implementation-plan.md", "docs/adr/0181-rdfc-admission-implementation-plan.md"]:
    p=root/path; check(path, p.is_file(), "present" if p.is_file() else "missing")
for cmd in ["java", "javac", "cargo", "mvn", "protoc"]:
    found=shutil.which(cmd); check(cmd, bool(found), found or "not-found")
check("github_workflow", (root/".github/workflows").is_dir(), ".github/workflows present" if (root/".github/workflows").is_dir() else "no workflow directory")
check("production_identity", False, "no workload identity/attestation evidence configured")
check("root_hsm_custody", False, "no HSM/offline custody evidence configured")
check("dual_approval_recovery", False, "no dual approval/recovery drill evidence")
check("registry_admission", False, "registry scripts are local acceptance seams only")
summary="READY" if all(c["status"]=="READY" for c in checks) else "BLOCKED"
report={"schema":"production-authority-preflight-v1","summary":summary,"diagnosticOnly":True,"gate":"NOT_RUN","trust":"NOT_EVALUATED","checks":checks}
(pathlib.Path(out)/"production-authority-preflight.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
print(f"production-authority-preflight={summary} evidence={pathlib.Path(out)/'production-authority-preflight.json'}")
if strict and summary != "READY": raise SystemExit(3)
PY
