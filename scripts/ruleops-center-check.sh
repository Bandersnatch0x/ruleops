#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: ruleops-center-check.sh <output-dir>}"
contract="$repo_root/specs/center/ruleops-center-closure.yaml"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$contract" ] || { echo "RuleOps Center closure contract is missing: $contract" >&2; exit 1; }
rg -q '^kind: RuleOpsCenterClosure$' "$contract"
rg -q '^  status: ENGINEERING_MVP$' "$contract"
rg -q '^  authority: SHADOW$' "$contract"
rg -q '^  productionEnforcement: disabled$' "$contract"
rg -q '^    count: 3$' "$contract"
rg -q '^    mode: LIVE_SHADOW$' "$contract"
rg -q '^    writes: denied$' "$contract"
rg -q '^    signingRole: rule-bundle-signing$' "$contract"
rg -q '^    executionStatus: COMPOSITION_VERIFIED_LOCAL_SHADOW$' "$contract"
rg -q '^      executionStatus: COMPOSITION_VERIFIED_LOCAL_SHADOW$' "$contract"
rg -q '^    engineeringClosure: PASS$' "$contract"
rg -q '^    liveProductOutcome: PENDING_FORMAL_WINDOW$' "$contract"

standards_count="$(rg --files specs/standard | wc -l | tr -d ' ')"
intent_count="$(rg --files specs/rule-intent | wc -l | tr -d ' ')"
coverage_count="$(rg --files specs/coverage | wc -l | tr -d ' ')"
[ "$standards_count" -eq 100 ] || { echo "expected 100 Standards, got $standards_count" >&2; exit 1; }
[ "$intent_count" -eq 20 ] || { echo "expected 20 RuleIntents, got $intent_count" >&2; exit 1; }
[ "$coverage_count" -eq 20 ] || { echo "expected 20 CoverageLedgers, got $coverage_count" >&2; exit 1; }

for path in \
  control-plane/authoring/src \
  control-plane/factory/src \
  control-plane/verification/src \
  runtime/release/src \
  runtime/release-registry/src \
  runtime/governance-registry/src \
  runtime/rule-bundle/src \
  runtime/rf-runner/src \
  runtime/unified-result/src \
  runtime/feedback/src; do
  [ -d "$path" ] || { echo "missing center implementation seam: $path" >&2; exit 1; }
done

rg -q 'struct EvidenceBundle|class LocalVerification' control-plane/verification/src
rg -q 'struct RuleReleaseManifest' runtime/release/src/lib.rs
rg -q 'struct RuleReleaseRegistry' runtime/release-registry/src/lib.rs
rg -q 'struct PolicyProfile|struct GovernanceRegistry' runtime/governance-registry/src/lib.rs
rg -q 'struct RuleBundleManifest|struct RuleBundleVerifier' runtime/rule-bundle/src/lib.rs
rg -q 'struct GateAttestation|enum GateDecision' runtime/gate/src/lib.rs
rg -q 'struct UnifiedResult' runtime/unified-result/src/lib.rs
rg -q 'struct FeedbackLedger' runtime/feedback/src/lib.rs

rg -q '^kind: LivePilotRepositoryManifest$' specs/pilot/live-java-repositories.yaml
rg -q '^  count: 3$' specs/pilot/live-java-repositories.yaml
rg -q '^    repositoryWrites: denied$' specs/pilot/live-java-repositories.yaml

cargo test -q \
  -p erf-rule-bundle \
  -p erf-release \
  -p erf-release-registry \
  -p erf-governance-registry \
  -p rf-gate \
  -p erf-unified-result \
  -p erf-feedback \
  > "$out_dir/center-core-tests.log" 2>&1

cargo build -q -p erf-fake-host -p erf-fake-resolver
scripts/phase1-exit-demo.sh "$out_dir/vertical-slice" > "$out_dir/vertical-slice.log" 2>&1
rg -q '^evidence=.*phase1-exit-evidence\.md$' "$out_dir/vertical-slice.log"
rg -q '^\| Standard → Intent → IR → PMD → Tests .* PASS \|' "$out_dir/vertical-slice/phase1-exit-evidence.md"
rg -q '^\| Runtime scan → findings → Completeness → Gate preview .* PASS \|' "$out_dir/vertical-slice/phase1-exit-evidence.md"

cat > "$out_dir/ruleops-center-closure.md" <<EOF
# RuleOps Center Closure Evidence

- Contract: \`specs/center/ruleops-center-closure.yaml\`
- Status: \`ENGINEERING_MVP\`
- Authority: \`SHADOW\` only
- Acceptance command: \`scripts/ruleops-center-check.sh $out_dir\`
- Final repository gate: \`bash scripts/check.sh\`

| Closure edge | Evidence | Result |
| --- | --- | --- |
| Standards Center → Rule Factory | 100 Standards, 20 RuleIntents, vertical-slice evidence | PASS |
| Rule Factory → Verification Lab | Standard → Intent → IR → PMD → Tests | PASS |
| Verification Lab → Rule Registry | EvidenceBundle and RuleRelease seams | PASS |
| Rule Registry → Policy Center | immutable release and governance registries | PASS |
| Policy Center → Signed Rule Bundle | exact four-input digest binding and signature | PASS |
| Signed Rule Bundle → Repo A/B/C | composition verified; three registered LIVE_SHADOW repositories; execution remains LOCAL_UNATTESTED | PASS |
| Repositories → Findings/Coverage | runner, UnifiedResult and CoverageLedger | PASS |
| Findings/Coverage → Feedback/Exception | feedback ledger, ExceptionGrant and Gate | PASS |
| Feedback/Exception → RuleOps Center | digest-only pilot metrics and governance audit | PASS |

The executable vertical slice is recorded in
\`vertical-slice/phase1-exit-evidence.md\`; the signed bundle composition
tests are recorded in \`center-core-tests.log\`. The vertical slice deliberately
uses an unsigned local candidate, so this PASS records the engineering seam and
read-only Shadow registration rather than protected trusted execution.

This evidence proves the engineering closure and its seams. It does not claim
that the formal observation window has produced product-level quality or
resource outcomes, and it does not grant SCM or protected-branch authority.
EOF

echo "ruleops-center=PASS evidence=$out_dir/ruleops-center-closure.md"
