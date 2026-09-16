#!/usr/bin/env bash
# Repo-level check: builds and tests both language sides of ruleops.
# - Java side: Java 21 modular monolith (control-plane/*) via Maven
# - Rust side: runtime workspace (runtime/*) via Cargo
# Exits non-zero on the first failing side.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi

echo "== [Java] mvn verify (modular monolith) =="
(cd "$REPO_ROOT" && mvn -q verify)
echo "== [Rust] cargo test (runtime workspace) =="
(cd "$REPO_ROOT" && cargo test --quiet)

echo "check: all sides green"
