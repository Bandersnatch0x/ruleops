#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-delivery-check.sh <output-dir>}"
manifest="$repo_root/specs/pilot/pilot-release-manifest.yaml"
contract="$repo_root/specs/pilot/pilot-success-contract.yaml"
plan="$repo_root/specs/pilot/pilot-observation-plan.yaml"
sample="$repo_root/specs/pilot/live-java-repositories.yaml"

mkdir -p "$out_dir"
cd "$repo_root"

for asset in "$manifest" "$contract" "$plan" "$sample"; do
  [ -s "$asset" ] || { echo "missing delivery asset: $asset" >&2; exit 1; }
done

rg -q '^kind: PilotReleaseManifest$' "$manifest"
rg -q '^  id: erf-java-shadow-mvp-v0$' "$manifest"
rg -q '^  version: 0.1.0$' "$manifest"
rg -q '^  status: READY_FOR_SHADOW_WINDOW$' "$manifest"
rg -q '^  releaseType: SHADOW_ONLY_MVP$' "$manifest"
rg -q '^  authority: SHADOW$' "$manifest"
rg -q '^    repositories: 3$' "$manifest"
rg -q '^    currentOutcome: INCONCLUSIVE_UNTIL_FORMAL_WINDOW$' "$manifest"
rg -q '^    sourceUpload: denied$' "$manifest"
rg -q '^    repositoryWrites: denied$' "$manifest"
rg -q '^    scmStatusPublication: denied$' "$manifest"
rg -q '^    protectedBranchAuthority: denied$' "$manifest"
rg -q '^    command: bash scripts/check.sh$' "$manifest"

rg -q '^  status: APPROVED$' "$contract"
rg -q '^  authority: SHADOW$' "$contract"
rg -q '^  status: REGISTERED$' "$plan"
rg -q '^  authority: SHADOW$' "$plan"
rg -q '^    repositoryCount: 3$' "$plan"
rg -q '^kind: LivePilotRepositoryManifest$' "$sample"
rg -q '^  count: 3$' "$sample"

for script in \
  scripts/check.sh \
  scripts/pilot-local-simulation.sh \
  scripts/pilot-live-observation.sh \
  scripts/pilot-observation-check.sh \
  scripts/pilot-delivery-check.sh; do
  [ -x "$script" ] || { echo "delivery script is not executable: $script" >&2; exit 1; }
done

./rf --help > "$out_dir/rf-help.txt"
for command in \
  'rf pilot delivery-check' \
  'rf pilot baseline' \
  'rf pilot observe' \
  'rf pilot feedback' \
  'rf pilot governance' \
  'rf pilot window-report' \
  'rf center check'; do
  rg -Fq "$command" "$out_dir/rf-help.txt" || {
    echo "missing delivery CLI surface: $command" >&2
    exit 1
  }
done

git diff --check

cat > "$out_dir/pilot-release-candidate.md" <<EOF
# ERF Shadow MVP Delivery Preflight

- Manifest: \`specs/pilot/pilot-release-manifest.yaml\`
- Release status: \`READY_FOR_SHADOW_WINDOW\`
- Release type: \`SHADOW_ONLY_MVP\`
- Authority: \`SHADOW\` only
- Preflight command: \`scripts/pilot-delivery-check.sh $out_dir\`
- Final engineering gate: \`bash scripts/check.sh\` (required before handoff)

| Delivery surface | Result |
| --- | --- |
| Approved Pilot Success Contract | PASS |
| Registered three-repository observation plan | PASS |
| Fixed Java repository manifest | PASS |
| Local simulation and live Shadow entrypoints | PASS |
| Digest-only and no-write safety boundary | PASS |
| \`rf\` delivery and observation CLI surface | PASS |
| RuleOps Center closure CLI surface | PASS |
| Working-tree diff check | PASS |

## Handoff boundary

This is a complete engineering MVP for a Shadow-only pilot. It is ready to
enter the registered 2026-10-01 through 2026-10-31 UTC observation window after
the final engineering gate passes. It is not a production-enforcement release
and does not grant protected-branch authority.

The outcome remains \`INCONCLUSIVE\` until the formal window supplies the
latency baseline, resource baseline, and human precision/recall/
false-positive adjudication required by the approved contract. Missing inputs
must remain missing; this package does not synthesize them.

## Standard handoff sequence

1. Run \`bash scripts/check.sh\` and retain its evidence directory.
2. Run \`scripts/pilot-live-observation.sh OUT_DIR\` during the registered window.
3. Run \`rf pilot window-report ...\` and attach only digest-only evidence.
4. Complete the human adjudication and governance decision before any future
   authority change.
EOF

echo "pilot-delivery=PASS evidence=$out_dir/pilot-release-candidate.md"
