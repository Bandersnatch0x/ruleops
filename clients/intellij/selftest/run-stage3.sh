#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
[ "$#" -le 1 ] || { echo "usage: run-stage3.sh [OUTPUT_DIR]" >&2; exit 2; }
out="${1:-clients/intellij/build/stage3-selftest}"
mkdir -p "$out"
timeout 60s javac -d "$out" \
  clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java \
  clients/intellij/src/main/java/io/ruleops/ide/ErfRegistry.java \
  clients/intellij/selftest/io/ruleops/ide/core/*.java \
  clients/intellij/selftest/io/ruleops/ide/RegistrySelfTest.java \
  clients/intellij/selftest/io/ruleops/ide/JavaSyntaxSelfTest.java
timeout 60s java -cp "$out" io.ruleops.ide.core.CoreSelfTest
timeout 60s java -cp "$out" io.ruleops.ide.RegistrySelfTest
timeout 60s java -cp "$out" io.ruleops.ide.JavaSyntaxSelfTest
