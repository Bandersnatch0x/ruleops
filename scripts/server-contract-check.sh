#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/server-contract-check}"
CONTRACT="$REPO_ROOT/contracts/http/ruleops-api-v1.yaml"
SERVER="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java"
STORE="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java"
SERVER_TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java"
mkdir -p "$OUT_DIR"
TEST_LOG="$OUT_DIR/server-contract-tests.log"
STATIC_LOG="$OUT_DIR/static-check.log"
TEST_REPORT="$REPO_ROOT/control-plane/server/target/surefire-reports/TEST-io.ruleops.server.RuleOpsHttpServerTest.xml"
TEST_SENTINEL="$OUT_DIR/maven-started"
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
record_input "$SERVER"
record_input "$STORE"
record_input "$SERVER_TEST"
contract_digest="$(digest_or_missing "$CONTRACT")"
server_digest="$(digest_or_missing "$SERVER")"
store_digest="$(digest_or_missing "$STORE")"
test_digest="$(digest_or_missing "$SERVER_TEST")"
static_status=STATIC_ONLY
behavior_status=BLOCKED
behavior_result="not run"
test_command="(cd $REPO_ROOT && mvn -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest -Dsurefire.failIfNoSpecifiedTests=false test)"
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
  'apiVersion: rulefactory.io/v1'
  'acceptsSource: false'
  'acceptsFullFinding: false'
  'Idempotency-Key'
  'incompleteNeverMapsToSuccess: true'
  'path: /v1/registry/rules'
  'path: /v1/registry/standards/{id}'
  'schemaVersion: asset-registry-v1'
  'resourceFields: [type, id, version, state, digest]'
  'path: /v1/standards'
  'path: /v1/coverage/analyze'
  'path: /v1/rules/{id}/generate'
  'path: /v1/releases/{id}/promote'
  'path: /v1/feedback'
  'path: /v1/exceptions'
  'path: /v1/audit'
  'transitionRequest:'
  'requiredFields: [version]'
  'missingVersionCode: RESOURCE_VERSION_REQUIRED'
  'exactLookup: [type, id, version]'
)
for needle in "${required[@]}"; do
  check_static "$needle" "$CONTRACT"
done
for needle in 'VersionQualifiedLifecycleTransition' 'requireTransitionIdentity' 'RESOURCE_VERSION_REQUIRED' \
  'validateGovernanceRequest' 'REQUEST_ID_MISMATCH' 'AUDIT_CONTEXT_MISMATCH' \
  'lifecycleTransitionRequiresAnExplicitVersionAndTargetsOnlyThatImmutableRevision' \
  'lifecycleTransitionOperationIdCannotBeReusedAcrossVersions' \
  'lifecycleTransitionRejectsMalformedVersionRequestBody' \
  'replayAfterALaterTransitionReturnsTheOriginalVersionQualifiedResult' \
  'registryReadsAreClosedDigestOnlyVersionedAndReadOnly' \
  'registryRejectsWritesUnknownResourcesAndInvalidSelectors' \
  'registryReadsUseTheRecoveredJournalWithoutAppending' \
  'thinClientGovernanceRequestsAreClosedDigestOnlyAndReplayable'; do
  check_static "$needle" "$SERVER" "$STORE" "$SERVER_TEST"
done

write_report() {
  cat > "$OUT_DIR/server-contract.md" <<EOF
# RuleOps Server API Contract Evidence

- evidenceSchema: feature-evidence-v1
- feature: ruleops-server-api-contract
- status: $behavior_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - contract: contracts/http/ruleops-api-v1.yaml (sha256:$contract_digest)
  - implementation: control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java (sha256:$server_digest)
  - store: control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java (sha256:$store_digest)
  - tests: control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java (sha256:$test_digest)
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - behavior: $test_command
- logs:
  - static: $STATIC_LOG
  - behavior: $TEST_LOG
- testReport: $TEST_REPORT

| Check | Evidence | Status |
| --- | --- | --- |
| API paths, privacy, version-qualified lifecycle symbols exist | contract + server/test rg assertions | $static_status |
| HTTP lifecycle, idempotency, version targeting, privacy, audit and governance negative cases | $test_command | $behavior_status |

- staticResult: ${static_failures[*]:-all assertions passed}
- behaviorResult: $behavior_result
- keyNegativeFixtures: conflictingIdempotencyAndImmutableIdentityFailClosed; lifecycleTransitionRequiresAnExplicitVersionAndTargetsOnlyThatImmutableRevision; lifecycleTransitionOperationIdCannotBeReusedAcrossVersions; lifecycleTransitionRejectsMalformedVersionRequestBody; replayAfterALaterTransitionReturnsTheOriginalVersionQualifiedResult; privacyAndAuditHeadersAreRequired; thinClientGovernanceRequestsAreClosedDigestOnlyAndReplayable
- failureLocation: Maven/Surefire log names the failing test class/method; contract/source failures stop before the behavior command
- authority: API tests do not grant Gate, SCM or production authority
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
: >"$TEST_SENTINEL"

if (cd "$REPO_ROOT" && mvn -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest -Dsurefire.failIfNoSpecifiedTests=false test) >"$TEST_LOG" 2>&1; then
  behavior_status=PASS
  behavior_result="exit=0; $(rg -m1 'Tests run:' "$TEST_LOG" || true)"
else
  behavior_status=BLOCKED
  behavior_result="maven test failed; see $TEST_LOG"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi

if [ ! -s "$TEST_REPORT" ] || [ ! "$TEST_REPORT" -nt "$TEST_SENTINEL" ]; then
  behavior_status=BLOCKED
  behavior_result="maven reported success but no fresh Surefire report was produced: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
tests_run="$(sed -n 's/.*tests="\([0-9][0-9]*\)".*/\1/p' "$TEST_REPORT" | head -1)"
if ! [[ "$tests_run" =~ ^[0-9]+$ ]] || [ "$tests_run" -eq 0 ]; then
  behavior_status=BLOCKED
  behavior_result="fresh Surefire report ran zero tests: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
skipped="$(sed -n 's/.*skipped="\([0-9][0-9]*\)".*/\1/p' "$TEST_REPORT" | head -1)"
if [ "$skipped" != "0" ] || rg -q '<skipped' "$TEST_REPORT"; then
  behavior_status=BLOCKED
  behavior_result="fresh Surefire report contains skipped test cases: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
for fixture in \
  'conflictingIdempotencyAndImmutableIdentityFailClosed' \
  'lifecycleTransitionRequiresAnExplicitVersionAndTargetsOnlyThatImmutableRevision' \
  'lifecycleTransitionOperationIdCannotBeReusedAcrossVersions' \
  'lifecycleTransitionRejectsMalformedVersionRequestBody' \
  'replayAfterALaterTransitionReturnsTheOriginalVersionQualifiedResult' \
  'privacyAndAuditHeadersAreRequired' \
  'registryReadsAreClosedDigestOnlyVersionedAndReadOnly' \
  'registryRejectsWritesUnknownResourcesAndInvalidSelectors' \
  'registryReadsUseTheRecoveredJournalWithoutAppending(Path)' \
  'thinClientGovernanceRequestsAreClosedDigestOnlyAndReplayable'; do
  if ! rg -F -q -- "name=\"$fixture\"" "$TEST_REPORT"; then
    behavior_status=BLOCKED
    behavior_result="fresh Surefire report omitted required test fixture: $fixture"
    write_report
    cat "$TEST_LOG" >&2
    exit 1
  fi
done
behavior_result="exit=0; tests=$tests_run; required negative fixtures present in $TEST_REPORT"

write_report
echo "server-contract=PASS evidence=$OUT_DIR/server-contract.md evidenceLevel=EXECUTABLE"
