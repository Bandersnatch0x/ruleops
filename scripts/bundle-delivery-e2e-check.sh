#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/bundle-delivery-e2e-check}"
CONTRACT="$REPO_ROOT/contracts/http/rule-bundle-delivery-v1.yaml"
API_CONTRACT="$REPO_ROOT/contracts/http/ruleops-api-v1.yaml"
CATALOG="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleBundleDeliveryCatalog.java"
SERVER="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java"
SERVER_TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java"
CLIENT="$REPO_ROOT/runtime/bundle-delivery/src/lib.rs"
RUNNER="$REPO_ROOT/runtime/rf-runner/src/main.rs"
RUNNER_EXAMPLE="$REPO_ROOT/runtime/rf-runner/examples/bundle_delivery_scan.rs"
CAPACITY_DOC="$REPO_ROOT/docs/pilot/bundle-delivery-capacity-profile.md"
mkdir -p "$OUT_DIR"
STATIC_LOG="$OUT_DIR/static-check.log"
RUST_LOG="$OUT_DIR/rust-delivery-tests.log"
RUNNER_LOG="$OUT_DIR/runner-delivery-scan.log"
JAVA_LOG="$OUT_DIR/java-delivery-tests.log"
JAVA_REPORT="$REPO_ROOT/control-plane/server/target/surefire-reports/TEST-io.ruleops.server.RuleOpsHttpServerTest.xml"
JAVA_SENTINEL="$OUT_DIR/maven-started"
static_failures=()

record_input() {
  local path="$1"
  [ -s "$path" ] || static_failures+=("missing input file: $path")
}

for input in "$CONTRACT" "$API_CONTRACT" "$CATALOG" "$SERVER" "$SERVER_TEST" "$CLIENT" "$RUNNER" "$RUNNER_EXAMPLE" "$CAPACITY_DOC"; do
  record_input "$input"
done
: >"$STATIC_LOG"

check_static() {
  local needle="$1"
  shift
  printf '$ rg -F -n -- %q %s\n' "$needle" "$*" >>"$STATIC_LOG"
  if rg -F -n -- "$needle" "$@" >>"$STATIC_LOG" 2>&1; then
    printf 'status=PASS\n' >>"$STATIC_LOG"
  else
    static_failures+=("missing [$needle] in [$*]")
    printf 'status=BLOCKED\n' >>"$STATIC_LOG"
  fi
}

for needle in \
  'path: /v1/rule-bundles/{bundleDigest}/delivery' \
  'boundedResponseBytes: 180005548' \
  'sourceUpload: false' \
  'fullFindingUpload: false' \
  'encoding: bundleBytesBase64' \
  'MAX_BUNDLE_BYTES = 128 * 1024 * 1024' \
  'ConcurrentHashMap' \
  'putIfAbsent' \
  'ThreadPoolExecutor(4, 32' \
  'ArrayBlockingQueue<>(256)' \
  'MAX_RESPONSE_BYTES' \
  'SingleFlightDelivery' \
  'OnceLock' \
  'bundle_delivery_identity' \
  'ERF_RULE_BUNDLE_DIGEST' \
  'delivered bundle bytes do not match' \
  'delivery-scan=PASS'; do
  check_static "$needle" "$CONTRACT" "$API_CONTRACT" "$CATALOG" "$SERVER" "$CLIENT" "$RUNNER" "$RUNNER_EXAMPLE" "$CAPACITY_DOC"
done

write_report() {
  local status="$1"
  local rust_result="$2"
  local java_result="$3"
  local capacity="$4"
  cat >"$OUT_DIR/bundle-delivery-e2e.md" <<EOF
# RuleBundle Server→Runner Delivery Evidence

- evidenceSchema: feature-evidence-v1
- feature: server-runner-bundle-delivery
- status: $status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- capacityProfile: docs/pilot/bundle-delivery-capacity-profile.md
- commands:
  - rust: cargo test -p erf-bundle-delivery --all-targets
  - runner: cargo run -p rf-runner --example bundle_delivery_scan -- target/debug/erf-fake-host target/debug/erf-fake-resolver
  - java: mvn -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest -Dsurefire.failIfNoSpecifiedTests=false test
- logs:
  - static: $STATIC_LOG
  - rust: $RUST_LOG
  - runner: $RUNNER_LOG
  - java: $JAVA_LOG

| Boundary | Evidence | Status |
| --- | --- | --- |
| Versioned Server response, bounded read path, no source/full Finding | contract + source assertions | $([ "${#static_failures[@]}" -eq 0 ] && echo PASS || echo BLOCKED) |
| Cold/warm/restart/offline/fail-closed/single-flight Runner resolution | Rust tests | $rust_result |
| Verified Server payload enters an actual rf-runner scan | $RUNNER_LOG | ${runner_result:-not-run} |
| HTTP response, immutability, concurrent reads and latency percentiles | Java tests | $java_result |

- capacityObserved: $capacity
- staticResult: ${static_failures[*]:-all assertions passed}
- privacy: delivery carries signed Bundle bytes only; no source upload, Finding full text or SCM credential
- authority: delivery/cache success is not Gate or governance authority
EOF
}

if ((${#static_failures[@]} > 0)); then
  write_report BLOCKED not-run not-run not-collected
  cat "$STATIC_LOG" >&2
  exit 1
fi

if (cd "$REPO_ROOT" && cargo test --package erf-bundle-delivery --all-targets) >"$RUST_LOG" 2>&1; then
  rust_result=PASS
else
  write_report BLOCKED BLOCKED not-run not-collected
  cat "$RUST_LOG" >&2
  exit 1
fi
for fixture in \
  'connected_cold_fetch_then_warm_restart_hit_is_deterministic' \
  'offline_hit_reuses_only_verified_bytes_and_missing_is_inconclusive' \
  'response_tamper_signature_expiry_and_descriptor_mismatch_fail_closed' \
  'slow_network_returns_a_bounded_transport_error' \
  'different_digest_cold_fetches_keep_independent_flights' \
  'same_digest_cold_fetch_is_single_flight'; do
  rg -F -q -- "$fixture ... ok" "$RUST_LOG" || {
    write_report BLOCKED BLOCKED not-run not-collected
    cat "$RUST_LOG" >&2
    exit 1
  }
done
if rg -q 'test result:.*[1-9][0-9]* ignored' "$RUST_LOG"; then
  write_report BLOCKED BLOCKED not-run not-collected
  cat "$RUST_LOG" >&2
  exit 1
fi

if (cd "$REPO_ROOT" && cargo build --quiet -p erf-fake-host -p erf-fake-resolver \
  && cargo run --quiet -p rf-runner --example bundle_delivery_scan -- \
      "$REPO_ROOT/target/debug/erf-fake-host" "$REPO_ROOT/target/debug/erf-fake-resolver") >"$RUNNER_LOG" 2>&1; then
  rg -F -q -- 'delivery-scan=PASS' "$RUNNER_LOG" || {
    runner_result=BLOCKED
    write_report BLOCKED PASS not-run not-collected
    cat "$RUNNER_LOG" >&2
    exit 1
  }
  runner_result=PASS
else
  runner_result=BLOCKED
  write_report BLOCKED PASS not-run not-collected
  cat "$RUNNER_LOG" >&2
  exit 1
fi

: >"$JAVA_SENTINEL"
if (cd "$REPO_ROOT" && mvn -q -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest -Dsurefire.failIfNoSpecifiedTests=false test) >"$JAVA_LOG" 2>&1; then
  java_result=PASS
else
  write_report BLOCKED PASS BLOCKED not-collected
  cat "$JAVA_LOG" >&2
  exit 1
fi
if [ ! -s "$JAVA_REPORT" ] || [ ! "$JAVA_REPORT" -nt "$JAVA_SENTINEL" ]; then
  write_report BLOCKED PASS BLOCKED not-collected
  cat "$JAVA_LOG" >&2
  exit 1
fi
tests_run="$(sed -n 's/.*tests="\([0-9][0-9]*\)".*/\1/p' "$JAVA_REPORT" | head -1)"
skipped="$(sed -n 's/.*skipped="\([0-9][0-9]*\)".*/\1/p' "$JAVA_REPORT" | head -1)"
if ! [[ "$tests_run" =~ ^[0-9]+$ ]] || [ "$tests_run" -eq 0 ] || [ "$skipped" != "0" ] || rg -q '<skipped' "$JAVA_REPORT"; then
  write_report BLOCKED PASS BLOCKED not-collected
  cat "$JAVA_LOG" >&2
  exit 1
fi
for fixture in \
  'ruleBundleDeliveryIsVersionedImmutableAndIndependentFromGovernanceStore' \
  'concurrentDeliveryReadsShareImmutableBytesWithoutTakingGovernanceJournalPath' \
  'deliveryCapacityProfileCapturesLatencyPercentilesWithBoundedReadPool'; do
  rg -F -q -- "name=\"$fixture\"" "$JAVA_REPORT" || {
    write_report BLOCKED PASS BLOCKED not-collected
    cat "$JAVA_LOG" >&2
    exit 1
  }
done
capacity="$(rg -m1 'DELIVERY_CAPACITY_PROFILE' "$JAVA_LOG" "$JAVA_REPORT" || true)"
[ -n "$capacity" ] || {
  write_report BLOCKED PASS BLOCKED not-collected
  cat "$JAVA_LOG" >&2
  exit 1
}

write_report PASS PASS "PASS (tests=$tests_run)" "$capacity"
echo "bundle-delivery-e2e=PASS evidence=$OUT_DIR/bundle-delivery-e2e.md capacity=$capacity evidenceLevel=EXECUTABLE"
