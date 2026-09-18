#!/usr/bin/env bash
# Reproducible Phase 4 RuleRelease Registry lifecycle evidence.
# Usage: scripts/phase4-release-registry.sh OUT_DIR
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-}"
[ -n "$OUT" ] || { echo "usage: $0 OUT_DIR" >&2; exit 2; }
mkdir -p "$OUT"
cd "$ROOT"
exec > >(tee "$OUT/console.log") 2>&1

echo "ERF Phase 4 RuleRelease Registry"
echo "commit=$(git rev-parse HEAD)"
cargo test -q -p erf-release-registry

cat > "$OUT/phase4-release-registry.md" <<'EOF'
# Phase 4 RuleRelease Registry Evidence

The `erf-release-registry` unit and failure-injection suite passed six checks:

| Check | Result |
| --- | --- |
| verified admission by immutable manifest digest | PASS |
| duplicate admission is idempotent | PASS |
| promotion and revocation preserve release bytes | PASS |
| rollback selects a verified target and audits both state changes | PASS |
| tampered manifest/artifact and wrong backend fail before admission | PASS |
| stale operations, revoked rollback targets and empty audit context fail closed | PASS |

The registry is an in-memory lifecycle core. Persistence, registry transport,
SCM publication and production signing-key management remain outside scope.
EOF
echo "registry=PASS evidence=$OUT/phase4-release-registry.md"
