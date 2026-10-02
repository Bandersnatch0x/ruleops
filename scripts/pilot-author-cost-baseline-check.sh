#!/usr/bin/env bash
# Validate the author-active-time measurement contract and render a paired
# baseline report. Missing human timings remain INCONCLUSIVE.
# Usage: scripts/pilot-author-cost-baseline-check.sh OUTPUT_DIR [RECORD_FILE]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-author-cost-baseline-check.sh <output-dir> [record-file]}"
record_arg="${2:-specs/pilot/author-cost-record-template.yaml}"
case "$record_arg" in
  /*) record="$record_arg" ;;
  *) record="$repo_root/$record_arg" ;;
esac
baseline="$repo_root/specs/pilot/author-cost-baseline.yaml"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$baseline" ] || { echo "missing author cost baseline: $baseline" >&2; exit 1; }
[ -s "$record" ] || { echo "missing author cost record: $record" >&2; exit 1; }

rg -q '^kind: AuthorCostBaseline$' "$baseline"
rg -q '^  id: author-cost-baseline-v0$' "$baseline"
rg -q '^  status: INCONCLUSIVE$' "$baseline"
rg -q '^  authority: SHADOW$' "$baseline"
rg -q '^    id: author-active-time$' "$baseline"
rg -q '^    unit: minutes$' "$baseline"
rg -q '^    baselinePath: manual$' "$baseline"
rg -q '^    comparisonPath: template$' "$baseline"
rg -q '^    design: paired-within-rule$' "$baseline"
rg -q '^    runMetadata: >-$' "$baseline"
rg -q '^    missingRunMetadata: INCONCLUSIVE$' "$baseline"
rg -q '^    missingData: INCONCLUSIVE$' "$baseline"
rg -q '^      doNotRetuneAfterMeasurement: true$' "$baseline"
rg -q '^    humanAdjudication: HUMAN_INPUT_REQUIRED$' "$baseline"

rg -q '^kind: AuthorCostRecord$' "$record"
rg -q '^  baselineRef: specs/pilot/author-cost-baseline.yaml$' "$record"

sample_ids=(
  corp.java.no-system-exit
  corp.java.no-clone-method
  corp.java.avoid-duplicate-literals
)
sample_standards=(SEC-JAVA-017 SEC-JAVA-024 SEC-JAVA-018)
sample_scenarios=(direct-system-exit no-clone-method avoid-duplicate-literals)
sample_kinds=(method-call structural-node existing-rule)
sample_templates=(SymbolRule StructuralRule ExistingRule)
sample_intents=(
  specs/rule-intent/corp.java.no-system-exit.yaml
  specs/rule-intent/corp.java.no-clone-method.yaml
  specs/rule-intent/corp.java.avoid-duplicate-literals.yaml
)

sample_count="$(rg -c '^    - id:' "$record")"
[ "$sample_count" -eq 3 ] || { echo "expected three author-cost samples, got $sample_count" >&2; exit 1; }

sample_field() {
  local file="$1" id="$2" field="$3"
  awk -v target="$id" -v key="$field" '
    $0 == "    - id: " target { in_sample=1; next }
    in_sample && /^    - id:/ { exit }
    in_sample && index($0, "      " key ":") == 1 {
      value=$0
      sub("^      " key ":[[:space:]]*", "", value)
      print value
      exit
    }
  ' "$file"
}

path_field() {
  local file="$1" id="$2" path="$3" field="$4"
  awk -v target="$id" -v wanted_path="$path" -v key="$field" '
    $0 == "    - id: " target { in_sample=1; next }
    in_sample && /^    - id:/ { exit }
    in_sample && $0 == "      " wanted_path ":" { in_path=1; next }
    in_path && in_sample && /^      [A-Za-z][A-Za-z0-9_-]*:/ { in_path=0 }
    in_path && index($0, "        " key ":") == 1 {
      value=$0
      sub("^        " key ":[[:space:]]*", "", value)
      print value
      exit
    }
  ' "$file"
}

is_placeholder() {
  [ "$1" = "HUMAN_INPUT_REQUIRED" ]
}

is_number() {
  [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]]
}

run_field() {
  local file="$1" field="$2"
  awk -v wanted="$field" '
    $0 == "  run:" { in_run=1; next }
    in_run && /^  [A-Za-z][A-Za-z0-9_-]*:/ { exit }
    in_run && index($0, "    " wanted ":") == 1 {
      value=$0
      sub("^    " wanted ":[[:space:]]*", "", value)
      print value
      exit
    }
  ' "$file"
}

run_metadata_complete=1
for field in id authorId startedAtUtc completedAtUtc pathOrder notes; do
  value="$(run_field "$record" "$field")"
  [ -n "$value" ] || { echo "missing run metadata field for $field" >&2; exit 1; }
  if is_placeholder "$value"; then
    run_metadata_complete=0
  fi
done

median_three() {
  printf '%s\n' "$@" | sort -n | sed -n '2p'
}

manual_values=()
template_values=()
complete_pairs=0

for index in 0 1 2; do
  id="${sample_ids[$index]}"
  expected_standard="${sample_standards[$index]}"
  expected_scenario="${sample_scenarios[$index]}"
  expected_kind="${sample_kinds[$index]}"
  expected_template="${sample_templates[$index]}"
  expected_intent="${sample_intents[$index]}"

  [ "$(sample_field "$record" "$id" standard)" = "$expected_standard" ] || {
    echo "sample metadata mismatch for $id: standard" >&2
    exit 1
  }
  [ "$(sample_field "$record" "$id" scenario)" = "$expected_scenario" ] || {
    echo "sample metadata mismatch for $id: scenario" >&2
    exit 1
  }
  [ "$(sample_field "$record" "$id" targetKind)" = "$expected_kind" ] || {
    echo "sample metadata mismatch for $id: targetKind" >&2
    exit 1
  }
  [ "$(sample_field "$record" "$id" template)" = "$expected_template" ] || {
    echo "sample metadata mismatch for $id: template" >&2
    exit 1
  }
  [ "$(sample_field "$record" "$id" intent)" = "$expected_intent" ] || {
    echo "sample metadata mismatch for $id: intent" >&2
    exit 1
  }

  manual="$(path_field "$record" "$id" manual activeMinutes)"
  template="$(path_field "$record" "$id" template activeMinutes)"
  manual_machine="$(path_field "$record" "$id" manual machineElapsedSeconds)"
  template_machine="$(path_field "$record" "$id" template machineElapsedSeconds)"
  manual_evidence="$(path_field "$record" "$id" manual evidence)"
  template_evidence="$(path_field "$record" "$id" template evidence)"
  [ -n "$manual" ] || { echo "missing manual activeMinutes for $id" >&2; exit 1; }
  [ -n "$template" ] || { echo "missing template activeMinutes for $id" >&2; exit 1; }
  [ -n "$manual_machine" ] || { echo "missing manual machineElapsedSeconds for $id" >&2; exit 1; }
  [ -n "$template_machine" ] || { echo "missing template machineElapsedSeconds for $id" >&2; exit 1; }
  [ -n "$manual_evidence" ] || { echo "missing manual evidence for $id" >&2; exit 1; }
  [ -n "$template_evidence" ] || { echo "missing template evidence for $id" >&2; exit 1; }

  for value_name in manual template; do
    value="${!value_name}"
    if is_placeholder "$value"; then
      :
    elif ! is_number "$value"; then
      echo "invalid $value_name activeMinutes for $id: $value" >&2
      exit 1
    elif ! awk -v value="$value" 'BEGIN { exit !(value > 0) }'; then
      echo "$value_name activeMinutes must be greater than zero for $id" >&2
      exit 1
    fi
  done

  for value_name in manual_machine template_machine; do
    value="${!value_name}"
    if ! is_placeholder "$value" && ! is_number "$value"; then
      echo "invalid $value_name for $id: $value" >&2
      exit 1
    elif ! is_placeholder "$value" && ! awk -v value="$value" 'BEGIN { exit !(value >= 0) }'; then
      echo "$value_name must be non-negative for $id" >&2
      exit 1
    fi
  done

  if is_number "$manual" && is_number "$template" \
    && is_number "$manual_machine" && is_number "$template_machine" \
    && ! is_placeholder "$manual_evidence" && ! is_placeholder "$template_evidence"; then
    manual_values+=("$manual")
    template_values+=("$template")
    complete_pairs=$((complete_pairs + 1))
  else
    manual_values+=("HUMAN_INPUT_REQUIRED")
    template_values+=("HUMAN_INPUT_REQUIRED")
  fi
done

if [ "$complete_pairs" -eq 3 ] && [ "$run_metadata_complete" -eq 1 ]; then
  measurement_status=MEASURED
  manual_median="$(median_three "${manual_values[@]}")"
  template_median="$(median_three "${template_values[@]}")"
  summary_delta="$(awk -v manual="$manual_median" -v template="$template_median" 'BEGIN { printf "%.2f", manual-template }')"
  summary_reduction="$(awk -v manual="$manual_median" -v template="$template_median" 'BEGIN { printf "%.2f", (manual-template)/manual*100 }')"
  summary_reduction_display="${summary_reduction}%"
else
  measurement_status=INCONCLUSIVE
  manual_median=HUMAN_INPUT_REQUIRED
  template_median=HUMAN_INPUT_REQUIRED
  summary_delta=HUMAN_INPUT_REQUIRED
  summary_reduction=HUMAN_INPUT_REQUIRED
  summary_reduction_display=HUMAN_INPUT_REQUIRED
fi

if [ "$run_metadata_complete" -eq 1 ]; then
  run_metadata_status=COMPLETE
else
  run_metadata_status=HUMAN_INPUT_REQUIRED
fi

if [ "$measurement_status" = MEASURED ]; then
  evidence_note="The source record contains all three paired timings. The measurement is still not a Pilot Go/No-Go decision until the required human adjudication is recorded."
elif [ "$complete_pairs" -eq 3 ]; then
  evidence_note="The source record contains all three paired timings but still has placeholder run metadata. This evidence remains INCONCLUSIVE and does not claim an author-cost improvement or a Pilot Go/No-Go decision."
else
  evidence_note="The source record still contains missing human timing data or placeholder run metadata. This evidence does not claim an author-cost improvement or a Pilot Go/No-Go decision."
fi

record_digest="$(sha256sum "$record" | awk '{print $1}')"
report="$out_dir/author-cost-baseline.md"
summary_tsv="$out_dir/author-cost-summary.tsv"

{
  printf '%s\n' $'rule_id\tmanual_active_minutes\ttemplate_active_minutes\tdelta_minutes\trelative_reduction_percent\tstatus'
  for index in 0 1 2; do
    id="${sample_ids[$index]}"
    manual="${manual_values[$index]}"
    template="${template_values[$index]}"
    if is_number "$manual" && is_number "$template"; then
      delta="$(awk -v manual="$manual" -v template="$template" 'BEGIN { printf "%.2f", manual-template }')"
      reduction="$(awk -v manual="$manual" -v template="$template" 'BEGIN { printf "%.2f", (manual-template)/manual*100 }')"
      status=MEASURED
    else
      delta=HUMAN_INPUT_REQUIRED
      reduction=HUMAN_INPUT_REQUIRED
      status=INCONCLUSIVE
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$manual" "$template" "$delta" "$reduction" "$status"
  done
} > "$summary_tsv"

{
  cat <<EOF
# Author Active-Time Baseline Evidence

- Acceptance command: scripts/pilot-author-cost-baseline-check.sh $out_dir $record_arg
- Contract: specs/pilot/author-cost-baseline.yaml
- Record: $record_arg
- Record digest: \`$record_digest\`
- Measurement status: \`$measurement_status\`
- Completed paired samples: $complete_pairs / 3
- Run metadata: \`$run_metadata_status\`
- Authority: \`SHADOW\` only
- Missing data behavior: \`INCONCLUSIVE\`, never an implicit PASS

## Method

This is a paired within-rule comparison over three fixed RuleIntents. The
manual path is the baseline: the author hand-writes the equivalent rule
artifact and fixtures until they pass the validation boundary. The template
path uses the corresponding typed template to produce a Candidate and its
fixtures until they pass the same boundary. Author-active-time is human active
time in minutes; compiler and verifier elapsed time is recorded separately and
is not substituted for author time.

Absolute values are the two recorded active-time values. Per-rule delta is
\`manual - template\` minutes. Relative reduction is
\`(manual - template) / manual * 100\`. The summary uses the ratio of the two
medians. The pre-registered contract threshold is template median at most 80%
of manual median; it is not changed after measurement.

## Sample and results

| RuleIntent | Template | Manual active minutes | Template active minutes | Delta minutes | Relative reduction | Status |
| --- | --- | ---: | ---: | ---: | ---: | --- |
EOF
  for index in 0 1 2; do
    id="${sample_ids[$index]}"
    template_name="${sample_templates[$index]}"
    manual="${manual_values[$index]}"
    template="${template_values[$index]}"
    if is_number "$manual" && is_number "$template"; then
      delta="$(awk -v manual="$manual" -v template="$template" 'BEGIN { printf "%.2f", manual-template }')"
      reduction="$(awk -v manual="$manual" -v template="$template" 'BEGIN { printf "%.2f%%", (manual-template)/manual*100 }')"
      status=MEASURED
    else
      delta=HUMAN_INPUT_REQUIRED
      reduction=HUMAN_INPUT_REQUIRED
      status=INCONCLUSIVE
    fi
    printf '| `%s` | `%s` | `%s` | `%s` | `%s` | `%s` | `%s` |\n' \
      "$id" "$template_name" "$manual" "$template" "$delta" "$reduction" "$status"
  done
  cat <<EOF

## Summary

| Metric | Value |
| --- | ---: |
| Manual median active minutes | $manual_median |
| Template median active minutes | $template_median |
| Median delta minutes (manual - template) | $summary_delta |
| Median relative reduction | $summary_reduction_display |
| Contract comparison | template median <= manual median * 0.80 |
| Current decision | HUMAN_INPUT_REQUIRED / INCONCLUSIVE until the record is complete and adjudicated |

$evidence_note Fill a copied record when data is missing, rerun the same command,
and retain the generated report and TSV together under \`.scratch/\`.
EOF
} > "$report"

echo "author-cost-baseline=$measurement_status evidence=$report pairs=$complete_pairs/3"
