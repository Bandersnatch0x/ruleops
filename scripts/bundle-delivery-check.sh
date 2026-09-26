#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/bundle-delivery-check}"
CONTRACT="$REPO_ROOT/contracts/http/rule-bundle-delivery-v1.yaml"
CACHE="$REPO_ROOT/runtime/bundle-cache/src/lib.rs"
mkdir -p "$OUT_DIR"
TEST_LOG="$OUT_DIR/bundle-cache-tests.log"
STATIC_LOG="$OUT_DIR/static-check.log"
static_failures=()

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
record_input "$CACHE"
contract_digest="$(digest_or_missing "$CONTRACT")"
cache_digest="$(digest_or_missing "$CACHE")"
static_status=STATIC_ONLY
behavior_status=BLOCKED
behavior_result="not run"
test_command="(cd $REPO_ROOT && cargo test --package erf-bundle-cache --all-targets)"
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

required=(
  'schemaVersion: rule-bundle-delivery-v1'
  'contentAddressed: true'
  'tagsAreIdentity: false'
  'overwrite: forbidden'
  'writeMode: atomic-create-only'
  'expiredIsInconclusive: true'
  'cacheMiss: SERVER_FETCH_REQUIRED'
  'cacheMiss: INCONCLUSIVE'
  'failedCheckOutcome: INCONCLUSIVE'
  'sourceUploadDefault: false'
)
for needle in "${required[@]}"; do
  check_static "$needle" "$CONTRACT"
done

for needle in \
  'pub trait BundleCache' \
  'pub struct FileBundleCache' \
  'pub fn resolve_cached' \
  'DeploymentProfile::EnterpriseAirGapped' \
  'CacheError::Corrupt' \
  'ServerFetchRequired'; do
  check_static "$needle" "$CACHE"
done

write_report() {
  cat > "$OUT_DIR/bundle-delivery.md" <<EOF
# RuleBundle Delivery Evidence

- evidenceSchema: feature-evidence-v1
- feature: bundle-delivery-cache
- status: $behavior_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/rule-bundle-delivery-v1.yaml (sha256:$contract_digest)
  - implementation: runtime/bundle-cache/src/lib.rs (sha256:$cache_digest)
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - behavior: $test_command
- logs:
  - static: $STATIC_LOG
  - behavior: $TEST_LOG

| Check | Evidence | Status |
| --- | --- | --- |
| Contract fields and source symbols exist | contract + implementation rg assertions | $static_status |
| Descriptor, cache immutability, corruption, expiry and profile boundaries | $test_command | $behavior_status |

- staticResult: ${static_failures[*]:-all assertions passed}
- behaviorResult: $behavior_result
- keyNegativeFixtures: corrupted_cache_entry_is_rejected_after_restart; offline_profiles_fail_closed_while_connected_profile_requests_fetch; expiry_and_verification_gaps_are_inconclusive
- privacy: source/full Finding upload remains disabled by default
- decision: static checks do not claim runtime behavior; executable Cargo evidence is required for overall PASS
EOF
}

if ((${#static_failures[@]} > 0)); then
  static_status=BLOCKED
  behavior_result="not run: static input assertion failed"
  printf 'staticFailure=%s\n' "${static_failures[@]}" >>"$STATIC_LOG"
  write_report
  cat "$STATIC_LOG" >&2
  exit 1
fi
printf '%s\n' 'static assertions complete: all passed' >>"$STATIC_LOG"

if (cd "$REPO_ROOT" && cargo test --package erf-bundle-cache --all-targets) >"$TEST_LOG" 2>&1; then
  behavior_status=PASS
  test_summary="$(rg 'test result:' "$TEST_LOG" | paste -sd ';' - || true)"
  behavior_result="exit=0; $test_summary"
  for fixture in \
    'corrupted_cache_entry_is_rejected_after_restart' \
    'offline_profiles_fail_closed_while_connected_profile_requests_fetch' \
    'expiry_and_verification_gaps_are_inconclusive'; do
    rg -F -q -- "$fixture ... ok" "$TEST_LOG" || {
      behavior_status=BLOCKED
      behavior_result="required fixture did not pass or was missing from test log: $fixture"
      write_report
      cat "$TEST_LOG" >&2
      exit 1
    }
  done
  if rg -q 'test result:.*[1-9][0-9]* ignored' "$TEST_LOG"; then
    behavior_status=BLOCKED
    behavior_result="cargo test reported ignored tests; executable evidence is incomplete"
    write_report
    cat "$TEST_LOG" >&2
    exit 1
  fi
else
  behavior_status=BLOCKED
  behavior_result="cargo test failed; see $TEST_LOG"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi

write_report
echo "bundle-delivery=PASS evidence=$OUT_DIR/bundle-delivery.md evidenceLevel=EXECUTABLE"
