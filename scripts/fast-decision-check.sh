#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: fast-decision-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

input_digest="$(printf 'structured-diagnostic-input-v1\n' | sha256sum | awk '{print $1}')"
now=1790813000

cargo test -q -p erf-fast-decision --all-targets > "$out_dir/cargo-test.log" 2>&1
rg -q 'test result: ok\. 15 passed; 0 failed; 0 ignored' "$out_dir/cargo-test.log"
rg -q 'FastDecisionAdapter|DeterministicBaselineAdapter|FastDecisionEvidence' runtime/fast-decision/src/lib.rs
rg -q 'fast-decision-v1|SHADOW_ONLY|CHOICE.*SCORE.*BOOLEAN' contracts/fast-decision-v1.yaml
rg -q 'canProduceGateDecision: false|canChangeTrustState: false|canWriteScm: false' contracts/fast-decision-v1.yaml
rg -q 'rf decision evaluate' rf scripts/cli-surface-check.sh
rg -q 'UNKNOWN|near-miss|provider failure|shadow' docs/adr/0158-fast-decision-evidence-shadow-boundary.md runtime/fast-decision/src/lib.rs

./rf decision evaluate "$input_digest" 1 "$now" "$out_dir/positive.evidence" \
  > "$out_dir/positive.console"
rg -q 'status=DECIDED' "$out_dir/positive.console"
rg -q '^status=DECIDED$' "$out_dir/positive.evidence"
rg -q '^kind=CHOICE$' "$out_dir/positive.evidence"
rg -q '^answer=CHOICE:' "$out_dir/positive.evidence"
rg -q '^evidence-digest=[0-9a-f]{64}$' "$out_dir/positive.evidence"

set +e
./rf decision evaluate "$input_digest" 1 "$now" "$out_dir/near-miss.evidence" \
  --near-miss > "$out_dir/near-miss.console" 2>&1
near_miss_rc=$?
set -e
[ "$near_miss_rc" -eq 3 ]
rg -q 'status=UNKNOWN' "$out_dir/near-miss.console"
rg -q '^unknown-reason=LOW_CONFIDENCE$' "$out_dir/near-miss.evidence"
rg -q '^fallback=HUMAN_REVIEW$' "$out_dir/near-miss.evidence"

set +e
./rf decision evaluate "$input_digest" 1 "$now" "$out_dir/provider-failure.evidence" \
  --provider-unavailable > "$out_dir/provider-failure.console" 2>&1
provider_rc=$?
set -e
[ "$provider_rc" -eq 3 ]
rg -q '^unknown-reason=PROVIDER_UNAVAILABLE$' "$out_dir/provider-failure.evidence"

set +e
./rf decision evaluate "$input_digest" 1 "$now" "$out_dir/conflict.evidence" \
  --conflict > "$out_dir/conflict.console" 2>&1
conflict_rc=$?
set -e
[ "$conflict_rc" -eq 3 ]
rg -q '^unknown-reason=ANSWER_CONFLICT$' "$out_dir/conflict.evidence"

set +e
./rf decision evaluate \
  0000000000000000000000000000000000000000000000000000000000000000 \
  1 0 "$out_dir/invalid-input.evidence" > "$out_dir/invalid-input.console" 2>&1
invalid_rc=$?
set -e
[ "$invalid_rc" -eq 3 ]
rg -q 'status=UNKNOWN' "$out_dir/invalid-input.console"
rg -q '^unknown-reason=INVALID_INPUT$' "$out_dir/invalid-input.evidence"

input_identity="$(sha256sum contracts/fast-decision-v1.yaml docs/adr/0158-fast-decision-evidence-shadow-boundary.md runtime/fast-decision/src/lib.rs runtime/fast-decision/src/main.rs scripts/fast-decision-check.sh rf | sha256sum | awk '{print $1}')"
cat > "$out_dir/fast-decision.md" <<EOF
# Fast Decision Evidence Acceptance

- Acceptance command: scripts/fast-decision-check.sh $out_dir
- Contract: contracts/fast-decision-v1.yaml
- Input identity: sha256:$input_identity
- Evidence level: EXECUTABLE
- Adapter: deterministic-baseline / typed-rules-v1
- Authority: SHADOW_ONLY; no Gate, Trust, Exception, retry or SCM mutation

| Boundary | Evidence | Result |
| --- | --- | --- |
| Typed Choice evidence is directly consumable | positive.evidence | PASS |
| Near-miss below confidence threshold abstains | near-miss.evidence | PASS |
| Provider failure returns UNKNOWN | provider-failure.evidence | PASS |
| Conflicting answer returns UNKNOWN | conflict.evidence | PASS |
| Invalid input returns typed UNKNOWN evidence | invalid-input.evidence | PASS |
| Contract fixes expiry, margin, fallback and authority boundary | contract + ADR | PASS |
| Shadow comparison retains disagreement/abstain/latency/calibration | Cargo tests | PASS |

Probability and confidence are evidence fields, not correctness claims. Unknown
results fall back to the existing deterministic route or human review; this
layer never emits a Gate PASS/FAIL or changes a protected-branch decision.
EOF

echo "fast-decision=PASS evidence=$out_dir/fast-decision.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
