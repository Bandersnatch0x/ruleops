#!/usr/bin/env bash
# Shell regression for evidence-claim linter PASS/WARN/FAIL semantics.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTER="$ROOT/scripts/evidence-claim-lint.sh"
FIXTURES="$ROOT/docs/research/evidence-claim-fixtures"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

run_expect() {
  local expected="$1" fixture="$2" actual output rc fixture_path
  if [[ "$fixture" = /* ]]; then
    fixture_path="$fixture"
  else
    fixture_path="$FIXTURES/$fixture"
  fi
  set +e
  output="$($LINTER "$fixture_path" 2>&1)"
  rc=$?
  set -e
  actual="$(printf '%s\n' "$output" | sed -n 's/^- status: //p')"
  if [ "$actual" != "$expected" ]; then
    printf 'fixture=%s expected=%s actual=%s rc=%s\n%s\n' "$fixture" "$expected" "$actual" "$rc" "$output" >&2
    exit 1
  fi
  case "$expected" in
    PASS) [ "$rc" -eq 0 ] || { echo "PASS fixture returned $rc" >&2; exit 1; } ;;
    WARN) [ "$rc" -eq 2 ] || { echo "WARN fixture returned $rc" >&2; exit 1; } ;;
    FAIL) [ "$rc" -eq 1 ] || { echo "FAIL fixture returned $rc" >&2; exit 1; } ;;
  esac
}

run_expect PASS pass.json
run_expect WARN warn.json
run_expect FAIL fail.json

# Markdown markers are supported as well; the command remains executable.
cat >"$TMP/audit.md" <<'EOF'
# Audit note
<!-- evidence-claim {"id":"md-1","text":"marker claim","source":"docs/research/audit.md:7","command":"python3 -c 'print(1)'","evidenceLevel":"EXECUTABLE","commitSha":"0123456789abcdef0123456789abcdef01234567","blocker":"none","consumer":"reviewer"} -->
EOF
run_expect PASS "$TMP/audit.md"

echo "evidence-claim-lint-test=PASS"
