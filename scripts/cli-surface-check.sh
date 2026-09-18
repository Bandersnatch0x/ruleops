#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: cli-surface-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

[ -x ./rf ] || { echo "rf dispatcher is not executable" >&2; exit 1; }
./rf --help > "$out_dir/help.txt"
for command in \
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
  'rf center check' \
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

mkdir -p "$out_dir/scan-repo/src/main/java"
printf '%s\n' 'public class CliScanSample { void run() { } }' > "$out_dir/scan-repo/src/main/java/CliScanSample.java"
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
rg -q 'missing --rule-id when --candidate is absent' "$out_dir/scan-missing-rule-id.txt"

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
rg -q 'cannot be combined with --candidate' "$out_dir/scan-conflicting-rule-id.txt"

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
# V0.1 rf CLI Surface Evidence

- Acceptance command: scripts/cli-surface-check.sh $out_dir
- Dispatcher: ./rf
- Authoring: standard/rule validate, rule generate, rule test
- Analysis/report: coverage analyze, pilot report, pilot window-report
- Observation: pilot baseline, observe, feedback, governance
- Runtime: scan delegates to rf-cli
- Unsupported authority surfaces: release/gate/feedback/exception fail with exit 2

| Check | Evidence | Result |
| --- | --- | --- |
| Help exposes the supported command surface | help.txt | PASS |
| Standard and Rule validation delegate successfully | standard-validate.txt, rule-validate.txt | PASS |
| Rule generation and fixture verification delegate successfully | rule-test.txt | PASS |
| Scan reports explicit no-Candidate rule identity | scan-rule-id.txt | PASS |
| Scan derives Candidate rule identity | scan-candidate-rule-id.txt | PASS |
| Scan requires explicit identity without a Candidate | scan-missing-rule-id.txt | PASS |
| Incomplete scan returns exit code 3 | scan-incomplete.txt | PASS |
| Conflicting identity inputs fail closed | scan-conflicting-rule-id.txt | PASS |
| Unknown areas are rejected | unknown.txt | PASS |
| Authority-changing unsupported commands fail closed | unsupported.txt | PASS |

The dispatcher is a thin local entrypoint and does not grant release, gate,
SCM, feedback or exception authority.
EOF

echo "cli-surface=PASS evidence=$out_dir/cli-surface.md"
