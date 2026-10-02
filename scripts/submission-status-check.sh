#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/submission-status-check}"
CONTRACT="$REPO_ROOT/contracts/http/submission-status-v1.yaml"
API_CONTRACT="$REPO_ROOT/contracts/http/ruleops-api-v1.yaml"
CLIENT="$REPO_ROOT/runtime/thin-client/src/transport.rs"
CLIENT_MAIN="$REPO_ROOT/runtime/thin-client/src/main.rs"
SERVER="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java"
STORE="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java"
SERVER_TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java"
RF="$REPO_ROOT/rf"
mkdir -p "$OUT_DIR"

STATIC_LOG="$OUT_DIR/static-check.log"
COMMAND_LOG="$OUT_DIR/command.log"
SERVER_LOG="$OUT_DIR/server.log"
RUST_LOG="$OUT_DIR/rust-tests.log"
JAVA_LOG="$OUT_DIR/java-tests.log"
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

for file in "$CONTRACT" "$API_CONTRACT" "$CLIENT" "$CLIENT_MAIN" "$SERVER" "$STORE" "$SERVER_TEST" "$RF"; do
  record_input "$file"
done

contract_digest="$(digest_or_missing "$CONTRACT")"
api_contract_digest="$(digest_or_missing "$API_CONTRACT")"
client_digest="$(digest_or_missing "$CLIENT")"
client_main_digest="$(digest_or_missing "$CLIENT_MAIN")"
server_digest="$(digest_or_missing "$SERVER")"
store_digest="$(digest_or_missing "$STORE")"
server_test_digest="$(digest_or_missing "$SERVER_TEST")"
rf_digest="$(digest_or_missing "$RF")"
: >"$STATIC_LOG"
: >"$COMMAND_LOG"

check_static() {
  local needle="$1"
  shift
  if rg -F -n -- "$needle" "$@" >>"$STATIC_LOG" 2>&1; then
    printf 'PASS %s\n' "$needle" >>"$STATIC_LOG"
  else
    static_failures+=("missing [$needle] in [$*]")
    printf 'BLOCKED %s\n' "$needle" >>"$STATIC_LOG"
  fi
}

for needle in \
  'path: /v1/submissions/status' \
  'schemaVersion: submission-status-v1' \
  'statuses: [PENDING, RECEIVED_ONLY, REPLAYED, UNKNOWN]' \
  'SUBMISSION_IDENTITY_CONFLICT' \
  'automaticRetry: false' \
  'query_submission_status' \
  'SubmissionStatus' \
  'submissionStatus' \
    'pendingSubmissions' \
    'replayedSubmissions' \
    'asSubmissionStatusJson' \
    'SUBMISSION_SELECTOR_INVALID' \
    'serverRequestId' \
    '--request-id may appear only once' \
    'submissionStatusResolvesRequestOrArtifactWithoutReplayingWrites' \
    'exceptionSubmissionStatusBindsArtifactAndResourceVersion' \
    'inFlightSubmissionIsPendingAndConflictingSelectorsFailClosed'; do
  check_static "$needle" "$CONTRACT" "$API_CONTRACT" "$CLIENT" "$CLIENT_MAIN" "$SERVER" "$STORE" "$SERVER_TEST" "$RF"
done

if ((${#static_failures[@]} > 0)); then
  printf 'staticFailure=%s\n' "${static_failures[*]}" >>"$STATIC_LOG"
  cat "$STATIC_LOG" >&2
  exit 1
fi

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
  "$@" >>"$COMMAND_LOG" 2>&1
}

CP_FILE="$(mktemp)"
SERVER_PID=""
trap 'if [ -n "${SERVER_PID:-}" ]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi; rm -f "$CP_FILE"' EXIT
run_logged "Maven package and dependency classpath" bash -c "cd \"$REPO_ROOT\" && mvn -q -pl control-plane/server -am package -DskipTests dependency:build-classpath -Dmdep.outputFile=\"$CP_FILE\" -q"
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
[ -n "$server_url" ] || { cat "$SERVER_LOG" >&2; exit 1; }
printf 'server_url=%s\n' "$server_url" >>"$COMMAND_LOG"

cat >"$OUT_DIR/finding.json" <<'EOF'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "status-check-finding",
  "ruleId": "corp.java.status-check",
  "fingerprint": "status-check-fp",
  "message": "submission status recovery sample",
  "location": {"path": "src/Main.java", "line": 4, "column": 2}
}
EOF

run_logged "create feedback draft" "$RF" feedback draft "$OUT_DIR/finding.json" \
  --classification true-fp --reason "status recovery" --actor status-check \
  --output "$OUT_DIR/feedback.json"
run_logged "submit feedback" "$RF" feedback submit "$OUT_DIR/feedback.json" --server "$server_url" \
  --output "$OUT_DIR/feedback-receipt.json"
request_id="$(jq -r '.requestId' "$OUT_DIR/feedback.json")"
artifact_digest="$(jq -r '.digest' "$OUT_DIR/feedback.json")"

run_logged "query feedback by request id" "$RF" submission status --server "$server_url" \
  --request-id "$request_id" --output "$OUT_DIR/status-received.json"
rg -q '"status": "RECEIVED_ONLY"' "$OUT_DIR/status-received.json"
rg -q '"artifactDigest": "sha256:' "$OUT_DIR/status-received.json"
rg -q '"authority": "RECEIVED_ONLY"' "$OUT_DIR/status-received.json"
rg -q '"auditContext"' "$OUT_DIR/status-received.json"
if jq -e '.resource | has("payload")' "$OUT_DIR/status-received.json" >/dev/null; then
  echo "received status unexpectedly exposed a resource payload" >&2
  exit 1
fi

run_logged "query feedback by artifact digest" "$RF" submission status --server "$server_url" \
  --artifact-digest "$artifact_digest" --output "$OUT_DIR/status-artifact.json"
rg -q '"requestId": "sha256:' "$OUT_DIR/status-artifact.json"
rg -q '"resourceType": "feedback"' "$OUT_DIR/status-artifact.json"

run_logged "replay feedback write" "$RF" feedback submit "$OUT_DIR/feedback.json" --server "$server_url" \
  --output "$OUT_DIR/feedback-replay.json"
rg -q '"status": "REPLAYED"' "$OUT_DIR/feedback-replay.json"
run_logged "query replayed feedback" "$RF" submission status --server "$server_url" \
  --request-id "$request_id" --output "$OUT_DIR/status-replayed.json"
rg -q '"status": "REPLAYED"' "$OUT_DIR/status-replayed.json"
rg -q '"automaticRetry": false' "$OUT_DIR/status-replayed.json"

unknown_id="sha256:$(printf '0%.0s' {1..64})"
set +e
"$RF" submission status --server "$server_url" --request-id "$unknown_id" \
  --output "$OUT_DIR/status-unknown.json" >"$OUT_DIR/status-unknown.console" 2>&1
unknown_status=$?
set -e
[ "$unknown_status" -eq 3 ]
rg -q '"status": "UNKNOWN"' "$OUT_DIR/status-unknown.json"
rg -q '"resource": null' "$OUT_DIR/status-unknown.json"
rg -q '"automaticRetry": false' "$OUT_DIR/status-unknown.json"
if jq -e '.resource | has("payload")' "$OUT_DIR/status-unknown.json" >/dev/null 2>&1; then
  echo "unknown status unexpectedly exposed a resource payload" >&2
  exit 1
fi

set +e
"$RF" submission status --server "$server_url" --request-id "$unknown_id" \
  --request-id "$unknown_id" >"$OUT_DIR/status-duplicate-cli.console" 2>&1
duplicate_cli_status=$?
set -e
[ "$duplicate_cli_status" -eq 2 ]
rg -q -- '--request-id may appear only once' "$OUT_DIR/status-duplicate-cli.console"

conflict_digest="sha256:$(printf '1%.0s' {1..64})"
set +e
"$RF" submission status --server "$server_url" --request-id "$request_id" \
  --artifact-digest "$conflict_digest" >"$OUT_DIR/status-conflict.console" 2>&1
conflict_status=$?
set -e
[ "$conflict_status" -eq 2 ]
rg -q 'SUBMISSION_IDENTITY_CONFLICT' "$OUT_DIR/status-conflict.console"

run_logged "create exception request" "$RF" exception request "$OUT_DIR/finding.json" \
  --scope 'src/**' --reason "status recovery" --actor status-check \
  --output "$OUT_DIR/exception.json"
run_logged "submit exception" "$RF" exception submit "$OUT_DIR/exception.json" --server "$server_url" \
  --output "$OUT_DIR/exception-receipt.json"
exception_artifact="$(jq -r '.digest' "$OUT_DIR/exception.json")"
run_logged "query exception by artifact digest" "$RF" submission status --server "$server_url" \
  --artifact-digest "$exception_artifact" --output "$OUT_DIR/status-exception.json"
rg -q '"status": "RECEIVED_ONLY"' "$OUT_DIR/status-exception.json"
rg -q '"resourceType": "exceptions"' "$OUT_DIR/status-exception.json"
rg -q '"resourceVersion": "exception-request-v1"' "$OUT_DIR/status-exception.json"
if jq -e '.resource | has("payload")' "$OUT_DIR/status-exception.json" >/dev/null; then
  echo "exception status unexpectedly exposed a resource payload" >&2
  exit 1
fi

record_command "Rust transport status tests" bash -c "cd \"$REPO_ROOT\" && cargo test -q -p erf-thin-client transport::tests"
(cd "$REPO_ROOT" && cargo test -q -p erf-thin-client transport::tests) >"$RUST_LOG" 2>&1
rg -q '[1-9][0-9]* passed; 0 failed; 0 ignored' "$RUST_LOG"

JAVA_REPORT="$REPO_ROOT/control-plane/server/target/surefire-reports/TEST-io.ruleops.server.RuleOpsHttpServerTest.xml"
rm -f "$JAVA_REPORT"
record_command "Maven RuleOpsHttpServerTest" bash -c "cd \"$REPO_ROOT\" && mvn -q -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest test"
(cd "$REPO_ROOT" && mvn -q -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest test) >"$JAVA_LOG" 2>&1
test -s "$JAVA_REPORT"
rg -q 'tests="[1-9][0-9]*" errors="0" skipped="0" failures="0"' "$JAVA_REPORT"
rg -q 'Rust transport status tests' "$COMMAND_LOG"
rg -q 'Maven RuleOpsHttpServerTest' "$COMMAND_LOG"

cat >"$OUT_DIR/submission-status.md" <<EOF
# Submission Status Recovery Evidence

- evidenceSchema: feature-evidence-v1
- feature: unknown-submission-status-recovery
- status: PASS
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/submission-status-v1.yaml (sha256:$contract_digest)
  - apiContract: contracts/http/ruleops-api-v1.yaml (sha256:$api_contract_digest)
  - transport: runtime/thin-client/src/transport.rs (sha256:$client_digest)
  - clientMain: runtime/thin-client/src/main.rs (sha256:$client_main_digest)
  - server: control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java (sha256:$server_digest)
  - store: control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java (sha256:$store_digest)
  - serverTests: control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java (sha256:$server_test_digest)
  - dispatcher: rf (sha256:$rf_digest)
- selectors: requestId and artifactDigest
- statuses: PENDING, RECEIVED_ONLY, REPLAYED, UNKNOWN
- verified:
  - feedback request recovered by request id and artifact digest
  - duplicate write returned REPLAYED and status query preserved received-only authority
  - blocked in-flight journal admission returned PENDING with UNKNOWN authority
  - unknown request returned UNKNOWN, no resource/audit, exit code 3 and no automatic retry
  - mismatched selectors returned SUBMISSION_IDENTITY_CONFLICT and exit code 2
  - duplicate and malformed selectors were rejected without a lookup
  - duplicate CLI selectors were rejected before transport
  - exception request recovered by artifact digest with resource/version/audit identity
  - status resource projections omitted arbitrary payload fields
  - Rust transport and Java HTTP Server tests passed without skipped cases
- logs:
  - static: $STATIC_LOG
  - command: $COMMAND_LOG
  - server: $SERVER_LOG
  - rust: $RUST_LOG
  - java: $JAVA_LOG
EOF

echo "submission-status=PASS evidence=$OUT_DIR/submission-status.md"
