#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:?usage: package-rf.sh <output-dir>}"

mkdir -p "$output_dir/bin"
cd "$repo_root"

# The package owns the public rf entrypoint and its release binaries.  The
# reference host/resolver occupy the same named slots as deployable assets so
# the CI-neutral example exercises the installation contract without making a
# fake engine look like a production PMD/SpotBugs adapter.
cargo build --quiet --release -p rf-cli -p erf-project-config -p erf-fake-host -p erf-fake-resolver
cp "$repo_root/rf" "$output_dir/rf"
chmod +x "$output_dir/rf"
cp "$repo_root/target/release/rf-cli" "$output_dir/bin/rf-cli"
cp "$repo_root/target/release/erf-project-config" "$output_dir/bin/erf-project-config"
cp "$repo_root/target/release/erf-fake-host" "$output_dir/bin/rf-backend-host"
cp "$repo_root/target/release/erf-fake-resolver" "$output_dir/bin/rf-workspace-resolver"
chmod +x "$output_dir/bin/rf-cli" "$output_dir/bin/erf-project-config" "$output_dir/bin/rf-backend-host" "$output_dir/bin/rf-workspace-resolver"

backend_digest="$(sha256sum "$output_dir/bin/rf-backend-host" | awk '{print $1}')"
resolver_digest="$(sha256sum "$output_dir/bin/rf-workspace-resolver" | awk '{print $1}')"
cli_digest="$(sha256sum "$output_dir/bin/rf-cli" | awk '{print $1}')"
config_digest="$(sha256sum "$output_dir/bin/erf-project-config" | awk '{print $1}')"

cat > "$output_dir/runtime-manifest.json" <<EOF
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "RfRuntimeRelease",
  "schemaVersion": "rf-runtime-release-v1",
  "entrypoint": "rf",
  "cli": "bin/rf-cli",
  "projectConfig": "bin/erf-project-config",
  "backendHost": "bin/rf-backend-host",
  "workspaceResolver": "bin/rf-workspace-resolver",
  "assets": {
    "cli": {
      "path": "bin/rf-cli",
      "digest": "sha256:$cli_digest",
      "compatibilityIdentity": "rf-cli-v1",
      "verification": "RELEASE_ARTIFACT"
    },
    "projectConfig": {
      "path": "bin/erf-project-config",
      "digest": "sha256:$config_digest",
      "compatibilityIdentity": "project-config-v1",
      "verification": "RELEASE_ARTIFACT"
    },
    "backendHost": {
      "path": "bin/rf-backend-host",
      "digest": "sha256:$backend_digest",
      "compatibilityIdentity": "reference-contract-v0",
      "verification": "REFERENCE_FIXTURE"
    },
    "workspaceResolver": {
      "path": "bin/rf-workspace-resolver",
      "digest": "sha256:$resolver_digest",
      "compatibilityIdentity": "reference-contract-v0",
      "verification": "REFERENCE_FIXTURE"
    }
  },
  "signature": {
    "status": "REFERENCE_FIXTURE",
    "algorithm": "none"
  },
  "productionReplacementRequired": true,
  "sourceUploadDefault": false,
  "fullFindingUploadDefault": false,
  "scmCredentials": false
}
EOF

echo "rf-package=PASS output=$output_dir manifest=$output_dir/runtime-manifest.json"
