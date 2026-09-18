#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: representative-java-repositories-check.sh <output-dir>}"
manifest="$repo_root/specs/pilot/representative-java-repositories.yaml"
shadow_script="$repo_root/scripts/phase4-shadow-pilot.sh"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$manifest" ] || { echo "missing representative repository manifest" >&2; exit 1; }
rg -q '^kind: RepresentativeRepositoryManifest$' "$manifest"
rg -q '^  count: 3$' "$manifest"
rg -q '^  authority: SHADOW$' "$manifest"
rg -q '^    productionRepository: false$' "$manifest"
rg -q '^    scmCredentials: none$' "$manifest"
rg -q '^    network: denied$' "$manifest"
rg -q '^    repositoryWrites: denied$' "$manifest"
rg -q '^    sourceUpload: denied$' "$manifest"
rg -q '^  - id: shadow-repository-1$' "$manifest"
rg -q '^  - id: shadow-repository-2$' "$manifest"
rg -q '^  - id: shadow-repository-3$' "$manifest"
[ "$(rg -c '^  - id:' "$manifest")" -eq 3 ] || { echo "expected three repository entries" >&2; exit 1; }

for field in repository commit fixturePath; do
  values="$(awk -v field="$field" '$0 ~ "^    " field ":" { print $2 }' "$manifest")"
  [ "$(printf '%s\n' "$values" | sed '/^$/d' | sort -u | wc -l)" -eq 3 ] || { echo "$field identities are not unique" >&2; exit 1; }
done
[ "$(rg '^  - id:' "$manifest" | sort -u | wc -l)" -eq 3 ] || { echo "repository ids are not unique" >&2; exit 1; }

for index in 1 2 3; do
  id="shadow-repository-$index"
  record="$(awk -v id="$id" '
    $0 == "  - id: " id { found=1 }
    found && /^  - id:/ && $0 != "  - id: " id { exit }
    found { print }
  ' "$manifest")"
  [ -n "$record" ] || { echo "missing manifest entry: $id" >&2; exit 1; }
  printf '%s\n' "$record" | rg -q "^    repository: org/ruleops-shadow-$index$"
  printf '%s\n' "$record" | rg -q "^    commit: phase4-shadow-commit-$index$"
  printf '%s\n' "$record" | rg -q "^    fixturePath: repositories/repo-$index$"
  printf '%s\n' "$record" | rg -q '^    primaryScenario: direct-system-exit$'
  printf '%s\n' "$record" | rg -q '^    ruleIntent: corp.java.no-system-exit$'
done

rg -q 'mkdir -p "\$repo/src/main/java"' "$shadow_script"
rg -q 'org/ruleops-shadow-\$index' "$shadow_script"
rg -q 'phase4-shadow-commit-\$index' "$shadow_script"
rg -q 'erf-trusted-ci --example phase3_real_scan' "$shadow_script"
! rg -n 'https?://|github|git@' "$manifest"

cat > "$out_dir/representative-java-repositories.md" <<EOF
# Representative Java Repository Manifest Evidence

- Acceptance command: scripts/representative-java-repositories-check.sh $out_dir
- Source: specs/pilot/representative-java-repositories.yaml
- Fixed repository count: 3
- Language/build: Java/Maven
- Authority: Shadow-only on linux/amd64
- Selection: deterministic local fixture with three distinct repository and commit identities

| Check | Evidence | Result |
| --- | --- | --- |
| Fixed three-entry denominator | manifest count and entry identities | PASS |
| Repository and commit identities are unique | manifest fields | PASS |
| Manifest identities match the Shadow handoff harness | phase4-shadow-pilot.sh | PASS |
| Local fixture, credential, network and write boundaries are explicit | manifest boundaries | PASS |
| No external production-repository claim is encoded | productionRepository=false and no external locator | PASS |

These entries qualify independent local handoff identities only. They do not
claim external repository selection, live production prevalence, or protected
branch authority.
EOF

echo "representative-java-repositories=PASS evidence=$out_dir/representative-java-repositories.md"
