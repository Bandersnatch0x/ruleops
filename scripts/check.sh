#!/usr/bin/env bash
# Repo-level check: builds and tests both language sides of ruleops.
# - Java side: Java 21 modular monolith (control-plane/*) via Maven, requires JDK >= 21
# - Rust side: runtime workspace (runtime/*) via Cargo, version pinned by rust-toolchain.toml
# Optional environment file: ~/.local/opt/erf-env.sh
#   contract: exports JAVA_HOME, PATH additions, and optionally RUSTFLAGS
#   (e.g. `-C linker=...` when no system C toolchain exists). Loaded if present.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi

if ! command -v java >/dev/null 2>&1; then
  echo "error: java not found. Set JAVA_HOME to a JDK >= 21 or provide ~/.local/opt/erf-env.sh" >&2
  exit 2
fi
JAVA_MAJOR="$(java -version 2>&1 | head -1 | sed -n 's/.*version "\([0-9][0-9]*\)\..*/\1/p')"
if [ -z "$JAVA_MAJOR" ] || [ "$JAVA_MAJOR" -lt 21 ]; then
  echo "error: JDK >= 21 required, got: $(java -version 2>&1 | head -1)" >&2
  exit 2
fi

echo "== [Java] mvn verify (modular monolith) =="
(cd "$REPO_ROOT" && mvn -q verify)
echo "== [Rust] cargo test (runtime workspace) =="
(cd "$REPO_ROOT" && cargo test --quiet)

echo "check: all sides green"
