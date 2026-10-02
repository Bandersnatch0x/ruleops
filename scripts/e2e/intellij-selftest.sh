#!/usr/bin/env bash
# Zero-dependency IntelliJ thin-client core selftest (plain javac + java).
set -euo pipefail
out="${1:?usage: intellij-selftest.sh OUT}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
classes="$out/intellij-classes"
rm -rf "$classes"
mkdir -p "$classes"
javac -d "$classes" \
  "$repo/clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java" \
  "$repo/clients/intellij/selftest/io/ruleops/ide/core/CoreSelfTest.java"
java -cp "$classes" io.ruleops.ide.core.CoreSelfTest
