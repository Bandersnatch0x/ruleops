#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="$REPO_ROOT/.scratch/phase3-repeat-$STAMP"
REPEAT_REPO="$EVIDENCE_DIR/repo"
DIGESTS_FILE="$EVIDENCE_DIR/digests.tsv"
EVIDENCE_FILE="$EVIDENCE_DIR/phase3-repeatability.md"

mkdir -p "$REPEAT_REPO/src/main/java"
printf '%s\n' 'public class RepeatabilitySample { void run() { } }' > "$REPEAT_REPO/src/main/java/RepeatabilitySample.java"

cargo build --quiet -p rf-cli -p erf-fake-host -p erf-fake-resolver

baseline=""
for run in $(seq 1 10); do
  output_file="$EVIDENCE_DIR/run-$run.txt"
  "$REPO_ROOT/target/debug/rf-cli" scan \
    --repo-root "$REPEAT_REPO" \
    --rule-id corp.java.no-system-exit \
    --resolver "$REPO_ROOT/target/debug/erf-fake-resolver" \
    --host "$REPO_ROOT/target/debug/erf-fake-host" \
    > "$output_file" 2>&1
  digest="$(sed -n 's/.*semantic_result_digest=\(\[[^]]*\]\).*/\1/p' "$output_file" | tail -n 1)"
  if [[ -z "$digest" ]]; then
    echo "run $run did not emit semantic_result_digest; see $output_file" >&2
    exit 1
  fi
  printf '%s\t%s\n' "$run" "$digest" >> "$DIGESTS_FILE"
  if [[ -z "$baseline" ]]; then
    baseline="$digest"
  elif [[ "$digest" != "$baseline" ]]; then
    echo "semantic_result_digest mismatch on run $run: expected $baseline, got $digest" >&2
    exit 1
  fi
done

{
  printf '%s\n' '# Phase 3 repeatability evidence'
  printf '\n%s\n' '- Runs: 10'
  printf '%s\n' "- Resolver: $REPO_ROOT/target/debug/erf-fake-resolver"
  printf '%s\n' "- Host: $REPO_ROOT/target/debug/erf-fake-host"
  printf '%s\n' '- Rule: corp.java.no-system-exit'
  printf '%s\n\n' "- Repository fixture: $REPEAT_REPO"
  printf '%s\n' '| Run | SemanticResultDigest |'
  printf '%s\n' '| ---: | --- |'
  while IFS=$'\t' read -r run digest; do
    printf '%s\n' "| $run | $digest |"
  done < "$DIGESTS_FILE"
} > "$EVIDENCE_FILE"

printf 'repeatability=PASS runs=10 semantic_result_digest=%s\nevidence=%s\n' "$baseline" "$EVIDENCE_FILE"
