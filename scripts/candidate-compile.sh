#!/usr/bin/env bash
# Compile a governed RuleIntent into a Candidate bundle (SymbolRule template).
# Usage: scripts/candidate-compile.sh INTENT.yaml OUTPUT_DIR
# Exit: 0 compiled, 1 compile/validation failure, 2 usage/invocation error.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi
command -v java >/dev/null 2>&1 || { echo "java not found (need JDK >= 21)" >&2; exit 2; }

[ $# -eq 2 ] || { echo "usage: $0 INTENT.yaml OUTPUT_DIR" >&2; exit 2; }

CP_FILE="${ERF_FACTORY_CP_FILE:-}"
if [ -n "$CP_FILE" ]; then
  [ -r "$CP_FILE" ] || { echo "factory classpath file is not readable: $CP_FILE" >&2; exit 2; }
else
  CP_FILE="$(mktemp)"
  trap 'rm -f "$CP_FILE"' EXIT
  (cd "$REPO_ROOT" && mvn -q -pl control-plane/factory -am package -DskipTests \
    dependency:build-classpath -Dmdep.outputFile="$CP_FILE" -q)
fi

CP="$REPO_ROOT/control-plane/factory/target/classes:$REPO_ROOT/control-plane/authoring/target/classes:$(cat "$CP_FILE")"

exec java -cp "$CP" io.ruleops.factory.CandidateCmd "$@"
