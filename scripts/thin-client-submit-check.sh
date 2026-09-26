#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/thin-client-submit-check}"
CONTRACT="$REPO_ROOT/contracts/http/thin-client-v1.yaml"
ADR="$REPO_ROOT/docs/adr/0152-thin-client-known-4xx-classification.md"
CLIENT="$REPO_ROOT/runtime/thin-client/src/transport.rs"
SERVER="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java"
SERVER_TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java"
RF="$REPO_ROOT/rf"
mkdir -p "$OUT_DIR"

STATIC_LOG="$OUT_DIR/static-check.log"
COMMAND_LOG="$OUT_DIR/command.log"
SERVER_LOG="$OUT_DIR/server.log"
TEST_LOG="$OUT_DIR/thin-client-transport-tests.log"
SERVER_TEST_LOG="$OUT_DIR/server-tests.log"
BUILD_LOG="$OUT_DIR/server-build.log"
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

for file in "$CONTRACT" "$ADR" "$CLIENT" "$SERVER" "$SERVER_TEST" "$RF"; do
  record_input "$file"
done
contract_digest="$(digest_or_missing "$CONTRACT")"
adr_digest="$(digest_or_missing "$ADR")"
client_digest="$(digest_or_missing "$CLIENT")"
server_digest="$(digest_or_missing "$SERVER")"
server_test_digest="$(digest_or_missing "$SERVER_TEST")"
rf_digest="$(digest_or_missing "$RF")"
: >"$STATIC_LOG"
: >"$COMMAND_LOG"

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
  'endpointRequired: true' \
  'inputMustBeClosedAndDigestVerified: true' \
  'unavailableOrUnknownResponse: SUBMISSION_UNKNOWN' \
  'known4xxStatusRange: "400-499"' \
  'redirectResponse: SUBMISSION_UNKNOWN' \
  'serverErrorResponse: SUBMISSION_UNKNOWN' \
  'responseIdentityRequired: true' \
  'retryAutomatically: false' \
  'classify_only_known_4xx_as_rejected' \
  'governance_outcome: "RECEIVED_ONLY"' \
  '.shutdown(Shutdown::Write)' \
  'DIGEST_MISMATCH' \
  'thinClientGovernanceRequestsAreClosedDigestOnlyAndReplayable'; do
  check_static "$needle" "$CONTRACT" "$ADR" "$CLIENT" "$SERVER" "$SERVER_TEST"
done

if ((${#static_failures[@]} > 0)); then
  printf 'staticFailure=%s\n' "${static_failures[*]}" >>"$STATIC_LOG"
  cat "$STATIC_LOG" >&2
  exit 1
fi
printf '%s\n' 'static assertions complete: all passed' >>"$STATIC_LOG"

record_command() {
  local label="$1"
  shift
  {
    printf '## %s\n$' "$label"
    printf ' %q' "$@"
    printf '\n'
  } >>"$COMMAND_LOG"
}

run_logged() {
  local label="$1"
  shift
  record_command "$label" "$@"
  if "$@" >>"$COMMAND_LOG" 2>&1; then
    return 0
  fi
  cat "$COMMAND_LOG" >&2
  return 1
}

run_captured() {
  local label="$1"
  local output="$2"
  shift 2
  record_command "$label" "$@"
  if "$@" >"$output" 2>&1; then
    cat "$output" >>"$COMMAND_LOG"
    return 0
  fi
  cat "$output" >>"$COMMAND_LOG"
  cat "$output" >&2
  return 1
}

CP_FILE="$(mktemp)"
SERVER_PID=""
trap 'if [ -n "${SERVER_PID:-}" ]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi; rm -f "$CP_FILE"' EXIT
run_captured "Maven package and dependency classpath" "$BUILD_LOG" bash -c "cd \"$REPO_ROOT\" && mvn -q -pl control-plane/server -am package -DskipTests dependency:build-classpath -Dmdep.outputFile=\"$CP_FILE\" -q"
SERVER_CP="$REPO_ROOT/control-plane/server/target/classes:$(cat "$CP_FILE")"
record_command "start local RuleOps server" java -cp "$SERVER_CP" io.ruleops.server.RuleOpsServerMain 0
java -cp "$SERVER_CP" io.ruleops.server.RuleOpsServerMain 0 >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
server_url=""
for _ in $(seq 1 50); do
  server_url="$(sed -n 's/^ruleops-server=//p' "$SERVER_LOG" | head -1)"
  [ -n "$server_url" ] && break
  sleep 0.1
done
if [ -z "$server_url" ]; then
  cat "$SERVER_LOG" >&2
  cat "$COMMAND_LOG" >&2
  exit 1
fi
printf 'server_url=%s\n' "$server_url" >>"$COMMAND_LOG"

cat > "$OUT_DIR/finding.json" <<'EOF'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "submit-check-finding",
  "ruleId": "corp.java.submit-check",
  "fingerprint": "submit-check-fp",
  "message": "submission transport sample",
  "location": {"path": "src/Main.java", "line": 3, "column": 1}
}
EOF
finding_digest="$(sha256sum "$OUT_DIR/finding.json" | awk '{print $1}')"
run_logged "create feedback draft" "$RF" feedback draft "$OUT_DIR/finding.json" \
  --classification true-fp --reason "transport integration" --actor submit-check \
  --output "$OUT_DIR/feedback.json"
run_logged "submit feedback" "$RF" feedback submit "$OUT_DIR/feedback.json" --server "$server_url" \
  --output "$OUT_DIR/feedback-receipt.json"
run_logged "assert feedback submitted receipt" rg -q '"status": "SUBMITTED"' "$OUT_DIR/feedback-receipt.json"
run_logged "assert feedback received-only receipt" rg -q '"governanceOutcome": "RECEIVED_ONLY"' "$OUT_DIR/feedback-receipt.json"
run_logged "assert feedback cannot approve" rg -q '"exceptionApproved": false' "$OUT_DIR/feedback-receipt.json"
run_logged "replay feedback" "$RF" feedback submit "$OUT_DIR/feedback.json" --server "$server_url" \
  --output "$OUT_DIR/feedback-replay.json"
run_logged "assert feedback replay receipt" rg -q '"status": "REPLAYED"' "$OUT_DIR/feedback-replay.json"

run_logged "create exception request" "$RF" exception request "$OUT_DIR/finding.json" \
  --scope 'src/**' --reason "transport integration" --actor submit-check \
  --output "$OUT_DIR/exception.json"
run_logged "submit exception" "$RF" exception submit "$OUT_DIR/exception.json" --server "$server_url" \
  --output "$OUT_DIR/exception-receipt.json"
run_logged "assert exception resource type" rg -q '"resourceType": "exceptions"' "$OUT_DIR/exception-receipt.json"
run_logged "assert exception remains requested" rg -q '"resourceState": "REQUESTED"' "$OUT_DIR/exception-receipt.json"

run_captured "Rust Thin Client transport tests" "$TEST_LOG" bash -c "cd \"$REPO_ROOT\" && cargo test -p erf-thin-client transport::tests"
for fixture in \
  'only_4xx_responses_are_known_rejections' \
  'missing_response_identity_is_unknown_even_for_4xx' \
  'malformed_draft_and_server_rejection_fail_closed' \
  'exception_submission_uses_exception_path_and_replay_status'; do
  rg -F -q -- "$fixture ... ok" "$TEST_LOG" || {
    cat "$TEST_LOG" >&2
    exit 1
  }
done
if rg -q 'test result:.*[1-9][0-9]* ignored' "$TEST_LOG"; then
  cat "$TEST_LOG" >&2
  exit 1
fi

run_captured "Java RuleOps HTTP server tests" "$SERVER_TEST_LOG" bash -c "cd \"$REPO_ROOT\" && mvn -q -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest test"

cat > "$OUT_DIR/thin-client-submit.md" <<EOF
# Thin Client Server Submission Evidence

- evidenceSchema: feature-evidence-v1
- feature: thin-client-server-submission
- status: PASS
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/thin-client-v1.yaml (sha256:$contract_digest)
  - decision: docs/adr/0152-thin-client-known-4xx-classification.md (sha256:$adr_digest)
  - client: runtime/thin-client/src/transport.rs (sha256:$client_digest)
  - server: control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java (sha256:$server_digest)
  - serverTests: control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java (sha256:$server_test_digest)
  - dispatcher: rf (sha256:$rf_digest)
  - findingFixture: $OUT_DIR/finding.json (sha256:$finding_digest)
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - execution: actual command lines and assertion output are recorded in $COMMAND_LOG
  - rust: cd $REPO_ROOT && cargo test -p erf-thin-client transport::tests
  - java: cd $REPO_ROOT && mvn -q -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest test
- logs:
  - static: $STATIC_LOG
  - execution: $COMMAND_LOG
  - server: $SERVER_LOG
  - build: $BUILD_LOG
  - rust: $TEST_LOG
  - java: $SERVER_TEST_LOG

| Check | Evidence | Status |
| --- | --- | --- |
| Closed contract, classification, identity and authority symbols exist | contract/ADR/client/server rg assertions | PASS |
| Feedback submit, replay and exception submit produce only received/requested receipts | local Java server + rf commands in $COMMAND_LOG | PASS |
| Only HTTP 400-499 is SUBMISSION_REJECTED; 3xx/5xx/missing identity/malformed response is SUBMISSION_UNKNOWN | $TEST_LOG | PASS |

- keyNegativeFixtures: only_4xx_responses_are_known_rejections; missing_response_identity_is_unknown_even_for_4xx; malformed_draft_and_server_rejection_fail_closed
- receipt: RECEIVED_ONLY; no approval, Gate mutation, or SCM publication
- retryAutomatically: false; unknown outcomes do not produce a success receipt
- local Java Server integration: feedback submitted/replayed and exception submitted
- Rust transport tests: success, replay receipt, closed artifact, known 4xx rejection, 3xx/5xx unknown, missing/empty identity, and malformed response
- Java Server tests: schema, privacy, identity, actor/reason, and replay validation
EOF
echo "thin-client-submit=PASS evidence=$OUT_DIR/thin-client-submit.md"
