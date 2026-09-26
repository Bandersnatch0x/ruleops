#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/scan-output-check}"
CONTRACT="$REPO_ROOT/contracts/http/scan-output-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"
for needle in \
  'schemaVersion: scan-output-v1' \
  'gateDecision: [PASS, FAIL, INCONCLUSIVE]' \
  'authority: [LOCAL_UNATTESTED, TRUSTED]' \
  'machineProgress: JSONL on stderr' \
  'sourceUploadDefault: false' \
  'fullFindingUploadDefault: false' \
  'inputOrContractError: 2' \
  'incompleteOrRuntimeFailure: 3' \
  'trustFailure: 4'; do
  rg -F -q -- "$needle" "$CONTRACT"
done
REPO="$OUT_DIR/repo"
# Recreate the generated repository so repeated gate runs do not inherit the
# prior bootstrap configuration or the symlink-output fixture.
rm -rf "$REPO"
mkdir -p "$REPO/src/main/java"
printf '%s\n' 'public class A { void run() { } }' > "$REPO/src/main/java/A.java"
printf '%s\n' '<project></project>' > "$REPO/pom.xml"
cargo run --quiet -p erf-project-config -- init --repo-root "$REPO" --non-interactive > "$OUT_DIR/config-init.txt"

run_scan() {
  cargo run --quiet -p rf-cli -- scan \
    --repo-root "$REPO" \
    --project-config "$REPO/.ruleops/config.yaml" \
    --rule-id corp.java.scan-output \
    --resolver "$REPO_ROOT/target/debug/erf-fake-resolver" \
    --host "$REPO_ROOT/target/debug/erf-fake-host" \
    "$@"
}

run_scan --format json --progress json > "$OUT_DIR/scan.json" 2> "$OUT_DIR/progress.jsonl"
run_scan --format sarif --progress none > "$OUT_DIR/scan.sarif"

ln -s "$OUT_DIR/escaped-output.json" "$REPO/.ruleops/scan-output.json"
set +e
run_scan --format json --progress none --output "$REPO/.ruleops/scan-output.json" > "$OUT_DIR/output-symlink.txt" 2>&1
output_symlink_rc=$?
set -e
[ "$output_symlink_rc" -eq 2 ]
rg -q '"code": "OUTPUT_PATH_INVALID"' "$OUT_DIR/output-symlink.txt"

python3 - "$OUT_DIR/scan.json" "$OUT_DIR/scan.sarif" "$OUT_DIR/progress.jsonl" <<'PY'
import json
import sys

scan = json.load(open(sys.argv[1], encoding="utf-8"))
sarif = json.load(open(sys.argv[2], encoding="utf-8"))
progress = [json.loads(line) for line in open(sys.argv[3], encoding="utf-8") if line.strip()]

assert scan["kind"] == "ScanResult"
assert scan["status"] == "FAIL"
assert scan["gateDecision"] == "FAIL"
assert scan["authority"] == "LOCAL_UNATTESTED"
assert scan["resultDigest"].startswith("sha256:")
assert scan["effectiveConfigDigest"].startswith("sha256:")
assert scan["workspaceDescriptorDigest"].startswith("sha256:")
assert scan["resolvedWorkspaceDescriptorDigest"].startswith("sha256:")
assert scan["configScanPlanDigest"].startswith("sha256:")
assert scan["findingViews"][0]["location"] == {"path": "src/main/java/A.java", "line": 1, "column": 5}
assert scan["findingRecords"][0]["findingId"] == scan["findingViews"][0]["findingId"]
assert {event["kind"] for event in progress} == {"ScanProgress"}
assert [event["event"]["stage"] for event in progress] == [
    "resolve", "affected-scope", "compile", "backend", "cache", "gate"
]
assert sarif["version"] == "2.1.0"
assert sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"] == {
    "startLine": 1, "startColumn": 5
}
assert "sourceContent" not in json.dumps(scan)
PY

set +e
FAKE_CRASH_ON_EXECUTE=1 run_scan --format json --progress none > "$OUT_DIR/incomplete.json"
incomplete_rc=$?
set -e
[ "$incomplete_rc" -eq 3 ]
python3 - "$OUT_DIR/incomplete.json" <<'PY'
import json
import sys

scan = json.load(open(sys.argv[1], encoding="utf-8"))
codes = {diagnostic["code"] for diagnostic in scan["diagnostics"]}
assert scan["status"] == "INCONCLUSIVE"
assert scan["gateDecision"] == "INCONCLUSIVE"
assert "INCOMPLETE_EXECUTION" in codes
assert any(event["stage"] == "backend" and event["outcome"] == "FAILED" for event in scan["progress"])
PY

set +e
run_scan --host "$REPO_ROOT/target/debug/erf-missing-host" --format json --progress json > "$OUT_DIR/failure.json" 2> "$OUT_DIR/failure-progress.jsonl"
failure_rc=$?
set -e
[ "$failure_rc" -eq 3 ]
python3 - "$OUT_DIR/failure.json" "$OUT_DIR/failure-progress.jsonl" <<'PY'
import json
import sys

failure = json.load(open(sys.argv[1], encoding="utf-8"))
progress = [json.loads(line) for line in open(sys.argv[2], encoding="utf-8") if line.strip()]
assert failure["kind"] == "ScanFailure"
assert failure["diagnostics"][0]["code"] == "HOST_UNAVAILABLE"
assert failure["diagnostics"][0]["stage"] == "backend"
assert failure["diagnostics"][0]["retry"] == "SAFE_RETRY"
assert failure["diagnostics"][0]["nextStep"]
assert progress[0]["kind"] == "ScanProgress"
assert progress[0]["event"]["stage"] == "backend"
PY

set +e
run_scan --host "$REPO_ROOT/target/debug/erf-missing-host" --format sarif --progress none > "$OUT_DIR/failure.sarif"
failure_sarif_rc=$?
set -e
[ "$failure_sarif_rc" -eq 3 ]
python3 - "$OUT_DIR/failure.sarif" <<'PY'
import json
import sys

failure = json.load(open(sys.argv[1], encoding="utf-8"))
assert failure["version"] == "2.1.0"
assert failure["runs"][0]["results"] == []
assert failure["runs"][0]["invocations"][0]["executionSuccessful"] is False
assert failure["runs"][0]["invocations"][0]["properties"]["erfDiagnosticCodes"] == ["HOST_UNAVAILABLE"]
PY

cat > "$OUT_DIR/scan-output.md" <<EOF
# Scan Output Evidence

- JSON result: \`ScanResult\` with \`PASS\`/\`FAIL\`/\`INCONCLUSIVE\` Gate state and explicit \`LOCAL_UNATTESTED\` authority
- Contract: contracts/http/scan-output-v1.yaml
- Finding identity: stable fingerprint plus digest-only FindingView with repository-relative path, line and column
- SARIF: version 2.1.0 with deterministic Finding results and no source upload
- Progress: six machine-readable resolve/affected-scope/compile/backend/cache/gate events
- Diagnostics: stable code, stage, retry class, next step and detail digest
- Incomplete execution: exit 3 and structured \`INCONCLUSIVE\` result
- Host startup failure: exit 3 and structured \`ScanFailure/HOST_UNAVAILABLE\`
EOF

echo "scan-output=PASS evidence=$OUT_DIR/scan-output.md"
