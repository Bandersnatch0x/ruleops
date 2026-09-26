#!/usr/bin/env bash
# Validate the feature evidence contract and the evidence-level markers emitted
# by the checks upgraded under issue #120.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/feature-evidence-check}"
CONTRACT="$REPO_ROOT/contracts/http/feature-evidence-v1.yaml"
mkdir -p "$OUT_DIR"
STATIC_LOG="$OUT_DIR/static-check.log"
static_failures=()
covered_scripts=(
  "$REPO_ROOT/scripts/bundle-delivery-check.sh"
  "$REPO_ROOT/scripts/server-contract-check.sh"
  "$REPO_ROOT/scripts/server-persistence-check.sh"
  "$REPO_ROOT/scripts/thin-client-check.sh"
)

record_input() {
  local path="$1"
  [ -s "$path" ] || static_failures+=("missing input file: $path")
}

digest_or_missing() {
  local path="$1"
  if [ -s "$path" ]; then
    sha256sum "$path" | awk '{print $1}'
  else
    printf 'missing'
  fi
}

record_input "$CONTRACT"
for script in "${covered_scripts[@]}"; do record_input "$script"; done
contract_digest="$(digest_or_missing "$CONTRACT")"
script_digest=missing
if [ "${#static_failures[@]}" -eq 0 ]; then
  script_digest="$(sha256sum "${covered_scripts[@]}" | sha256sum | awk '{print $1}')"
fi
: >"$STATIC_LOG"

check_static() {
  local needle="$1"
  shift
  local command_repr="rg -F -n -- $(printf '%q' "$needle")"
  local file
  for file in "$@"; do command_repr+=" $(printf '%q' "$file")"; done
  printf '$ %s\n' "$command_repr" >>"$STATIC_LOG"
  if rg -F -n -- "$needle" "$@" >>"$STATIC_LOG" 2>&1; then
    printf 'status=PASS\n' >>"$STATIC_LOG"
  else
    static_failures+=("missing [$needle] in [$*]")
    printf 'status=BLOCKED\n' >>"$STATIC_LOG"
  fi
}

for needle in \
  'schemaVersion: feature-evidence-v1' \
  'status: [PASS, STATIC_ONLY, BLOCKED]' \
  'executableBehaviorPassMayBeReportedAs: PASS' \
  'structuralOnlyCheckMayBeReportedAs: STATIC_ONLY' \
  'blockedExecutableCheckMustNotBeReportedAs: PASS' \
  'failedTestLogMustNameTestFixtureOrContract: true'; do
  check_static "$needle" "$CONTRACT"
done

for script in \
  scripts/bundle-delivery-check.sh \
  scripts/server-contract-check.sh \
  scripts/server-persistence-check.sh \
  scripts/thin-client-check.sh; do
  if [ ! -x "$REPO_ROOT/$script" ]; then
    static_failures+=("covered script is not executable: $REPO_ROOT/$script")
  fi
  check_static 'STATIC_ONLY' "$REPO_ROOT/$script"
  check_static 'BLOCKED' "$REPO_ROOT/$script"
  check_static 'TEST_LOG' "$REPO_ROOT/$script"
  check_static 'inputIdentity' "$REPO_ROOT/$script"
done

write_report() {
  local report_status=STATIC_ONLY
  [ "${#static_failures[@]}" -eq 0 ] || report_status=BLOCKED
  cat > "$OUT_DIR/feature-evidence.md" <<EOF
# Feature Evidence Contract Check

- evidenceSchema: feature-evidence-v1
- feature: feature-evidence-contract
- status: $report_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/feature-evidence-v1.yaml (sha256:$contract_digest)
  - coveredScripts: scripts/bundle-delivery-check.sh, scripts/server-contract-check.sh, scripts/server-persistence-check.sh, scripts/thin-client-check.sh (sha256:$script_digest)
- checks: contract and covered script source-marker assertions
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
- logs:
  - static: $STATIC_LOG
- Covered scripts: bundle delivery, server contract, server persistence, thin client
- Required statuses: PASS / STATIC_ONLY / BLOCKED
- Required provenance: repository-relative input identity, reproducible command, stdout/stderr log
- Failure rule: a failed or unavailable executable check cannot be reported as PASS

| Check | Evidence | Result |
| --- | --- | --- |
| Evidence status vocabulary is closed | feature-evidence-v1.yaml | STATIC_ONLY |
| Covered checks expose STATIC_ONLY and BLOCKED branches | script source scan | STATIC_ONLY |
| Covered checks persist named test logs and input identity | script source scan | STATIC_ONLY |

- staticResult: ${static_failures[*]:-all assertions passed}
EOF
}

if [ "${#static_failures[@]}" -gt 0 ]; then
  printf 'staticFailure=%s\n' "${static_failures[@]}" >>"$STATIC_LOG"
  write_report
  cat "$STATIC_LOG" >&2
  exit 1
fi
printf '%s\n' 'static assertions complete: all passed' >>"$STATIC_LOG"
write_report

echo "feature-evidence=STATIC_ONLY evidence=$OUT_DIR/feature-evidence.md evidenceLevel=STRUCTURAL"
