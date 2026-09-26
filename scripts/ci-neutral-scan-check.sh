#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: ci-neutral-scan-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

package_dir="$out_dir/rf-release"
repo_dir="$(mktemp -d "$out_dir/java-repo.XXXXXX")"
mkdir -p "$repo_dir/src/main/java/example"
printf '%s\n' '<project></project>' > "$repo_dir/pom.xml"
printf '%s\n' 'package example; public class CiNeutral { }' > "$repo_dir/src/main/java/example/CiNeutral.java"

scripts/package-rf.sh "$package_dir" > "$out_dir/package.txt"
rg -q '^rf-package=PASS ' "$out_dir/package.txt"
rg -q '"sourceUploadDefault": false' "$package_dir/runtime-manifest.json"
rg -q '"scmCredentials": false' "$package_dir/runtime-manifest.json"
python3 - "$package_dir/runtime-manifest.json" <<'PY'
import hashlib
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
assert manifest["productionReplacementRequired"] is True
assert manifest["signature"]["status"] == "REFERENCE_FIXTURE"
for name, asset in manifest["assets"].items():
    path = manifest_path.parent / asset["path"]
    digest = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    assert asset["digest"] == digest, (name, asset["digest"], digest)
    assert asset["compatibilityIdentity"]
    assert asset["verification"] in {"RELEASE_ARTIFACT", "REFERENCE_FIXTURE"}
PY

"$package_dir/rf" init --repo-root "$repo_dir" --non-interactive > "$out_dir/init.txt"
rg -q '^CONFIG_WRITTEN ' "$out_dir/init.txt"

# This is the supported CI-neutral path: only the project config and released
# runtime package are supplied.  No internal --host/--resolver command is
# present in the invocation.
"$package_dir/rf" scan \
  --repo-root "$repo_dir" \
  --rule-id corp.java.ci-neutral \
  --format json \
  --progress none \
  > "$out_dir/scan.json"
rg -q '"kind": "ScanResult"' "$out_dir/scan.json"
rg -q '"effectiveConfigDigest": "sha256:' "$out_dir/scan.json"
rg -q '"workspaceDescriptorDigest": "sha256:' "$out_dir/scan.json"
rg -q '"resolvedWorkspaceDescriptorDigest": "sha256:' "$out_dir/scan.json"
rg -q '"configScanPlanDigest": "sha256:' "$out_dir/scan.json"
! rg -q 'source(Code|Content|Snippet)?|fullFinding|credentials|scm' "$out_dir/scan.json"

artifact_path="$repo_dir/.ruleops/ci-scan.json"
"$package_dir/rf" scan \
  --repo-root "$repo_dir" \
  --rule-id corp.java.ci-neutral \
  --format json \
  --progress none \
  --output "$artifact_path"
rg -q '"artifactPath": "\.ruleops/ci-scan.json"' "$artifact_path"

for mode in offline standalone; do
  if "$package_dir/rf" scan \
      --repo-root "$repo_dir" \
      --rule-id corp.java.ci-neutral \
      --execution-mode "$mode" \
      --format json \
      --progress none \
      > "$out_dir/scan-$mode-failure.json" \
      2> "$out_dir/scan-$mode-failure.stderr"; then
    mode_rc=0
  else
    mode_rc=$?
  fi
  [ "$mode_rc" -eq 3 ] || { echo "expected $mode scan to exit 3, got $mode_rc" >&2; exit 1; }
  [ ! -s "$out_dir/scan-$mode-failure.stderr" ] || { echo "structured $mode failure leaked raw stderr" >&2; exit 1; }
  python3 - "$out_dir/scan-$mode-failure.json" <<'PY'
import json
import sys

failure = json.loads(open(sys.argv[1], encoding="utf-8").read())
assert failure["kind"] == "ScanFailure"
assert failure["exitCode"] == 3
assert failure["diagnostics"][0]["code"] == "DELIVERY_INCONCLUSIVE"
assert "detail" not in failure["diagnostics"][0]
assert failure["diagnostics"][0]["detailDigest"].startswith("sha256:")
PY
done

for mode in connected offline standalone; do
  "$package_dir/rf" config show --repo-root "$repo_dir" --execution-mode "$mode" > "$out_dir/config-$mode.txt"
  rg -q "^execution_mode: $mode$" "$out_dir/config-$mode.txt"
done
connected_digest="$(sed -n 's/^scan_plan_digest=//p' "$out_dir/config-connected.txt")"
offline_digest="$(sed -n 's/^scan_plan_digest=//p' "$out_dir/config-offline.txt")"
standalone_digest="$(sed -n 's/^scan_plan_digest=//p' "$out_dir/config-standalone.txt")"
[ -n "$connected_digest" ] && [ "$connected_digest" != "$offline_digest" ] && [ "$offline_digest" != "$standalone_digest" ]

cat > "$out_dir/ci-neutral-scan.md" <<EOF
# CI-neutral rf scan evidence

- Acceptance command: scripts/ci-neutral-scan-check.sh $out_dir
- Runtime manifest: runtime-manifest.json
- Invocation uses project configuration plus the installed runtime package; it does not pass internal --host or --resolver commands.
- Output format: closed JSON ScanResult; source, full Finding and SCM credential fields are absent.
- Artifact output: --output writes a repository-local artifact and the result carries artifactPath.
- Execution modes: connected, offline and standalone each resolve a distinct deterministic ScanPlan identity.
- Offline/Standalone without a verified Candidate or delivery/cache input fail closed as structured INCONCLUSIVE (exit 3).

| Boundary | Evidence | Result |
| --- | --- | --- |
| Installable rf release layout | package.txt, runtime-manifest.json | PASS |
| Non-interactive CI scan without internal process arguments | scan.json | PASS |
| Project identity and machine result | scan.json | PASS |
| Source/credential privacy boundary | scan.json | PASS |
| Artifact entrance | ci-scan.json | PASS |
| Connected/Offline/Standalone config identity | config-*.txt | PASS |
| Offline/Standalone delivery boundary | scan-offline-failure.json, scan-standalone-failure.json | PASS |
EOF

echo "ci-neutral-scan=PASS evidence=$out_dir/ci-neutral-scan.md"
