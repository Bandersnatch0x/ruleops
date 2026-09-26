#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: cli-surface-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

[ -x ./rf ] || { echo "rf dispatcher is not executable" >&2; exit 1; }
./rf --help > "$out_dir/help.txt"
for command in \
  'rf init' \
  'rf doctor' \
  'rf config validate' \
  'rf config show' \
  'rf standard validate' \
  'rf rule validate' \
  'rf rule generate' \
  'rf rule test' \
  'rf coverage analyze' \
  'rf pilot report' \
  'rf pilot delivery-check' \
  'rf pilot baseline' \
  'rf pilot observe' \
  'rf pilot feedback' \
  'rf pilot governance' \
  'rf pilot window-report' \
  'rf pilot evidence package' \
  'rf pilot evidence adjudicate' \
  'rf decision evaluate' \
  'rf center check' \
  'rf explain' \
  'rf feedback draft' \
  'rf exception request' \
  'rf feedback submit' \
  'rf exception submit' \
  'rf status contract-check' \
  'rf status deployment-check' \
  'rf scan'; do
  rg -Fq "$command" "$out_dir/help.txt" || { echo "missing CLI help entry: $command" >&2; exit 1; }
done

./rf standard validate specs/standard/SEC-JAVA-017.yaml > "$out_dir/standard-validate.txt"
rg -q '^OK specs/standard/SEC-JAVA-017\.yaml$' "$out_dir/standard-validate.txt"
./rf rule validate specs/standard/SEC-JAVA-017.yaml specs/rule-intent/corp.java.no-system-exit.yaml > "$out_dir/rule-validate.txt"
rg -q '^OK specs/rule-intent/corp\.java\.no-system-exit\.yaml$' "$out_dir/rule-validate.txt"

compiled="$(./rf rule generate specs/rule-intent/corp.java.no-system-exit.yaml "$out_dir/candidate-build")"
candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate" ] || { echo "CLI did not return candidate path" >&2; exit 1; }
./rf rule test "$candidate" "$out_dir/candidate-evidence" > "$out_dir/rule-test.txt"
rg -q '^VERDICT PASS \(LOCAL_UNATTESTED\)$' "$out_dir/rule-test.txt"

# This fixture is regenerated on every run so the CLI's non-destructive init
# guard does not turn a repeated check into a false failure.
rm -rf "$out_dir/scan-repo"
mkdir -p "$out_dir/scan-repo/src/main/java"
printf '%s\n' 'public class CliScanSample { void run() { } }' > "$out_dir/scan-repo/src/main/java/CliScanSample.java"
printf '%s\n' '<project></project>' > "$out_dir/scan-repo/pom.xml"
./rf init --repo-root "$out_dir/scan-repo" --non-interactive --language java --build-system maven > "$out_dir/config-init.txt"
rg -q '^CONFIG_WRITTEN ' "$out_dir/config-init.txt"
./rf config validate --repo-root "$out_dir/scan-repo" > "$out_dir/config-validate.txt"
rg -q '^CONFIG_VALID ' "$out_dir/config-validate.txt"
cargo build --quiet -p erf-fake-host -p erf-fake-resolver
./rf scan \
  --repo-root "$out_dir/scan-repo" \
  --rule-id corp.java.cli-smoke \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" \
  > "$out_dir/scan-rule-id.txt"
rg -q '^rule_id=corp\.java\.cli-smoke$' "$out_dir/scan-rule-id.txt"

./rf scan \
  --repo-root "$out_dir/scan-repo" \
  --candidate "$candidate" \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" \
  > "$out_dir/scan-candidate-rule-id.txt"
rg -q '^rule_id=corp\.java\.no-system-exit$' "$out_dir/scan-candidate-rule-id.txt"

set +e
./rf scan \
  --repo-root "$out_dir/scan-repo" \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" \
  > "$out_dir/scan-missing-rule-id.txt" 2>&1
missing_rule_id_rc=$?
set -e
[ "$missing_rule_id_rc" -eq 2 ] || { echo "scan without --rule-id returned $missing_rule_id_rc, expected 2" >&2; exit 1; }
rg -q 'diagnostic code=CLI_INPUT_INVALID' "$out_dir/scan-missing-rule-id.txt"

set +e
FAKE_DELAY_MS=200 ./rf scan \
  --repo-root "$out_dir/scan-repo" \
  --rule-id corp.java.cli-timeout \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" \
  --deadline-ms 20 \
  > "$out_dir/scan-incomplete.txt" 2>&1
incomplete_rc=$?
set -e
[ "$incomplete_rc" -eq 3 ] || { echo "incomplete scan returned $incomplete_rc, expected 3" >&2; exit 1; }
rg -q '^gate=Inconclusive ' "$out_dir/scan-incomplete.txt"

cat > "$out_dir/finding.json" <<'EOF'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "cli-finding-1",
  "ruleId": "corp.java.cli-smoke",
  "fingerprint": "cli-fp-1",
  "message": "client explanation sample",
  "location": {"path": "src/main/java/CliScanSample.java", "line": 1, "column": 1}
}
EOF
./rf explain "$out_dir/finding.json" > "$out_dir/explain.txt"
rg -q '^location=src/main/java/CliScanSample\.java:1:1$' "$out_dir/explain.txt"
./rf feedback draft "$out_dir/finding.json" \
  --classification true-fp --reason "client draft" --actor cli-test \
  --output "$out_dir/feedback-draft.json"
rg -q '"status": "PROPOSED"' "$out_dir/feedback-draft.json"
rg -q '"kind": "FeedbackDraft"' "$out_dir/feedback-draft.json"
./rf exception request "$out_dir/finding.json" \
  --scope 'src/main/java/**' --reason "client request" --actor cli-test \
  --output "$out_dir/exception-request.json"
rg -q '"status": "REQUESTED"' "$out_dir/exception-request.json"
rg -q '"kind": "ExceptionRequest"' "$out_dir/exception-request.json"
./rf status contract-check "$out_dir/status-contract" > "$out_dir/status-contract.txt"
rg -q '^status-publication=PASS ' "$out_dir/status-contract.txt"
./rf status deployment-check "$out_dir/status-adapter-deployment" > "$out_dir/status-adapter-deployment.txt"
rg -q '^status-adapter-deployment=PASS ' "$out_dir/status-adapter-deployment.txt"

set +e
./rf scan \
  --repo-root "$out_dir/scan-repo" \
  --candidate "$candidate" \
  --rule-id corp.java.wrong \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" \
  > "$out_dir/scan-conflicting-rule-id.txt" 2>&1
conflicting_rule_id_rc=$?
set -e
[ "$conflicting_rule_id_rc" -eq 2 ] || { echo "scan with conflicting identity returned $conflicting_rule_id_rc, expected 2" >&2; exit 1; }
rg -q 'diagnostic code=CLI_INPUT_INVALID' "$out_dir/scan-conflicting-rule-id.txt"

set +e
./rf unknown > "$out_dir/unknown.txt" 2>&1
unknown_rc=$?
./rf release promote > "$out_dir/unsupported.txt" 2>&1
unsupported_rc=$?
set -e
[ "$unknown_rc" -eq 2 ] || { echo "unknown CLI area returned $unknown_rc, expected 2" >&2; exit 1; }
rg -q '^error: unknown command area: unknown$' "$out_dir/unknown.txt"
[ "$unsupported_rc" -eq 2 ] || { echo "unsupported release command returned $unsupported_rc, expected 2" >&2; exit 1; }
rg -q 'release command is not enabled' "$out_dir/unsupported.txt"

cat > "$out_dir/cli-surface.md" <<EOF
# rf Thin Client Surface Evidence

- Acceptance command: scripts/cli-surface-check.sh $out_dir
- Dispatcher: ./rf
- Authoring: standard/rule validate, rule generate, rule test
- Analysis/report: coverage analyze, pilot report, pilot window-report
- Evidence: pilot evidence package and pilot evidence adjudicate
- Observation: pilot baseline, observe, feedback, governance
- Runtime: scan delegates to rf-cli
- Thin client: explain plus digest-only feedback/exception draft/request and explicit Server submission
- Publication: StatusPublication contract evidence prepared for an external adapter; no SCM write
- Deployment: Status Adapter HTTPS/opaque-secret/minimal-scope seam; no connector or network call
- Unsupported authority surfaces: release/gate and approval remain outside the client

| Check | Evidence | Result |
| --- | --- | --- |
| Help exposes the supported command surface | help.txt | PASS |
| Init and config validation freeze project identity | config-init.txt, config-validate.txt | PASS |
| Standard and Rule validation delegate successfully | standard-validate.txt, rule-validate.txt | PASS |
| Rule generation and fixture verification delegate successfully | rule-test.txt | PASS |
| Scan reports explicit no-Candidate rule identity | scan-rule-id.txt | PASS |
| Scan derives Candidate rule identity | scan-candidate-rule-id.txt | PASS |
| Scan requires explicit identity without a Candidate | scan-missing-rule-id.txt | PASS |
| Incomplete scan returns exit code 3 | scan-incomplete.txt | PASS |
| Conflicting identity inputs fail closed | scan-conflicting-rule-id.txt | PASS |
| Explain exposes digest-only navigable location | explain.txt | PASS |
| Feedback draft remains PROPOSED and non-submitting | feedback-draft.json | PASS |
| Exception request remains REQUESTED and non-granting | exception-request.json | PASS |
| Status publication is adapter-ready and credential-free | status-contract.txt | PASS |
| Status adapter deployment seam is HTTPS/scoped and credential-free | status-adapter-deployment.txt | PASS |
| Unknown areas are rejected | unknown.txt | PASS |
| Authority-changing unsupported commands fail closed | unsupported.txt | PASS |

The dispatcher is a thin local entrypoint and does not grant release, gate,
SCM, exception approval or Gate authority. Feedback and exception submission
commands only send closed digest-only requests to the selected Server; a
received receipt is not approval, Gate mutation, or SCM publication.
Status publication evidence is likewise adapter-ready only: the local runtime
does not hold SCM credentials or write branches, checks, or protected status.
Status adapter deployment evidence freezes an external connector seam only; it
does not resolve credentials, call a provider, or retry an unknown transport.
EOF

echo "cli-surface=PASS evidence=$out_dir/cli-surface.md"
