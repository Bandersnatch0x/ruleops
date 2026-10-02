#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-evidence-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

contract="specs/pilot/pilot-success-contract.yaml"
sample="specs/pilot/live-java-repositories.yaml"
contract_digest="$(sha256sum "$contract" | awk '{print $1}')"
sample_digest="$(sha256sum "$sample" | awk '{print $1}')"
window_start=1790812800
window_end=1793491199
input="$out_dir/evidence.input"
package="$out_dir/evidence.package"
adjudication="$out_dir/adjudication.txt"

metrics=(
  author-active-time precision recall false-positive-regression
  complete-rate inconclusive-rate scan-latency resource-consumption
  new-violation-trend historical-debt-trend false-pass-count
)

{
  echo 'format=erf-pilot-evidence-input-v1'
  echo 'contract-id=pilot-success-contract-v0'
  echo 'contract-version=1.0.0'
  echo "contract-digest=$contract_digest"
  echo "window-start=$window_start"
  echo "window-end=$window_end"
  echo "sample-digest=$sample_digest"
  index=0
  for metric in "${metrics[@]}"; do
    artifact_digest="$(printf 'pilot-evidence-artifact-v1:%s:%s\n' "$metric" "$index" | sha256sum | awk '{print $1}')"
    printf 'metric=%s\tpilot-check:%s\t%s\t%s\tCOMPLETE\tPASS\t%s\n' \
      "$metric" "$metric" "$window_start" "$window_end" "$artifact_digest"
    index=$((index + 1))
  done
} > "$input"

cargo test -q -p erf-pilot-evidence --all-targets > "$out_dir/cargo-test.log" 2>&1
rg -q 'test result: ok\. 6 passed; 0 failed; 0 ignored' "$out_dir/cargo-test.log"
rg -q 'PilotEvidencePackage' runtime/pilot-evidence/src/lib.rs docs/adr/0157-pilot-evidence-package-and-adjudication-runtime-boundary.md
rg -q 'FAIL.*INCONCLUSIVE.*PASS' specs/pilot/pilot-evidence-package.yaml runtime/pilot-evidence/src/lib.rs
rg -q 'contract_digest|sample_digest|artifact_digest|PilotAdjudication' runtime/pilot-evidence/src/lib.rs
rg -q 'APPROVED_CONTRACT_ID|APPROVED_WINDOW_START_EPOCH_SECS' runtime/pilot-evidence/src/lib.rs
rg -q 'pilot evidence package' rf

./rf pilot evidence package "$input" "$package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/package.console"
rg -q 'aggregate=PASS' "$out_dir/package.console"
rg -q '^package-digest=[0-9a-f]{64}$' "$package"

./rf pilot evidence adjudicate "$package" "$adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/adjudication.console"
rg -q 'decision=GO' "$out_dir/adjudication.console"
rg -q '^decision=GO$' "$adjudication"

sed '/^metric=precision\t/d' "$input" > "$out_dir/missing.input"
./rf pilot evidence package "$out_dir/missing.input" "$out_dir/missing.package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/missing-package.console"
rg -q 'aggregate=INCONCLUSIVE' "$out_dir/missing-package.console"
set +e
./rf pilot evidence adjudicate "$out_dir/missing.package" "$out_dir/missing.adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/missing-adjudication.console" 2>&1
missing_rc=$?
set -e
[ "$missing_rc" -eq 3 ]
rg -q '^decision=INCONCLUSIVE$' "$out_dir/missing.adjudication"

sed -E 's/^(metric=false-pass-count\t[^\t]*\t[^\t]*\t[^\t]*\tCOMPLETE\t)PASS/\1FAIL/' \
  "$input" > "$out_dir/fail.input"
./rf pilot evidence package "$out_dir/fail.input" "$out_dir/fail.package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/fail-package.console"
rg -q 'aggregate=FAIL' "$out_dir/fail-package.console"
set +e
./rf pilot evidence adjudicate "$out_dir/fail.package" "$out_dir/fail.adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/fail-adjudication.console" 2>&1
fail_rc=$?
set -e
[ "$fail_rc" -eq 1 ]
rg -q '^decision=NO_GO$' "$out_dir/fail.adjudication"

wrong_contract="$(printf 'wrong-approved-contract\n' | sha256sum | awk '{print $1}')"
sed "s/^contract-digest=.*/contract-digest=$wrong_contract/" "$input" > "$out_dir/wrong-contract.input"
set +e
./rf pilot evidence package "$out_dir/wrong-contract.input" "$out_dir/wrong-contract.package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/wrong-contract.console" 2>&1
wrong_contract_rc=$?
set -e
[ "$wrong_contract_rc" -eq 2 ]
rg -q 'contract digest does not match' "$out_dir/wrong-contract.console"

wrong_sample="$(printf 'wrong-registered-sample\n' | sha256sum | awk '{print $1}')"
set +e
./rf pilot evidence package "$input" "$out_dir/wrong-sample.package" \
  --contract-digest "$contract_digest" --sample-digest "$wrong_sample" \
  > "$out_dir/wrong-sample.console" 2>&1
wrong_sample_rc=$?
set -e
[ "$wrong_sample_rc" -eq 2 ]
rg -q 'sample digest does not match' "$out_dir/wrong-sample.console"

sed 's/^metric=author-active-time\tpilot-check:author-active-time/metric=author-active-time\tpilot-check:tampered/' \
  "$package" > "$out_dir/tampered.package"
set +e
./rf pilot evidence adjudicate "$out_dir/tampered.package" "$out_dir/tampered.adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/tampered.console" 2>&1
tampered_rc=$?
set -e
[ "$tampered_rc" -eq 3 ]
rg -q '^decision=INCONCLUSIVE$' "$out_dir/tampered.adjudication"

sed '/^package-digest=/d' "$package" > "$out_dir/missing-digest.package"
set +e
./rf pilot evidence adjudicate "$out_dir/missing-digest.package" "$out_dir/missing-digest.adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/missing-digest.console" 2>&1
missing_digest_rc=$?
set -e
[ "$missing_digest_rc" -eq 3 ]
rg -q '^decision=INCONCLUSIVE$' "$out_dir/missing-digest.adjudication"

printf '%s\n' 'format=erf-pilot-evidence-package-v1' > "$out_dir/malformed.package"
set +e
./rf pilot evidence adjudicate "$out_dir/malformed.package" "$out_dir/malformed.adjudication" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/malformed.console" 2>&1
malformed_rc=$?
set -e
[ "$malformed_rc" -eq 3 ]
rg -q '^decision=INCONCLUSIVE$' "$out_dir/malformed.adjudication"

sed -e 's/^window-start=.*/window-start=1790812801/' \
  -e 's/\t1790812800\t/\t1790812801\t/g' "$input" > "$out_dir/wrong-window.input"
set +e
./rf pilot evidence package "$out_dir/wrong-window.input" "$out_dir/wrong-window.package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/wrong-window.console" 2>&1
wrong_window_rc=$?
set -e
[ "$wrong_window_rc" -eq 2 ]
rg -q 'window does not match the approved' "$out_dir/wrong-window.console"

cp "$input" "$out_dir/duplicate.input"
rg '^metric=precision\t' "$input" >> "$out_dir/duplicate.input"
set +e
./rf pilot evidence package "$out_dir/duplicate.input" "$out_dir/duplicate.package" \
  --contract-digest "$contract_digest" --sample-digest "$sample_digest" \
  > "$out_dir/duplicate.console" 2>&1
duplicate_rc=$?
set -e
[ "$duplicate_rc" -eq 2 ]
rg -q 'duplicated or conflicting' "$out_dir/duplicate.console"

input_identity="$(sha256sum "$contract" "$sample" "$input" runtime/pilot-evidence/src/lib.rs runtime/pilot-evidence/src/main.rs rf | sha256sum | awk '{print $1}')"
cat > "$out_dir/pilot-evidence.md" <<EOF
# Pilot Evidence Package Acceptance

- Acceptance command: scripts/pilot-evidence-check.sh $out_dir
- Contract: $contract (sha256:$contract_digest)
- Registered sample: $sample (sha256:$sample_digest)
- Input identity: sha256:$input_identity
- Evidence level: EXECUTABLE
- Disclosure: digest-only; no source, complete Finding, SCM state, credential or
  transport is accepted.

| Boundary | Evidence | Result |
| --- | --- | --- |
| Complete package aggregates all required metric classes | package.console | PASS |
| Complete package admits only governance-owner GO | adjudication.txt | PASS |
| Missing metric cannot become GO | missing-adjudication.console | PASS |
| Known FAIL outranks missing/uncertain evidence | fail-adjudication.console | PASS |
| Wrong approved-contract digest is rejected | wrong-contract.console | PASS |
| Wrong registered-sample digest is rejected | wrong-sample.console | PASS |
| Tampered package is INCONCLUSIVE | tampered.adjudication | PASS |
| Missing package digest is INCONCLUSIVE | missing-digest.adjudication | PASS |
| Malformed package is INCONCLUSIVE | malformed.adjudication | PASS |
| Wrong approved observation window is rejected | wrong-window.console | PASS |
| Duplicate/conflicting metric is rejected | duplicate.console | PASS |

Aggregation is fixed as FAIL > INCONCLUSIVE > PASS. The package digest covers
all identities, windows, provenance, completeness, statuses and artifact
digests; the adjudication digest covers the package identity, fixed owner and
decision. A GO does not grant SCM or protected-branch authority.
EOF

echo "pilot-evidence=PASS evidence=$out_dir/pilot-evidence.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
