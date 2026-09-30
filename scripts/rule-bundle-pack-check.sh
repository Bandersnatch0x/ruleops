#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/.scratch/rule-bundle-pack-check}"
case "$out_dir" in
  /*) ;;
  *) out_dir="$repo_root/$out_dir" ;;
esac
mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"

first="$out_dir/first"
second="$out_dir/second"
# 只清理门禁固定临时目录内的自有产物，不删除调用者指定目录的数据。
if [ "$out_dir" = "$repo_root/.scratch/rule-bundle-pack-check" ]; then
  rm -rf -- "$first" "$second"
fi
intents=(
  "$repo_root/specs/rule-intent/corp.java.no-system-exit.yaml"
  "$repo_root/specs/rule-intent/corp.java.avoid-duplicate-literals.yaml"
)

scripts/rule-bundle-pack.sh "$first" "${intents[@]}" > "$out_dir/first.log"
scripts/rule-bundle-pack.sh "$second" "${intents[@]}" > "$out_dir/second.log"

for root in "$first" "$second"; do
  manifest="$root/bundle/manifest.json"
  archive="$(sed -n 's/^BUNDLE_ARCHIVE //p' "$root.log")"
  test -s "$manifest"
  test -s "$archive"
  jq -e '
    .kind == "erf-rule-bundle" and
    .schemaVersion == "erf-rule-bundle-v1" and
    .ruleCount == 2 and
    (.rules | map(.id) | sort) == [
      "corp.java.avoid-duplicate-literals",
      "corp.java.no-system-exit"
    ] and
    (.digest | test("^[0-9a-f]{64}$"))
  ' "$manifest" >/dev/null
  test "$(rg -c '^  <rule ' "$root/bundle/ruleset.xml")" -eq 2
  test "$(tar -tzf "$archive" | sort | tr '\n' ' ')" = "./ ./manifest.json ./ruleset.xml "
  tar -xOf "$archive" ./manifest.json | cmp - "$manifest"
done

cmp "$first/bundle/ruleset.xml" "$second/bundle/ruleset.xml"
cmp "$first/bundle/manifest.json" "$second/bundle/manifest.json"
first_archive="$(sed -n 's/^BUNDLE_ARCHIVE //p' "$out_dir/first.log")"
second_archive="$(sed -n 's/^BUNDLE_ARCHIVE //p' "$out_dir/second.log")"
test "$(sha256sum "$first_archive" | awk '{print $1}')" = "$(sha256sum "$second_archive" | awk '{print $1}')"

cat > "$out_dir/rule-bundle-pack.md" <<EOF
# Multi-rule RuleBundle packaging evidence

- schema: \`erf-rule-bundle-v1\`
- rules: \`corp.java.avoid-duplicate-literals\`, \`corp.java.no-system-exit\`
- payload: one deterministic PMD ruleset containing two rule nodes
- manifest: lists each RuleIntent, template and Candidate digest
- digest: excludes \`manifest.json\` and binds the executable \`ruleset.xml\` payload
- archive: fixed tar member metadata and gzip timestamp
- repeatability: two independent builds have identical ruleset, manifest and archive bytes

The pack contains no repository source and remains a Candidate/Shadow delivery
input until a separate trusted CI attestation establishes authority.
EOF

echo "rule-bundle-pack=PASS evidence=$out_dir/rule-bundle-pack.md"
