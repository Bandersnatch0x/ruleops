#!/usr/bin/env bash
# End-to-end: Rust host client drives the Java Backend Host (PMD 7.x).
# Builds the shaded host jar, then runs the java_host integration tests.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi

(cd "$REPO_ROOT/backend-hosts/java" && mvn -q clean package -DskipTests)
export ERF_JAVA_HOST_JAR="$REPO_ROOT/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar"
export CARGO_NET_RETRY=10
(cd "$REPO_ROOT" && cargo test -p erf-host-client --test java_host)
