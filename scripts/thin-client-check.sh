#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/thin-client-check}"
CONTRACT="$REPO_ROOT/contracts/http/thin-client-v1.yaml"
ADR="$REPO_ROOT/docs/adr/0152-thin-client-known-4xx-classification.md"
CLIENT="$REPO_ROOT/runtime/thin-client/src/lib.rs"
TRANSPORT="$REPO_ROOT/runtime/thin-client/src/transport.rs"
CLI="$REPO_ROOT/runtime/thin-client/src/main.rs"
EXPLAIN="$REPO_ROOT/runtime/thin-client/src/explain.rs"
RF="$REPO_ROOT/rf"
mkdir -p "$OUT_DIR"
TEST_LOG="$OUT_DIR/thin-client-tests.log"
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

for input in "$CONTRACT" "$ADR" "$CLIENT" "$TRANSPORT" "$CLI" "$EXPLAIN" "$RF"; do
  record_input "$input"
done
contract_digest="$(digest_or_missing "$CONTRACT")"
adr_digest="$(digest_or_missing "$ADR")"
client_digest="$(digest_or_missing "$CLIENT")"
transport_digest="$(digest_or_missing "$TRANSPORT")"
cli_digest="$(digest_or_missing "$CLI")"
explain_digest="$(digest_or_missing "$EXPLAIN")"
rf_digest="$(digest_or_missing "$RF")"
static_status=STATIC_ONLY
behavior_status=BLOCKED
behavior_result="not run"
test_command="(cd $REPO_ROOT && cargo test --package erf-thin-client --all-targets)"
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
  'schemaVersion: finding-view-v1' \
  'sourceUploadDefault: false' \
  'unknownFields: reject' \
  'status: PROPOSED' \
  'status: REQUESTED' \
  'grantsException: false' \
  'submitsToServer: false' \
  'submitFeedback:' \
  'submitException:' \
  'successMeans: request-recorded-only' \
  'unavailableOrUnknownResponse: SUBMISSION_UNKNOWN' \
  'known4xxStatusRange: "400-499"' \
  'redirectResponse: SUBMISSION_UNKNOWN' \
  'serverErrorResponse: SUBMISSION_UNKNOWN' \
  'responseIdentityRequired: true' \
  'unknownOutcomeReceipt: none' \
  'authority: serverOrHumanReview'; do
  check_static "$needle" "$CONTRACT"
done

for needle in \
  'pub struct FindingView' \
  'pub fn make_feedback_draft' \
  'pub fn make_exception_request' \
  'pub fn submit_feedback' \
  'pub fn submit_exception' \
  'classify_only_known_4xx_as_rejected' \
  'only_4xx_responses_are_known_rejections' \
  'missing_response_identity_is_unknown_even_for_4xx' \
  'ForbiddenPrivacyField' \
  'validate_relative_path' \
  'actions=show,navigate,feedback-draft,exception-request'; do
  check_static "$needle" "$CLIENT" "$TRANSPORT" "$CLI" "$EXPLAIN"
done

for command in \
  'rf explain FINDING.json' \
  'rf feedback draft FINDING.json' \
  'rf exception request FINDING.json' \
  'rf feedback submit REQUEST.json' \
  'rf exception submit REQUEST.json'; do
  check_static "$command" "$RF"
done

write_report() {
  cat > "$OUT_DIR/thin-client.md" <<EOF
# Thin Client Evidence

- evidenceSchema: feature-evidence-v1
- feature: thin-client-action-contract
- status: $behavior_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/thin-client-v1.yaml (sha256:$contract_digest)
  - decision: docs/adr/0152-thin-client-known-4xx-classification.md (sha256:$adr_digest)
  - implementation: runtime/thin-client/src/lib.rs (sha256:$client_digest)
  - transport: runtime/thin-client/src/transport.rs (sha256:$transport_digest)
  - cli: runtime/thin-client/src/main.rs (sha256:$cli_digest)
  - explain: runtime/thin-client/src/explain.rs (sha256:$explain_digest)
  - dispatcher: rf (sha256:$rf_digest)
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - behavior: $test_command
- logs:
  - static: $STATIC_LOG
  - behavior: $TEST_LOG

| Check | Evidence | Status |
| --- | --- | --- |
| Closed FindingView, action and CLI symbols exist | contract + client/transport/CLI rg assertions | $static_status |
| Digest-only parsing, forbidden fields, path safety, request determinism, receipt and rejection behavior | $test_command | $behavior_status |

- staticResult: ${static_failures[*]:-all assertions passed}
- behaviorResult: $behavior_result
- keyNegativeFixtures: source_and_unknown_fields_fail_closed; absolute_and_parent_paths_are_rejected; malformed_draft_and_server_rejection_fail_closed; only_4xx_responses_are_known_rejections; missing_response_identity_is_unknown_even_for_4xx
- authority: client cannot approve exceptions, alter Gate, write SCM or turn a receipt into authority
- IDE: action contract only; real plugin UI remains out of scope
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

if (cd "$REPO_ROOT" && cargo test --package erf-thin-client --all-targets) >"$TEST_LOG" 2>&1; then
  behavior_status=PASS
  test_summary="$(rg 'test result:' "$TEST_LOG" | paste -sd ';' - || true)"
  behavior_result="exit=0; $test_summary"
  for fixture in \
    'source_and_unknown_fields_fail_closed' \
    'absolute_and_parent_paths_are_rejected' \
    'malformed_draft_and_server_rejection_fail_closed' \
    'only_4xx_responses_are_known_rejections' \
    'missing_response_identity_is_unknown_even_for_4xx'; do
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
echo "thin-client=PASS evidence=$OUT_DIR/thin-client.md evidenceLevel=EXECUTABLE"
