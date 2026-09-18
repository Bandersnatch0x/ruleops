#!/usr/bin/env bash
# Reproducible Java artifact evidence for one unchanged working tree.
# Usage: scripts/reproducible-java-build-check.sh OUT_DIR
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: reproducible-java-build-check.sh <output-dir>}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out_dir"

cd "$repo_root"
for run in 1 2; do
  if ! mvn -q clean verify >"$out_dir/mvn-run-$run.log" 2>&1; then
    echo "java-build=$run failed; see $out_dir/mvn-run-$run.log" >&2
    exit 1
  fi
  mkdir -p "$work/$run/artifacts"
  find "$repo_root/control-plane" -type f -path '*/target/*.jar' -printf '%P\n' \
    | sort >"$work/$run/paths.txt"
  [ -s "$work/$run/paths.txt" ] || {
    echo "java-build=$run produced no control-plane JARs" >&2
    exit 1
  }
  while IFS= read -r relative; do
    source="$repo_root/control-plane/$relative"
    destination="$work/$run/artifacts/$relative"
    mkdir -p "$(dirname "$destination")"
    cp "$source" "$destination"
  done <"$work/$run/paths.txt"
  (
    cd "$work/$run/artifacts"
    while IFS= read -r relative; do
      sha256sum "$relative"
    done <"../paths.txt"
  ) >"$work/$run/sha256.txt"
done

diff -u "$work/1/paths.txt" "$work/2/paths.txt" >/dev/null || {
  echo "control-plane JAR set changed between clean builds" >&2
  exit 1
}
diff -u "$work/1/sha256.txt" "$work/2/sha256.txt" >/dev/null || {
  echo "control-plane JAR digest changed between clean builds" >&2
  exit 1
}
while IFS= read -r relative; do
  cmp -- "$work/1/artifacts/$relative" "$work/2/artifacts/$relative"
done <"$work/1/paths.txt"

cp "$work/1/paths.txt" "$out_dir/artifacts.txt"
cp "$work/1/sha256.txt" "$out_dir/sha256.txt"
cat >"$out_dir/reproducible-java-build.md" <<EOF
# Reproducible Java Build Evidence

- Command: \`mvn clean verify\`, run twice on one unchanged working tree
- Artifact scope: control-plane JAR files under target directories
- Artifact count: $(wc -l <"$work/1/paths.txt")
- Java: $(java -version 2>&1 | head -n 1)
- Maven: $(mvn -version 2>&1 | head -n 1)

| Check | Result |
| --- | --- |
| First clean Maven build succeeded | PASS |
| Second clean Maven build succeeded | PASS |
| JAR path set is identical | PASS |
| JAR SHA-256 manifest is identical | PASS |
| JAR bytes are identical | PASS |

Artifact paths and the first-build SHA-256 manifest are retained beside this
report. This check covers build reproducibility only; it does not grant runtime
trust or production authority.
EOF

echo "java-reproducibility=PASS artifacts=$(wc -l <"$work/1/paths.txt") evidence=$out_dir/reproducible-java-build.md"
