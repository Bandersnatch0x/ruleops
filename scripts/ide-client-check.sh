#!/usr/bin/env bash
set -euo pipefail

# IDE Thin Client 门禁：EXECUTABLE 证据。
# 真实运行 rf explain，跑两端纯核测试（node --test / javac+java），
# 并令两端 parity harness 消费同一份真实 rf 输出后 diff，证明跨端 byte-identical。
# 完整 VSIX/Gradle 插件构建与交互式真实 IDE 验收属人工离线步骤，不在本门禁内。

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/ide-client-check}"
RF="$REPO_ROOT/rf"
VSCODE_CORE="$REPO_ROOT/clients/vscode/src/core.js"
VSCODE_TEST="$REPO_ROOT/clients/vscode/test/core.test.js"
VSCODE_HARNESS="$REPO_ROOT/clients/vscode/test/parity-harness.js"
IJ_CORE="$REPO_ROOT/clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java"
IJ_SELFTEST="$REPO_ROOT/clients/intellij/selftest/io/ruleops/ide/core/CoreSelfTest.java"
IJ_HARNESS="$REPO_ROOT/clients/intellij/selftest/io/ruleops/ide/core/ParityHarness.java"
ADR="$REPO_ROOT/docs/adr/0166-ide-thin-client-adapter-boundary.md"
PLUGIN_XML="$REPO_ROOT/clients/intellij/src/main/resources/META-INF/plugin.xml"
VSCODE_MANIFEST="$REPO_ROOT/clients/vscode/package.json"

mkdir -p "$OUT_DIR"
NODE_LOG="$OUT_DIR/vscode-node-test.log"
JAVA_LOG="$OUT_DIR/intellij-selftest.log"
RF_LOG="$OUT_DIR/rf-explain.log"
PARITY_VSCODE="$OUT_DIR/parity-vscode.txt"
PARITY_IJ="$OUT_DIR/parity-intellij.txt"
PARITY_DIFF="$OUT_DIR/parity.diff"
FINDING="$OUT_DIR/finding.json"
CLASSES="$OUT_DIR/classes"
failures=()

digest_or_missing() {
  local path="$1"
  if [ -s "$path" ]; then sha256sum "$path" | awk '{print $1}'; else printf 'missing'; fi
}

for f in "$RF" "$VSCODE_CORE" "$VSCODE_TEST" "$VSCODE_HARNESS" "$IJ_CORE" "$IJ_SELFTEST" \
  "$IJ_HARNESS" "$ADR" "$PLUGIN_XML" "$VSCODE_MANIFEST"; do
  [ -s "$f" ] || failures+=("missing input file: $f")
done

# digest-only Finding 夹具（无源码字段）。
cat > "$FINDING" <<'JSON'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "ide-client-check-finding",
  "ruleId": "corp.java.ide-client-check",
  "fingerprint": "ide-client-check-fp",
  "message": "ide client evidence sample",
  "location": {"path": "src/Main.java", "line": 3, "column": 1}
}
JSON

status_final=PASS
node_result="not run"
java_result="not run"
rf_result="not run"
parity_result="not run"

run_or_fail() {
  # 只在前置输入齐备时执行。
  if ((${#failures[@]} > 0)); then return; fi

  # 1) 真实 rf explain（EXECUTABLE 行为证据）。
  if (cd "$REPO_ROOT" && "$RF" explain "$FINDING" --format text) >"$RF_LOG" 2>&1; then
    rf_result="exit=0"
  else
    failures+=("rf explain failed; see $RF_LOG")
    rf_result="rf explain non-zero"
    return
  fi
  grep -q '^finding_id=' "$RF_LOG" || failures+=("rf explain output missing finding_id")
  grep -q '^authority_status=' "$RF_LOG" || failures+=("rf explain output missing authority_status")

  # 2) VS Code 纯核单测（node --test，离线零依赖；显式指定测试文件，避免误收 harness）。
  if (cd "$REPO_ROOT/clients/vscode" && node --test test/core.test.js) >"$NODE_LOG" 2>&1; then
    node_result="$(grep -E '(tests|pass|fail) [0-9]' "$NODE_LOG" | paste -sd ' ' - || true)"
    grep -Eq 'fail 0$' "$NODE_LOG" || failures+=("vscode node --test reported failures")
  else
    failures+=("vscode node --test failed; see $NODE_LOG")
    node_result="node --test non-zero"
  fi

  # 3) IntelliJ 纯核自测（plain javac + java，离线零依赖，无 IDE SDK）。
  rm -rf "$CLASSES"; mkdir -p "$CLASSES"
  if javac -d "$CLASSES" "$IJ_CORE" "$IJ_SELFTEST" "$IJ_HARNESS" >"$JAVA_LOG" 2>&1; then
    if java -cp "$CLASSES" io.ruleops.ide.core.CoreSelfTest >>"$JAVA_LOG" 2>&1; then
      java_result="$(grep -E '^intellij-core-selftest:' "$JAVA_LOG" | tail -1 || true)"
      grep -Eq 'fail=0$' "$JAVA_LOG" || failures+=("intellij CoreSelfTest reported failures")
    else
      failures+=("intellij CoreSelfTest run failed; see $JAVA_LOG")
      java_result="java run non-zero"
    fi
  else
    failures+=("intellij javac failed; see $JAVA_LOG")
    java_result="javac non-zero"
  fi

  # 4) 跨端 parity：两端 harness 消费同一份真实 rf 输出，diff 必须为空。
  if [ -d "$CLASSES" ] && [ -s "$RF_LOG" ]; then
    node "$VSCODE_HARNESS" "$RF_LOG" >"$PARITY_VSCODE" 2>>"$JAVA_LOG" || failures+=("vscode parity harness failed")
    java -cp "$CLASSES" io.ruleops.ide.core.ParityHarness "$RF_LOG" >"$PARITY_IJ" 2>>"$JAVA_LOG" || failures+=("intellij parity harness failed")
    if [ -s "$PARITY_VSCODE" ] && [ -s "$PARITY_IJ" ]; then
      if diff -u "$PARITY_VSCODE" "$PARITY_IJ" >"$PARITY_DIFF" 2>&1; then
        parity_result="byte-identical across both cores ($(wc -l <"$PARITY_VSCODE" | tr -d ' ') lines)"
        # 关键负例/边界必须真的出现在规范化输出中。
        for needle in \
          'path-abs=ABSOLUTE_PATH' \
          'path-dotdot=DOT_SEGMENT' \
          'view-forbidden=FORBIDDEN_FIELD:source' \
          'view-unknown=UNKNOWN_FIELD:extra' \
          'exit3=REFUSAL_OR_PENDING' \
          'receipt-violate=scmPublished' \
          'status-unknown=failclosed' \
          'parse-authority_status=LOCAL_UNATTESTED' \
          'parse-authoritative=false'; do
          grep -Fq "$needle" "$PARITY_VSCODE" || failures+=("parity output missing required line: $needle")
        done
      else
        failures+=("cross-core parity diff non-empty; see $PARITY_DIFF")
        parity_result="diff non-empty"
      fi
    else
      failures+=("parity harness produced empty output")
    fi
  fi
}

run_or_fail

rf_digest="$(digest_or_missing "$RF")"
vscode_core_digest="$(digest_or_missing "$VSCODE_CORE")"
ij_core_digest="$(digest_or_missing "$IJ_CORE")"
adr_digest="$(digest_or_missing "$ADR")"
finding_digest="$(digest_or_missing "$FINDING")"

write_report() {
  cat > "$OUT_DIR/ide-client.md" <<EOF
# IDE Thin Client Evidence

- evidenceSchema: feature-evidence-v1
- feature: ide-thin-client-dual-core
- status: $status_final
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - dispatcher: rf (sha256:$rf_digest)
  - vscodeCore: clients/vscode/src/core.js (sha256:$vscode_core_digest)
  - intellijCore: clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java (sha256:$ij_core_digest)
  - decision: docs/adr/0166-ide-thin-client-adapter-boundary.md (sha256:$adr_digest)
  - finding: $FINDING (sha256:$finding_digest)
- commands:
  - behavior-rf: (cd $REPO_ROOT && rf explain FINDING --format text)
  - behavior-vscode: (cd $REPO_ROOT/clients/vscode && node --test)
  - behavior-intellij: javac ThinClientCore + CoreSelfTest + ParityHarness && java CoreSelfTest
  - parity: node parity-harness.js RF_OUT vs java ParityHarness RF_OUT; diff must be empty
- logs:
  - rf: $RF_LOG
  - vscode: $NODE_LOG
  - intellij: $JAVA_LOG
  - parity: $PARITY_DIFF ($PARITY_VSCODE vs $PARITY_IJ)

| Check | Evidence | Status |
| --- | --- | --- |
| Real rf explain runs on a digest-only Finding | $RF_LOG | $rf_result |
| VS Code pure core offline tests | $NODE_LOG | $node_result |
| IntelliJ pure core offline self-test (no IDE SDK) | $JAVA_LOG | $java_result |
| Both cores byte-identical on the same real rf output | $PARITY_DIFF | $parity_result |

- authority: IDE clients hold no Runtime/Server/Registry/SCM/Gate authority; receipts are RECEIVED_ONLY; false-authority guards (exceptionApproved/gateMutated/scmPublished) always false
- privacy: FORBIDDEN_FIELDS rejected recursively; FindingView closed-world; telemetry default off; submit requires explicit serverEndpoint
- pathSafety: validateRelativePath rejects absolute/parent/backslash/control; binding layer additionally confirms resolved path stays under project root
- humanStep: full VSIX (@vscode/vsce) + Gradle IntelliJ-Platform build + interactive real-IDE acceptance + plugin repo maintenance remain the offline human step
- result: ${failures[*]:-all assertions passed}
EOF
}

if ((${#failures[@]} > 0)); then
  status_final=BLOCKED
  write_report
  printf 'ideClientFailure=%s\n' "${failures[@]}" >&2
  exit 1
fi

write_report
input_identity="$(printf '%s' "$rf_digest$vscode_core_digest$ij_core_digest$finding_digest" | sha256sum | awk '{print $1}')"
echo "ide-client=PASS evidence=$OUT_DIR/ide-client.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
