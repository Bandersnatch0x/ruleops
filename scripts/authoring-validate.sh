#!/usr/bin/env bash
# Validate governed authoring assets (Standard / RuleIntent / Rule IR).
# Usage: scripts/authoring-validate.sh FILE...
# Exit: 0 valid, 1 validation failures, 2 usage/invocation error.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi
command -v java >/dev/null 2>&1 || { echo "java not found (need JDK >= 21)" >&2; exit 2; }

[ $# -ge 1 ] || { echo "usage: $0 FILE..." >&2; exit 2; }

(cd "$REPO_ROOT" && mvn -q -pl control-plane/authoring -am package -DskipTests)

CP_FILE="$(mktemp)"
trap 'rm -f "$CP_FILE"' EXIT
(cd "$REPO_ROOT" && mvn -q -pl control-plane/authoring dependency:build-classpath -Dmdep.outputFile="$CP_FILE" -q)
CP="$REPO_ROOT/control-plane/authoring/target/classes:$(cat "$CP_FILE")"

exec java -cp "$CP" io.ruleops.authoring.ValidateCmd "$@"
