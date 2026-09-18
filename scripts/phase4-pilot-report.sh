#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-pilot-report.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

scripts/phase4-release-registry.sh "$out_dir/release" > "$out_dir/release-console.log"
scripts/phase4-governance-registry.sh "$out_dir/governance" > "$out_dir/governance-console.log"
scripts/phase4-unified-result.sh "$out_dir/unified" > "$out_dir/unified-console.log"
scripts/phase4-shadow-pilot.sh "$out_dir/shadow" > "$out_dir/shadow-console.log"
scripts/phase4-authority.sh "$out_dir/authority" > "$out_dir/authority-console.log"
scripts/phase4-pilot-metrics.sh "$out_dir/metrics" > "$out_dir/metrics-console.log"
scripts/phase4-coverage-ledger.sh "$out_dir/coverage" > "$out_dir/coverage-console.log"
scripts/pilot-success-contract-check.sh "$out_dir/contract" > "$out_dir/contract-console.log"
scripts/representative-java-repositories-check.sh "$out_dir/repository-manifest" > "$out_dir/repository-manifest-console.log"

rg -q '^registry=PASS ' "$out_dir/release-console.log"
rg -q '^governance=PASS ' "$out_dir/governance-console.log"
rg -q '^unified-result=PASS ' "$out_dir/unified-console.log"
rg -q '^Passed 7/7 checks\.' "$out_dir/unified/trusted-ci/phase3-real-scan.md"
rg -q '^shadow-pilot=PASS ' "$out_dir/shadow-console.log"
shadow_rule_count="$(sed -n 's/^- RuleIntents: `\([0-9][0-9]*\)`$/\1/p' "$out_dir/shadow/phase4-shadow-pilot.md")"
[ -n "$shadow_rule_count" ] && [ "$shadow_rule_count" -ge 20 ]
rg -q '^- Shadow repositories: 3$' "$out_dir/shadow/phase4-shadow-pilot.md"
rg -q '^authority=PASS ' "$out_dir/authority-console.log"
rg -q '^pilot-metrics=PASS ' "$out_dir/metrics-console.log"
rg -q '^coverage=PASS ' "$out_dir/coverage-console.log"
rg -q '^pilot-contract=PASS ' "$out_dir/contract-console.log"
rg -q '^representative-java-repositories=PASS ' "$out_dir/repository-manifest-console.log"
rg -q '^- Standard inventory: 100$' "$out_dir/coverage/phase4-coverage-ledger.md"
rg -q 'pilot authority remains `SHADOW`' "$out_dir/shadow/phase4-shadow-pilot.md"

cat > "$out_dir/phase4-pilot-report.md" <<EOF
# Phase 4 Pilot Acceptance Report

- Repository: \`$repo_root\`
- Evidence run: \`$out_dir\`
- Pilot authority: Shadow-only; this report does not authorize protected-branch enforcement.

| Area | Evidence | Result |
| --- | --- | --- |
| RuleRelease immutable lifecycle | release registry focused suite | PASS |
| PolicyProfile and ExceptionGrant lifecycle | governance registry focused suite | PASS |
| UnifiedResult and audit chain | unified-result suite | PASS |
| Real Java Runner -> Trusted CI handoff | 7-check trusted scan | PASS |
| Three-repository Shadow pilot | shadow/phase4-shadow-pilot.md | PASS |
| Representative Java repository identity manifest | repository-manifest/representative-java-repositories.md | PASS |
| Twenty governed RuleIntent qualification | shadow/phase4-shadow-pilot.md | PASS |
| Protected-branch authority boundary | authority/phase4-authority.md | PASS |
| Digest-only pilot measurement ledger | metrics/phase4-pilot-metrics.md | PASS |
| 100-standard Java inventory | coverage/phase4-coverage-ledger.md | PASS |
| Twenty Standard CoverageLedgers | coverage/phase4-coverage-ledger.md | PASS |
| Pilot Success Contract fail-closed boundary | contract/pilot-success-contract.md | PASS |
| SCM credential isolation | no SCM adapter in this repository slice | PASS |

## Measured boundary

- Representative language path: Java.
- Trusted execution platform: linux/amd64 test path.
- Standards inventory: one hundred Java Standard records; twenty are
  explicitly promoted into the current candidate pack and eighty remain
  inventory-only.
- Repository qualification: three distinct Shadow repository and commit
  identities are exercised; this is count-based qualification evidence.
- Rule qualification: twenty governed Java RuleIntent paths pass authoring,
  fixture, EvidenceBundle, and deterministic recompilation checks.
- Pilot Success Contract: APPROVED for the registered 2026-10-01 through
  2026-10-31 window; this report does not treat pre-window rehearsals as live
  outcome measurements.
- Production enforcement: disabled. Local and test evidence remain distinct
  from a protected-branch authority decision.

## Operational counters

- Complete trusted scan: PASS path exercised.
- Incomplete/crash scan: INCONCLUSIVE -> ActionRequired path exercised.
- Immutable lifecycle rollback: exercised for release, profile, and grant.
- Audit tamper rejection: exercised for unified result and governance inputs.
- Failure injection: crash path remains INCONCLUSIVE -> ActionRequired in all
  three Shadow repository handoffs.
- Default disclosure: summaries and digests only; no source or full Finding
  upload path is introduced by this slice.

## Product metrics not yet measured

This generated snapshot does not claim product-level measurements for:

- new-violation versus historical-debt trends;
- false-positive regression outcomes;
- rollback and exception age/count over a live pilot window;
- scan latency and resource consumption.

Those require observations inside the registered live window and the required
human quality adjudication; count-based qualification or pre-window rehearsal
evidence must not be presented as those outcomes.

The observation runner remains Shadow-only; an explicit future authority
decision is still required before any protected-branch change.
EOF

echo "pilot-report=PASS evidence=$out_dir/phase4-pilot-report.md"
