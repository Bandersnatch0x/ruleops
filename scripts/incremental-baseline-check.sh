#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: incremental-baseline-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

crate="runtime/incremental-baseline"
[ -s "$crate/Cargo.toml" ]
[ -s "$crate/src/lib.rs" ]
[ -s "$crate/examples/render_baseline.rs" ]
rg -q 'FallbackReason::BuildConfigurationChanged' "$crate/src/lib.rs"
rg -q 'FallbackReason::DependencyGraphUncertain' "$crate/src/lib.rs"
rg -q 'FallbackReason::GeneratedSourceChanged' "$crate/src/lib.rs"
rg -q 'FallbackReason::CacheCorrupt' "$crate/src/lib.rs"
rg -q 'FallbackReason::CacheExpired' "$crate/src/lib.rs"
rg -q 'FallbackReason::MergeBaseUnverifiable' "$crate/src/lib.rs"
rg -q 'FallbackReason::ScopeBudgetExceeded' "$crate/src/lib.rs"
rg -q 'StageTimer' "$crate/src/lib.rs"
rg -q 'percentile' "$crate/src/lib.rs"
rg -q 'SecurityComparison' "$crate/src/lib.rs"
rg -q 'fallback_inputs' runtime/rf-runner/src/main.rs
rg -q 'stage_telemetry' runtime/rf-runner/src/main.rs
rg -q 'allows_authoritative_gate' runtime/rf-cli/src/main.rs runtime/trusted-ci/src/lib.rs runtime/pilot-observer/src/main.rs
! rg -n 'Command::new|reqwest|ureq|std::net|std::process' "$crate/src" "$crate/examples"

command -v javac >/dev/null 2>&1
command -v python3 >/dev/null 2>&1
fixture_tmp="$(mktemp -d)"
trap 'rm -rf "$fixture_tmp"' EXIT
: > "$out_dir/fixture-compile.log"
for scale in small medium large; do
  mapfile -t sources < <(rg --files "$crate/fixtures/$scale" | rg '\.java$' | sort)
  [ "${#sources[@]}" -gt 0 ]
  mkdir -p "$fixture_tmp/$scale"
  javac --release 21 -d "$fixture_tmp/$scale" "${sources[@]}" > "$out_dir/$scale-javac.log" 2>&1
  printf '%s fixture_sources=%s compile=PASS\n' "$scale" "${#sources[@]}" >> "$out_dir/fixture-compile.log"
done

timing_file="$out_dir/fixture-timings.tsv"
: > "$timing_file"
for scale in small medium large; do
  mapfile -t sources < <(rg --files "$crate/fixtures/$scale" | rg '\.java$' | sort)
  for band in FULL INCREMENTAL_COLD INCREMENTAL_WARM; do
    timing_dir="$fixture_tmp/timing/$scale/$band"
    if [ "$band" = FULL ]; then
      compile_sources=("${sources[@]}")
    else
      # The first sorted source is a leaf in each local fixture. This keeps the
      # incremental probe bounded while the full path still compiles the set.
      compile_sources=("${sources[0]}")
    fi
    if [ "$band" = INCREMENTAL_WARM ]; then
      mkdir -p "$timing_dir"
      javac --release 21 -d "$timing_dir" "${compile_sources[@]}" > "$out_dir/$scale-warm-prime-javac.log" 2>&1
    fi
    for repetition in 0 1 2; do
      if [ "$band" != INCREMENTAL_WARM ]; then
        rm -rf "$timing_dir"
        mkdir -p "$timing_dir"
      fi
      started_at="$(python3 -c 'import time; print(time.monotonic_ns())')"
      javac --release 21 -d "$timing_dir" "${compile_sources[@]}" > "$out_dir/$scale-${band,,}-$repetition-javac.log" 2>&1
      finished_at="$(python3 -c 'import time; print(time.monotonic_ns())')"
      elapsed_nanos=$((finished_at - started_at))
      [ "$elapsed_nanos" -gt 0 ]
      printf '%s\t%s\t%s\t%s\n' "${scale^^}" "$band" "$repetition" "$elapsed_nanos" >> "$timing_file"
    done
  done
done

cargo test -q -p erf-incremental-baseline --all-targets > "$out_dir/incremental-baseline-tests.log"
rg -q 'test result: ok\.' "$out_dir/incremental-baseline-tests.log"
! rg -q 'failed: [1-9]' "$out_dir/incremental-baseline-tests.log"

cargo run -q -p erf-incremental-baseline --example render_baseline -- "$out_dir/incremental-baseline-report.md" "$timing_file" > "$out_dir/render-console.log"
rg -q '^incremental-baseline=.* status=INCONCLUSIVE$' "$out_dir/render-console.log"
rg -q '^- Synthetic repositories: `20`$' "$out_dir/incremental-baseline-report.md"
rg -q '^- Cases: `44`$' "$out_dir/incremental-baseline-report.md"
rg -q '^- Observed fixture timing samples: `27`$' "$out_dir/incremental-baseline-report.md"
rg -q '^- Observed fixture timing slices: ' "$out_dir/incremental-baseline-report.md"
for scale in SMALL MEDIUM LARGE; do
  rg -q "^\\| $scale \\|" "$out_dir/incremental-baseline-report.md"
done
for band in FULL INCREMENTAL_COLD INCREMENTAL_WARM; do
  rg -q "\\| $band \\|" "$out_dir/incremental-baseline-report.md"
done
rg -q 'Rule hit' "$out_dir/incremental-baseline-report.md"
rg -q 'Zero hit' "$out_dir/incremental-baseline-report.md"
rg -q 'cache-corrupt.*`[1-9]' "$out_dir/incremental-baseline-report.md"
rg -q 'build-failed.*`[1-9]' "$out_dir/incremental-baseline-report.md"
for reason in \
  build-configuration-changed rule-bundle-changed policy-changed resolver-changed \
  dependency-graph-uncertain generated-source-changed generated-source-uncertain \
  cache-corrupt cache-expired cache-identity-mismatch cache-incomplete \
  merge-base-unverifiable scope-budget-exceeded build-failed plan-not-authoritative \
  plan-incomplete plan-widened-to-full; do
  rg -q "^- \`$reason\`: \`[1-9][0-9]*\`$" "$out_dir/incremental-baseline-report.md" || {
    echo "missing fallback evidence: $reason" >&2
    exit 1
  }
done
rg -q 'no-authoritative-pass' "$out_dir/incremental-baseline-report.md"
rg -q 'diagnostic-only' "$out_dir/incremental-baseline-report.md"

cat > "$out_dir/incremental-baseline-evidence.md" <<EOF
# Incremental Scan Baseline Evidence

- Acceptance command: \`scripts/incremental-baseline-check.sh $out_dir\`
- Implementation: \`runtime/incremental-baseline\`
- Focused tests: PASS; see \`incremental-baseline-tests.log\`.
- Generated report: \`incremental-baseline-report.md\`.
- Java fixtures: local SMALL/MEDIUM/LARGE source sets compile with JDK 21;
  see \`fixture-compile.log\`, \`fixture-timings.tsv\` and the per-scale javac logs.
- Timing provenance: 27 local measurements (three repetitions for each of the
  nine FULL/INCREMENTAL_COLD/INCREMENTAL_WARM slices) feed the compile stage
  of the corresponding P50/P95 slices; FULL compiles the fixture set, while
  cold/warm probes compile a deterministic leaf source with a clean/primed
  output directory. The report carries their digest and no source/path data.
- Fixture matrix: 3 scales, 20 digest-only synthetic repository identities,
  44 cases, full/incremental-cold/incremental-warm bands, rule-hit and zero-hit
  paths, and build/cache fault injections.
- Fallback closure: all registered build, rule, policy, resolver, graph,
  generated-source, cache, merge-base, scope-budget and plan-coverage reasons
  are emitted as structured full-scan widening reasons.
- Safety: incremental/full digest mismatch, missing comparison, budget breach,
  injected build failure or cache fault is INCONCLUSIVE/diagnostic and cannot
  authorize a PASS or lower the gate threshold.
- Disclosure: no source, path, credential, external SCM, network or IDE data.
EOF

echo "incremental-baseline=PASS evidence=$out_dir/incremental-baseline-evidence.md"
