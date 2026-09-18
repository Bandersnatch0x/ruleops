#!/usr/bin/env bash
# Exercise the observation seam with local commits. No remote is contacted or
# mutated; every clone and synthetic commit is confined to OUT_DIR.
# Usage: scripts/pilot-local-simulation.sh OUT_DIR [SOURCE_ROOT]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-local-simulation.sh OUT_DIR [SOURCE_ROOT]}"
source_root="${2:-$repo_root/.scratch/github-java-repositories-20260918/candidates}"
[ ! -e "$out_dir" ] || { echo "output directory already exists: $out_dir" >&2; exit 2; }
mkdir -p "$out_dir/repositories" "$out_dir/baselines" "$out_dir/runs"
cd "$repo_root"

java_host="${ERF_JAVA_HOST:-$repo_root/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar}"
resolver="$repo_root/target/debug/erf-static-resolver"
if [ ! -s "$java_host" ]; then
  mvn -q -B -ntp -f backend-hosts/java/pom.xml package -DskipTests
fi
[ -s "$java_host" ] || { echo "missing Java Host: $java_host" >&2; exit 1; }
if [ ! -x "$resolver" ]; then
  cargo build -q -p erf-static-resolver
fi

compiled="$(scripts/candidate-compile.sh specs/rule-intent/corp.java.avoid-duplicate-literals.yaml "$out_dir/candidate-build")"
candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate" ] || { echo "candidate path missing" >&2; exit 1; }

simulation_start="$(date -u +%s)"
baseline_latency=1
repositories=(word-wrap markedj keycloak-protocol-cas)

for repository in "${repositories[@]}"; do
  source="$source_root/$repository"
  destination="$out_dir/repositories/$repository"
  [ -d "$source/.git" ] || { echo "missing source clone: $source" >&2; exit 1; }
  git clone --local --no-hardlinks "$source" "$destination" >/dev/null
  git -C "$destination" config user.name "ERF Pilot Simulation"
  git -C "$destination" config user.email "pilot-simulation@invalid.example"
  git -C "$destination" switch -c "erf/pilot-simulation-$repository" >/dev/null
  base_commit="$(git -C "$destination" rev-parse HEAD)"
  baseline="$out_dir/baselines/$repository.baseline"
  ./rf pilot baseline \
    --repository "$repository" --commit "$base_commit" \
    --repo-root "$destination" --candidate "$candidate" \
    --host java --host-arg -jar --host-arg "$java_host" \
    --resolver "$resolver" --output "$baseline" \
    > "$out_dir/runs/$repository-baseline.console" 2>&1
  latency="$(sed -n 's/.*elapsed_ms=\([0-9][0-9]*\).*/\1/p' "$out_dir/runs/$repository-baseline.console")"
  [ -n "$latency" ] || { echo "baseline latency missing for $repository" >&2; exit 1; }
  [ "$latency" -gt "$baseline_latency" ] && baseline_latency="$latency"
  printf '%s\tbase\t%s\n' "$repository" "$base_commit" >> "$out_dir/commit-sequence.tsv"

  for stage in 1 2 3; do
    simulation_file="$destination/src/main/java/ErfPilotSimulation.java"
    if [ "$stage" -eq 1 ]; then
      mkdir -p "$(dirname "$simulation_file")"
      printf '%s\n' 'public final class ErfPilotSimulation { void run() { System.out.println("pilot-new-literal"); System.out.println("pilot-new-literal"); System.out.println("pilot-new-literal"); System.out.println("pilot-new-literal"); } }' > "$simulation_file"
    elif [ "$stage" -eq 2 ]; then
      printf '%s\n' 'public final class ErfPilotSimulation { private static final String MESSAGE = "pilot-new-literal"; void run() { System.out.println(MESSAGE); System.out.println(MESSAGE); System.out.println(MESSAGE); System.out.println(MESSAGE); } }' > "$simulation_file"
    else
      second_file="$destination/src/main/java/ErfPilotSimulationSecond.java"
      printf '%s\n' 'public final class ErfPilotSimulationSecond { void run() { System.out.println("pilot-second-literal"); System.out.println("pilot-second-literal"); System.out.println("pilot-second-literal"); System.out.println("pilot-second-literal"); } }' > "$second_file"
    fi
    git -C "$destination" add src/main/java/ErfPilotSimulation*.java
    git -C "$destination" commit -m "pilot simulation stage $stage" >/dev/null
    commit="$(git -C "$destination" rev-parse HEAD)"
    printf '%s\tstage-%s\t%s\n' "$repository" "$stage" "$commit" >> "$out_dir/commit-sequence.tsv"
    observed_at=$((simulation_start + stage))
    set +e
    ./rf pilot observe \
      --repository "$repository" --commit "$commit" \
      --repo-root "$destination" --candidate "$candidate" \
      --host java --host-arg -jar --host-arg "$java_host" \
      --resolver "$resolver" --baseline "$baseline" \
      --journal "$out_dir/metrics.journal" --mode local-simulation \
      --measurement-id "LOCAL_SIMULATION:$repository:$commit" \
      --observed-at "$observed_at" --reason "synthetic local commit stage $stage" \
      > "$out_dir/runs/$repository-stage-$stage.console" 2>&1
    status=$?
    set -e
    if [ "$status" -ne 0 ] && [ "$status" -ne 3 ]; then
      cat "$out_dir/runs/$repository-stage-$stage.console" >&2
      exit "$status"
    fi
  done
done

fingerprint="$(sed -n 's/^fingerprint=//p' "$out_dir/baselines/word-wrap.baseline" | head -1)"
if [ -n "$fingerprint" ]; then
  ./rf pilot feedback \
    --journal "$out_dir/metrics.journal" \
    --measurement-id "simulation-feedback:$fingerprint" \
    --classification false-positive --regression pending \
    --actor simulation-reviewer --reason "exercise local simulation feedback seam" \
    --observed-at "$((simulation_start + 10))" \
    > "$out_dir/runs/feedback.console" 2>&1
fi
./rf pilot governance \
  --journal "$out_dir/metrics.journal" \
  --measurement-id simulation-governance-1 \
  --rollback-count 0 --active-exception-count 0 --oldest-exception-age-secs 0 \
  --actor simulation-governance --reason "exercise local simulation governance seam" \
  --observed-at "$((simulation_start + 11))" \
  > "$out_dir/runs/governance.console" 2>&1

set +e
./rf pilot window-report \
  --journal "$out_dir/metrics.journal" \
  --output "$out_dir/local-simulation-report.md" \
  --window-start "$simulation_start" --window-end "$((simulation_start + 100))" \
  --expected-repositories 3 \
  --mode local-simulation \
  --latency-baseline-millis "$baseline_latency" \
  > "$out_dir/runs/window-report.console" 2>&1
report_status=$?
set -e
[ "$report_status" -eq 3 ] || { cat "$out_dir/runs/window-report.console" >&2; exit 1; }
rg -q '^window-report=INCONCLUSIVE ' "$out_dir/runs/window-report.console"
rg -q '^- Overall: `INCONCLUSIVE`$' "$out_dir/local-simulation-report.md"
[ "$(awk 'END { print NR }' "$out_dir/commit-sequence.tsv")" -eq 12 ]

cat > "$out_dir/local-simulation.md" <<EOF
# Local Pilot Observation Simulation

- Mode: LOCAL_SIMULATION
- Source: local clones of the registered three-repository sample
- Synthetic commits: 3 stages per repository
- Journal: $out_dir/metrics.journal
- Window report: $out_dir/local-simulation-report.md
- Live outcome eligibility: denied

The simulation validates commit identity, baseline classification, digest-only
metrics, feedback, governance, replay and fail-closed window aggregation. Its
INCONCLUSIVE quality result is expected because synthetic feedback is not a
live precision/recall adjudication.
EOF
echo "local-simulation=PASS evidence=$out_dir/local-simulation.md"
