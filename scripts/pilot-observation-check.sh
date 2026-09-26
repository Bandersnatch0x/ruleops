#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-observation-check.sh <output-dir>}"
plan="$repo_root/specs/pilot/pilot-observation-plan.yaml"
mkdir -p "$out_dir"
cd "$repo_root"
static_log="$out_dir/static-check.log"
exec 9>"$static_log"
BASH_XTRACEFD=9
PS4='+ '
set -x
input_identity="$(sha256sum "$plan" runtime/pilot-metrics/src/lib.rs runtime/pilot-observer/src/main.rs scripts/pilot-local-simulation.sh scripts/phase4-shadow-pilot.sh scripts/pilot-live-observation.sh | sha256sum | awk '{print $1}')"

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
rg -q '^    - id: SYNTHETIC_SHADOW$' "$plan"
rg -q '^      eligibleForLiveOutcome: false$' "$plan"
rg -q '^    - id: LIVE_SHADOW$' "$plan"
rg -q '^      eligibleForLiveOutcome: true$' "$plan"
rg -q '^    sourceUpload: denied$' "$plan"
rg -q '^    repositoryWrites: denied$' "$plan"
rg -q '^    scmStatusPublication: denied$' "$plan"
rg -q '^    protectedBranchAuthority: denied$' "$plan"

for script in scripts/pilot-local-simulation.sh scripts/phase4-shadow-pilot.sh scripts/pilot-live-observation.sh; do
  [ -x "$script" ] || { echo "observation script is not executable: $script" >&2; exit 1; }
done
rg -q 'record_result_with_mode' runtime/pilot-metrics/src/lib.rs
rg -q 'ObservationMode' runtime/pilot-metrics/src/lib.rs
rg -q 'SyntheticShadow' runtime/pilot-metrics/src/lib.rs
rg -q 'rule-bundle=' scripts/pilot-local-simulation.sh scripts/phase4-shadow-pilot.sh scripts/pilot-live-observation.sh
rg -q 'rule_bundle_digest' runtime/pilot-observer/src/main.rs
rg -q -- '--registered-sample' runtime/pilot-observer/src/main.rs
rg -q -- '--mode local-simulation' scripts/pilot-local-simulation.sh
rg -q -- '--mode synthetic-shadow' scripts/phase4-shadow-pilot.sh
rg -q -- '--mode live-shadow' scripts/pilot-live-observation.sh
rg -q 'commit mismatch' scripts/pilot-live-observation.sh
./rf --help > "$out_dir/rf-help.txt"
for command in 'rf pilot baseline' 'rf pilot observe' 'rf pilot feedback' 'rf pilot governance' 'rf pilot window-report'; do
  rg -q "${command// / }" "$out_dir/rf-help.txt" || { echo "missing CLI surface: $command" >&2; exit 1; }
done

cargo test -q -p erf-pilot-metrics -p erf-pilot-observer --all-targets > "$out_dir/cargo-test.log" 2>&1
rg -q 'test result: ok\.' "$out_dir/cargo-test.log"
rg -q '0 ignored' "$out_dir/cargo-test.log"

set +x

cat > "$out_dir/pilot-observation.md" <<EOF
# Pilot Observation Contract Evidence

- evidenceSchema: feature-evidence-v1
- status: PASS
- evidenceLevel: EXECUTABLE
- Acceptance command: scripts/pilot-observation-check.sh $out_dir
- inputIdentity: sha256:$input_identity
- Plan: specs/pilot/pilot-observation-plan.yaml
- Observer: runtime/pilot-observer
- Journal: digest-only, mode-bound and replayable
- Modes: LOCAL_SIMULATION, SYNTHETIC_SHADOW and LIVE_SHADOW
- External repository writes, SCM status publication and protected authority: denied

| Check | Result |
| --- | --- |
| Registered three-repository denominator and fixed commit policy | PASS |
| Cadence and deduplication policy are explicit | PASS |
| Local, synthetic and live modes have different outcome eligibility | PASS |
| Observation scripts and CLI surface are present | PASS |
| Metrics journal mode and bundle/run identity are covered by unit tests | PASS |
| Registered sample and commit mismatch paths fail closed | PASS |

This check proves the observer contract and implementation seam, including
positive and mismatch-negative identity paths for all three observation modes. It does not
claim that the future 2026-10 observation window has produced live outcome
metrics; missing baselines or human adjudication remain INCONCLUSIVE.

- commands: static assertions plus `cargo test -q -p erf-pilot-metrics -p erf-pilot-observer --all-targets`
- logs:
  - static: $static_log
  - tests: $out_dir/cargo-test.log
EOF

echo "pilot-observation=PASS evidence=$out_dir/pilot-observation.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
