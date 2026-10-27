#!/usr/bin/env bash
# Local wrapper for the audit/research evidence-claim linter.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$REPO_ROOT/scripts/evidence-claim-lint.py" "$@"
