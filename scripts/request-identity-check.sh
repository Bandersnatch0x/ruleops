#!/usr/bin/env bash
set -euo pipefail

# requestId 冻结算法（#121 / ADR-0165）跨语言一致性证据。
# requestId = sha256(canonical(envelope))，envelope = 请求 payload 去除派生自字段
# {requestId, digest}；canonicalization 为递归 key 字典序排序，schemaVersion 参与摘要。
# 本脚本给出 EXECUTABLE 级证据：
#   1) contract / ADR / Rust 客户端 / Java 服务端静态锚点均在场；
#   2) Rust requestId 单元测试通过；
#   3) 独立第三实现（jq + sha256sum）复算 Rust 产出的 requestId 逐字节相等；
#   4) 真实 Rust→Java 往返：Rust 生成的 requestId 被 Java 独立复算接受（received-only）；
#   5) 任意格式正确但与 envelope 不匹配的 requestId 被 Client（from_json）与 Server
#      （HTTP 400 REQUEST_ID_MISMATCH）双侧 fail-closed 拒绝。

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/request-identity-check}"
CONTRACT="$REPO_ROOT/contracts/http/thin-client-v1.yaml"
ADR="$REPO_ROOT/docs/adr/0165-freeze-governance-request-id-content-addressing.md"
CLIENT="$REPO_ROOT/runtime/thin-client/src/lib.rs"
SERVER="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java"
SERVER_TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java"
RF="$REPO_ROOT/rf"
mkdir -p "$OUT_DIR"
cd "$REPO_ROOT"

STATIC_LOG="$OUT_DIR/static-check.log"
COMMAND_LOG="$OUT_DIR/command.log"
SERVER_LOG="$OUT_DIR/server.log"
TEST_LOG="$OUT_DIR/rust-request-id-tests.log"
BUILD_LOG="$OUT_DIR/server-build.log"
: >"$STATIC_LOG"
: >"$COMMAND_LOG"
static_failures=()

record_input() {
  [ -s "$1" ] || static_failures+=("missing input file: $1")
}
digest_or_missing() {
  if [ -s "$1" ]; then sha256sum "$1" | awk '{print $1}'; else printf 'missing'; fi
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

check_static() {
  local needle="$1"; shift
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

# Contract freezes the canonical envelope, its formula and the mismatch code.
check_static 'canonicalEnvelope:' "$CONTRACT"
check_static 'definition: requestPayloadWithoutDerivedFields' "$CONTRACT"
check_static 'canonicalization: recursiveLexicographicKeySort' "$CONTRACT"
check_static 'versionParticipates: true' "$CONTRACT"
check_static 'formula: "requestId = sha256(canonical(envelope))"' "$CONTRACT"
check_static 'crossImplementationEquality: required' "$CONTRACT"
check_static 'REQUEST_ID_MISMATCH' "$CONTRACT"
# Decision record present and dated.
check_static 'ADR-0165' "$ADR"
check_static 'canonical(envelope)' "$ADR"
# Rust client implements the freeze and rejects mismatches fail-closed.
check_static 'fn request_envelope_digest' "$CLIENT"
check_static 'RequestIdMismatch' "$CLIENT"
# Java server independently recomputes and rejects mismatches with the same code.
check_static 'requestEnvelopeDigest' "$SERVER"
check_static 'REQUEST_ID_MISMATCH' "$SERVER"
check_static 'forgedRequestIdIsRejectedFailClosed' "$SERVER_TEST"

if ((${#static_failures[@]} > 0)); then
  printf 'staticFailure=%s\n' "${static_failures[*]}" >>"$STATIC_LOG"
  cat "$STATIC_LOG" >&2
  exit 1
fi
printf '%s\n' 'static assertions complete: all passed' >>"$STATIC_LOG"

record_command() {
  local label="$1"; shift
  { printf '## %s\n$' "$label"; printf ' %q' "$@"; printf '\n'; } >>"$COMMAND_LOG"
}
run_logged() {
  local label="$1"; shift
  record_command "$label" "$@"
  if "$@" >>"$COMMAND_LOG" 2>&1; then return 0; fi
  cat "$COMMAND_LOG" >&2
  return 1
}
run_captured() {
  local label="$1" output="$2"; shift 2
  record_command "$label" "$@"
  if "$@" >"$output" 2>&1; then
    cat "$output" >>"$COMMAND_LOG"
    return 0
  fi
  cat "$output" >>"$COMMAND_LOG"
  cat "$output" >&2
  return 1
}

# 2) Rust requestId 单元测试：客户端复算等式、篡改拒绝、以及相同请求同 id。
run_captured "Rust request identity unit tests" "$TEST_LOG" \
  cargo test -q -p erf-thin-client --lib -- \
  request_id_is_the_canonical_envelope_digest \
  tampered_request_id_is_rejected_fail_closed \
  identical_requests_have_identical_ids_and_digests
rg -q 'test result: ok\. 3 passed; 0 failed' "$TEST_LOG" || { cat "$TEST_LOG" >&2; exit 1; }

# 独立第三实现（jq 递归 key 排序 + sha256sum）复算 envelope digest，用于交叉校验。
envelope_digest() {
  jq -cS 'del(.requestId,.digest)' "$1" | tr -d '\n' | sha256sum | awk '{print "sha256:"$1}'
}
# 按同一 canonical 约定，对去除 digest 自字段后的 payload 复算 digest 自摘要。
self_digest() {
  jq -cS 'del(.digest)' "$1" | tr -d '\n' | sha256sum | awk '{print "sha256:"$1}'
}

cat > "$OUT_DIR/finding.json" <<'EOF'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "request-identity-finding",
  "ruleId": "corp.java.request-identity",
  "fingerprint": "request-identity-fp",
  "message": "content-addressed request identity sample",
  "location": {"path": "src/Main.java", "line": 5, "column": 2}
}
EOF
finding_digest="$(sha256sum "$OUT_DIR/finding.json" | awk '{print $1}')"

# 3) Rust 生成 requestId；独立实现复算逐字节相等（feedback 与 exception 两条路径）。
run_logged "create feedback draft (Rust computes requestId)" "$RF" feedback draft "$OUT_DIR/finding.json" \
  --classification true-fp --reason "content addressing evidence" --actor request-identity \
  --output "$OUT_DIR/feedback.json"
run_logged "create exception request (Rust computes requestId)" "$RF" exception request "$OUT_DIR/finding.json" \
  --scope 'src/**' --reason "content addressing evidence" --actor request-identity \
  --output "$OUT_DIR/exception.json"

assert_independent_equality() {
  local path="$1" label="$2"
  local rust_id independent_id
  rust_id="$(jq -r .requestId "$path")"
  independent_id="$(envelope_digest "$path")"
  printf '## independent recompute (%s)\nrustRequestId=%s\nindependentRequestId=%s\n' \
    "$label" "$rust_id" "$independent_id" >>"$COMMAND_LOG"
  if [ "$rust_id" != "$independent_id" ]; then
    printf 'crossImplementationEquality FAIL for %s\n' "$label" >&2
    exit 1
  fi
}
assert_independent_equality "$OUT_DIR/feedback.json" feedback
assert_independent_equality "$OUT_DIR/exception.json" exception
feedback_request_id="$(jq -r .requestId "$OUT_DIR/feedback.json")"

# 4) 真实 Rust→Java 往返：站起本地 Java 服务端，Rust 生成的 requestId 被独立复算接受。
CP_FILE="$(mktemp)"
SERVER_PID=""
trap 'if [ -n "${SERVER_PID:-}" ]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi; rm -f "$CP_FILE"' EXIT
run_captured "Maven package and dependency classpath" "$BUILD_LOG" \
  bash -c "cd \"$REPO_ROOT\" && mvn -q -pl control-plane/server -am package -DskipTests dependency:build-classpath -Dmdep.outputFile=\"$CP_FILE\""
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
  cat "$SERVER_LOG" >&2; cat "$COMMAND_LOG" >&2; exit 1
fi
printf 'server_url=%s\n' "$server_url" >>"$COMMAND_LOG"

# Rust 生成的 requestId 被 Java 独立复算接受（received-only，绝非批准/Gate）。
run_logged "submit Rust-generated feedback (server independently recomputes requestId)" \
  "$RF" feedback submit "$OUT_DIR/feedback.json" --server "$server_url" \
  --output "$OUT_DIR/feedback-receipt.json"
run_logged "assert received-only receipt" rg -q '"governanceOutcome": "RECEIVED_ONLY"' "$OUT_DIR/feedback-receipt.json"

# 5a) Client fail-closed：篡改 requestId（并按同一约定补齐一致的 digest），from_json 拒绝。
forged_id="sha256:$(printf 'c%.0s' $(seq 1 64))"
jq --arg id "$forged_id" '.requestId=$id' "$OUT_DIR/feedback.json" > "$OUT_DIR/forged-nodigest.json"
forged_self_digest="$(self_digest "$OUT_DIR/forged-nodigest.json")"
jq --arg d "$forged_self_digest" '.digest=$d' "$OUT_DIR/forged-nodigest.json" > "$OUT_DIR/forged.json"
record_command "client rejects forged requestId (expect exit 2, no server contact)" \
  "$RF" feedback submit "$OUT_DIR/forged.json" --server "$server_url"
set +e
"$RF" feedback submit "$OUT_DIR/forged.json" --server "$server_url" >"$OUT_DIR/client-forged.log" 2>&1
client_rc=$?
set -e
cat "$OUT_DIR/client-forged.log" >>"$COMMAND_LOG"
if [ "$client_rc" -ne 2 ]; then
  echo "client accepted a forged requestId (rc=$client_rc), expected fail-closed exit 2" >&2
  cat "$OUT_DIR/client-forged.log" >&2; exit 1
fi
rg -q 'request identity does not match the canonical request envelope' "$OUT_DIR/client-forged.log" || {
  cat "$OUT_DIR/client-forged.log" >&2; exit 1;
}

# 5b) Server fail-closed：绕过客户端，直接把 forged envelope 投递到服务端，独立复算拒绝为 400 REQUEST_ID_MISMATCH。
# envelope.id 与 Idempotency-Key 均设为 forged，令 id/key 一致，隔离出 envelope-digest 复算这一道检查。
forged_payload="$(cat "$OUT_DIR/forged.json")"
forged_envelope="$(jq -cn --arg id "$forged_id" --argjson payload "$forged_payload" \
  '{id:$id, version:"feedback-draft-v1", payload:$payload}')"
record_command "server rejects forged requestId (expect HTTP 400 REQUEST_ID_MISMATCH)" \
  curl -s -o "$OUT_DIR/server-forged.json" -w '%{http_code}' -X POST "$server_url/feedback"
server_forged_status="$(curl -s -o "$OUT_DIR/server-forged.json" -w '%{http_code}' \
  -X POST "$server_url/feedback" \
  -H 'Content-Type: application/json' \
  -H "X-Request-Id: request-$forged_id" \
  -H "Idempotency-Key: $forged_id" \
  -H 'X-Actor: request-identity' \
  -H 'X-Reason: forged identity evidence' \
  --data "$forged_envelope")"
printf 'server_forged_status=%s\n' "$server_forged_status" >>"$COMMAND_LOG"
cat "$OUT_DIR/server-forged.json" >>"$COMMAND_LOG"; printf '\n' >>"$COMMAND_LOG"
if [ "$server_forged_status" != "400" ]; then
  echo "server accepted a forged requestId (status=$server_forged_status), expected 400" >&2
  cat "$OUT_DIR/server-forged.json" >&2; exit 1
fi
jq -e '.code == "REQUEST_ID_MISMATCH"' "$OUT_DIR/server-forged.json" >/dev/null \
  || { cat "$OUT_DIR/server-forged.json" >&2; exit 1; }

input_identity="$(sha256sum "$CONTRACT" "$ADR" "$CLIENT" "$SERVER" "$SERVER_TEST" \
  scripts/request-identity-check.sh "$RF" | sha256sum | awk '{print $1}')"

cat > "$OUT_DIR/request-identity.md" <<EOF
# Governance requestId 内容寻址一致性证据

- evidenceSchema: feature-evidence-v1
- feature: governance-request-id-content-addressing
- status: PASS
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- decision: docs/adr/0165-freeze-governance-request-id-content-addressing.md
- contract: contracts/http/thin-client-v1.yaml (canonicalEnvelope)
- formula: requestId = sha256(canonical(payload 去除 {requestId, digest}))
- evidenceLevel: EXECUTABLE
- inputIdentity: sha256:$input_identity
- inputs:
  - contract: contracts/http/thin-client-v1.yaml (sha256:$contract_digest)
  - decision: docs/adr/0165-freeze-governance-request-id-content-addressing.md (sha256:$adr_digest)
  - client: runtime/thin-client/src/lib.rs (sha256:$client_digest)
  - server: control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java (sha256:$server_digest)
  - serverTests: control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java (sha256:$server_test_digest)
  - dispatcher: rf (sha256:$rf_digest)
  - findingFixture: $OUT_DIR/finding.json (sha256:$finding_digest)
- feedbackRequestId: $feedback_request_id
- logs:
  - static: $STATIC_LOG
  - execution: $COMMAND_LOG
  - rust: $TEST_LOG
  - server: $SERVER_LOG
  - build: $BUILD_LOG

| 验收点 | 证据 | 结果 |
| --- | --- | --- |
| contract 冻结 canonicalization、字段顺序、版本参与与 SHA-256 计算式 | contract/ADR rg 断言 | PASS |
| Client 复算 requestId = sha256(canonical(envelope))，篡改即拒绝 | Rust 单元测试 $TEST_LOG | PASS |
| 独立第三实现（jq+sha256sum）复算 requestId 与 Rust 逐字节相等 | feedback/exception 交叉校验 | PASS |
| Server 独立复算并接受 Rust 生成的 requestId（received-only，非批准/Gate） | 本地 Java 服务端往返 $COMMAND_LOG | PASS |
| 格式正确但不匹配的 requestId 被 Client fail-closed 拒绝（exit 2） | client-forged.log | PASS |
| 格式正确但不匹配的 requestId 被 Server fail-closed 拒绝（HTTP 400 REQUEST_ID_MISMATCH） | server-forged.json | PASS |

- crossImplementation: Rust 客户端、Java 服务端、jq 参考实现三者对同一 envelope 产生同一 requestId。
- failClosed: 篡改的 requestId 在客户端 from_json 与服务端 envelope 复算两道独立检查上均被拒绝。
- receipt: RECEIVED_ONLY；requestId 同时充当 envelope.id 与 Idempotency-Key；replay/identity 冲突保持 SUBMISSION_IDENTITY_CONFLICT。
EOF

echo "request-identity=PASS evidence=$OUT_DIR/request-identity.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
