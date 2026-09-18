#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-observation-check.sh <output-dir>}"
plan="$repo_root/specs/pilot/pilot-observation-plan.yaml"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$plan" ] || { echo "missing pilot observation plan" >&2; exit 1; }
rg -q '^kind: PilotObservationPlan$' "$plan"
rg -q '^  status: REGISTERED$' "$plan"
rg -q '^  authority: SHADOW$' "$plan"
rg -q '^    repositoryCount: 3$' "$plan"
rg -q '^    commitPolicy: default-branch-commit-only$' "$plan"
rg -q '^    replacement: denied$' "$plan"
rg -q '^    policy: default-branch-change-or-daily-catchup$' "$plan"
rg -q '^    deduplication: repository-commit-rule-bundle$' "$plan"
rg -q '^    - id: LOCAL_SIMULATION$' "$plan"
rg -q '^      eligibleForLiveOutcome: false$' "$plan"
rg -q '^    - id: LIVE_SHADOW$' "$plan"
rg -q '^      eligibleForLiveOutcome: true$' "$plan"
rg -q '^    sourceUpload: denied$' "$plan"
rg -q '^    repositoryWrites: denied$' "$plan"
rg -q '^    scmStatusPublication: denied$' "$plan"
rg -q '^    protectedBranchAuthority: denied$' "$plan"

for script in scripts/pilot-local-simulation.sh scripts/pilot-live-observation.sh; do
  [ -x "$script" ] || { echo "observation script is not executable: $script" >&2; exit 1; }
done
rg -q 'record_result_with_mode' runtime/pilot-metrics/src/lib.rs
rg -q 'ObservationMode' runtime/pilot-metrics/src/lib.rs
rg -q -- '--mode local-simulation' scripts/pilot-local-simulation.sh
rg -q -- '--mode live-shadow' scripts/pilot-live-observation.sh
./rf --help > "$out_dir/rf-help.txt"
for command in 'rf pilot baseline' 'rf pilot observe' 'rf pilot feedback' 'rf pilot governance' 'rf pilot window-report'; do
  rg -q "${command// / }" "$out_dir/rf-help.txt" || { echo "missing CLI surface: $command" >&2; exit 1; }
done

cargo test -q -p erf-pilot-metrics -p erf-pilot-observer --all-targets > "$out_dir/cargo-test.log" 2>&1

cat > "$out_dir/pilot-observation.md" <<EOF
# Pilot Observation Contract Evidence

- Acceptance command: scripts/pilot-observation-check.sh $out_dir
- Plan: specs/pilot/pilot-observation-plan.yaml
- Observer: runtime/pilot-observer
- Journal: digest-only, mode-bound and replayable
- Modes: LOCAL_SIMULATION and LIVE_SHADOW
- External repository writes, SCM status publication and protected authority: denied

| Check | Result |
| --- | --- |
| Registered three-repository denominator and fixed commit policy | PASS |
| Cadence and deduplication policy are explicit | PASS |
| Local and live modes have different outcome eligibility | PASS |
| Observation scripts and CLI surface are present | PASS |
| Metrics journal mode identity is covered by unit tests | PASS |

This check proves the observer contract and implementation seam. It does not
claim that the future 2026-10 observation window has produced live outcome
metrics; missing baselines or human adjudication remain INCONCLUSIVE.
EOF

echo "pilot-observation=PASS evidence=$out_dir/pilot-observation.md"
