#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: java-ci-inventory-check.sh <output-dir>}"
inventory="$repo_root/specs/compatibility/java-ci-inventory.yaml"
matrix="$repo_root/specs/compatibility/java-backend-matrix.yaml"
pom="$repo_root/backend-hosts/java/pom.xml"
mkdir -p "$out_dir"
cd "$repo_root"

for file in "$inventory" "$matrix" "$pom"; do
  [ -f "$file" ] || { echo "missing configuration inventory input: $file" >&2; exit 1; }
done

pmd_version="$(sed -n 's/.*<pmd.version>\([^<]*\)<\/pmd.version>.*/\1/p' "$pom")"
spotbugs_version="$(sed -n 's/.*<spotbugs.version>\([^<]*\)<\/spotbugs.version>.*/\1/p' "$pom")"
main_class="$(sed -n 's/.*<mainClass>\([^<]*\)<\/mainClass>.*/\1/p' "$pom")"
[ "$pmd_version" = "7.9.0" ] || { echo "unexpected PMD POM version: $pmd_version" >&2; exit 1; }
[ "$spotbugs_version" = "4.9.8" ] || { echo "unexpected SpotBugs POM version: $spotbugs_version" >&2; exit 1; }
[ "$main_class" = "io.ruleops.javahost.JavaHostMain" ] || { echo "unexpected Java Host main class: $main_class" >&2; exit 1; }

rg -q '^  id: java-ci-configuration-v0$' "$inventory"
rg -q '^    artifactId: erf-java-host$' "$inventory"
rg -q '^    compilerRelease: 21$' "$inventory"
rg -q '^    sourceLevels: \["11", "17", "21"\]$' "$inventory"
rg -q "^      engineVersion: $pmd_version$" "$inventory"
rg -q "^      engineVersion: $spotbugs_version$" "$inventory"
rg -q "^    mainClass: $main_class$" "$inventory"
rg -q '^    repositoryConfig: absent$' "$inventory"
rg -q '^    externalStatusPublisher: absent$' "$inventory"
rg -q '^    scmCredentials: absent$' "$inventory"
rg -q '^    sourceUpload: absent$' "$inventory"
rg -q '^    networkAdapter: absent$' "$inventory"

for entrypoint in \
  scripts/check.sh \
  scripts/java-host-e2e.sh \
  scripts/phase2-java-shadow.sh \
  scripts/check-java-compatibility.sh \
  scripts/phase4-pilot-report.sh \
  rf; do
  [ -f "$entrypoint" ] || { echo "missing executable check: $entrypoint" >&2; exit 1; }
done

[ ! -d .github ] || { echo "repository CI configuration changed; update java-ci-inventory.yaml" >&2; exit 1; }

cat > "$out_dir/java-ci-configuration-inventory.md" <<EOF
# Java PMD/SpotBugs and CI Configuration Inventory

- Inventory: specs/compatibility/java-ci-inventory.yaml
- Java Host: backend-hosts/java/pom.xml, main class $main_class
- Compiler release: 21
- Supported declared source levels: 11, 17, 21
- PMD: $pmd_version, source mode, VERIFIED by scripts/java-host-e2e.sh
- SpotBugs: $spotbugs_version, bytecode mode, VERIFIED by scripts/java-host-e2e.sh
- Repository gate: bash scripts/check.sh
- External CI repository configuration: absent in the current checkout
- Trust: linux/amd64 and SHADOW only
- SCM credentials, source upload, and network adapter: absent

## Executable checks

The local gate, Java Host E2E, representative Shadow scan, compatibility
matrix check, Phase 4 pilot report, and V0.1 rf CLI entrypoints are present and
recorded in the machine inventory. Their PASS results are evidence for this
checkout; no external CI provider status is inferred.

## Explicitly untested

- other PMD or SpotBugs engine versions;
- non-linux or non-amd64 trusted execution;
- external CI provider status integration;
- SCM repository write or credential integration.
EOF

echo "java-ci-inventory=PASS evidence=$out_dir/java-ci-configuration-inventory.md"
