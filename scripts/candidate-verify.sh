#!/usr/bin/env bash
# Local verification of a compiled Candidate: run the PMD artifact over the
# generated fixtures and write an EvidenceBundle skeleton (LOCAL_UNATTESTED).
# Usage: scripts/candidate-verify.sh CANDIDATE_DIR OUT_DIR
# Exit: 0 PASS, 1 FAIL, 2 usage/invocation error.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi
command -v java >/dev/null 2>&1 || { echo "java not found (need JDK >= 21)" >&2; exit 2; }

[ $# -eq 2 ] || { echo "usage: $0 CANDIDATE_DIR OUT_DIR" >&2; exit 2; }

CP_FILE="${ERF_VERIFICATION_CP_FILE:-}"
if [ -n "$CP_FILE" ]; then
  [ -r "$CP_FILE" ] || { echo "verification classpath file is not readable: $CP_FILE" >&2; exit 2; }
else
  CP_FILE="$(mktemp)"
  trap 'rm -f "$CP_FILE"' EXIT
  (cd "$REPO_ROOT" && mvn -q -pl control-plane/verification -am package -DskipTests \
    dependency:build-classpath -Dmdep.outputFile="$CP_FILE" -q)
fi

CP="$REPO_ROOT/control-plane/verification/target/classes:$REPO_ROOT/control-plane/factory/target/classes:$REPO_ROOT/control-plane/authoring/target/classes:$(cat "$CP_FILE")"

exec java -cp "$CP" io.ruleops.verification.VerifyCmd "$@"
