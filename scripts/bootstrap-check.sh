#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: bootstrap-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

cargo test -q -p erf-project-config --all-targets > "$out_dir/project-config-tests.log" 2>&1
rg -q 'test result: ok\. 15 passed; 0 failed; 0 ignored' "$out_dir/project-config-tests.log"
rg -q 'ProjectConfig|apply_precedence|detect_workspace|run_doctor' runtime/project-config/src/lib.rs
rg -q 'effectiveConfigDigest|workspaceDescriptorDigest|configScanPlanDigest' contracts/http/scan-output-v1.yaml runtime/scan-output/src/lib.rs
rg -q 'init|doctor|config validate|config show' rf scripts/cli-surface-check.sh
rg -q 'CONFIG_MISSING|DEPENDENCY_UNAVAILABLE|CACHE_INVALID|TRUST_STATE_MISSING|SCAN_FAILED' runtime/project-config/src/lib.rs

# Recreate the generated fixture so repeated gate runs exercise the same
# bootstrap path without asking init to overwrite a reviewed configuration.
rm -rf "$out_dir/java-repo"
mkdir -p "$out_dir/java-repo/src/main/java/example"
printf '%s\n' '<project></project>' > "$out_dir/java-repo/pom.xml"
printf '%s\n' 'package example; public class App { }' > "$out_dir/java-repo/src/main/java/example/App.java"

./rf init --repo-root "$out_dir/java-repo" --non-interactive > "$out_dir/init.txt"
rg -q '^CONFIG_WRITTEN ' "$out_dir/init.txt"
./rf config validate --repo-root "$out_dir/java-repo" > "$out_dir/config-validate.txt"
rg -q '^CONFIG_VALID ' "$out_dir/config-validate.txt"
./rf config show --repo-root "$out_dir/java-repo" --format json > "$out_dir/config-show.json"
rg -q '"config_digest"|"configDigest"' "$out_dir/config-show.json"

./rf config show --repo-root "$out_dir/java-repo" > "$out_dir/config-show-1.txt"
./rf config show --repo-root "$out_dir/java-repo" > "$out_dir/config-show-2.txt"
diff -u "$out_dir/config-show-1.txt" "$out_dir/config-show-2.txt"
RULEOPS_EXECUTION_MODE=offline ./rf config show --repo-root "$out_dir/java-repo" > "$out_dir/config-env.txt"
rg -q '^execution_mode: offline$' "$out_dir/config-env.txt"
./rf config show --repo-root "$out_dir/java-repo" --execution-mode standalone > "$out_dir/config-cli.txt"
rg -q '^execution_mode: standalone$' "$out_dir/config-cli.txt"

# A real repository may contain a supported Java backend and an unsupported
# frontend. Explicit Java/Maven scope must resolve that language ambiguity;
# leaving it unspecified must remain fail-closed.
rm -rf "$out_dir/multilang-repo"
mkdir -p "$out_dir/multilang-repo/src/main/java/example" "$out_dir/multilang-repo/schemaplexai-ui/src"
printf '%s\n' '<project><modelVersion>4.0.0</modelVersion><groupId>example</groupId><artifactId>multilang</artifactId><version>1</version></project>' > "$out_dir/multilang-repo/pom.xml"
printf '%s\n' 'package example; public final class App { private App() {} }' > "$out_dir/multilang-repo/src/main/java/example/App.java"
printf '%s\n' 'export const frontend = "javascript";' > "$out_dir/multilang-repo/schemaplexai-ui/src/main.ts"
set +e
./rf init --repo-root "$out_dir/multilang-repo" --non-interactive > "$out_dir/multilang-init-default.txt" 2>&1
multilang_default_rc=$?
set -e
[ "$multilang_default_rc" -eq 2 ]
rg -q 'language detection is ambiguous: javascript' "$out_dir/multilang-init-default.txt"
./rf init --repo-root "$out_dir/multilang-repo" --non-interactive --language java --build-system maven > "$out_dir/multilang-init-explicit.txt"
rg -q '^CONFIG_WRITTEN ' "$out_dir/multilang-init-explicit.txt"
rg -q '^language: java$' "$out_dir/multilang-repo/.ruleops/config.yaml"
rg -q '^build_system: maven$' "$out_dir/multilang-repo/.ruleops/config.yaml"
rg -q 'src/main/java/\*\*' "$out_dir/multilang-repo/.ruleops/config.yaml"

cp "$out_dir/java-repo/.ruleops/config.yaml" "$out_dir/duplicate-config.yaml"
printf '%s\n' 'language: java' >> "$out_dir/duplicate-config.yaml"
set +e
./rf config validate --repo-root "$out_dir/java-repo" --config "$out_dir/duplicate-config.yaml" > "$out_dir/duplicate-config.txt" 2>&1
duplicate_rc=$?
set -e
[ "$duplicate_rc" -eq 2 ]
rg -q 'duplicated' "$out_dir/duplicate-config.txt"

mkdir -p "$out_dir/unsupported-repo"
printf '%s\n' 'print("not Java")' > "$out_dir/unsupported-repo/main.py"
set +e
./rf init --repo-root "$out_dir/unsupported-repo" --non-interactive > "$out_dir/unsupported-init.txt" 2>&1
unsupported_rc=$?
set -e
[ "$unsupported_rc" -eq 2 ]
[ ! -e "$out_dir/unsupported-repo/.ruleops/config.yaml" ]
rg -q 'unsupported|conflict|detection' "$out_dir/unsupported-init.txt"

set +e
./rf doctor --repo-root "$out_dir/missing-repo" > "$out_dir/doctor-missing.txt" 2>&1
missing_doctor_rc=$?
set -e
[ "$missing_doctor_rc" -eq 2 ]
rg -q 'CONFIG_MISSING' "$out_dir/doctor-missing.txt"

set +e
./rf doctor --repo-root "$out_dir/java-repo" > "$out_dir/doctor-empty.txt" 2>&1
empty_doctor_rc=$?
set -e
[ "$empty_doctor_rc" -eq 3 ]
rg -q 'TRUST_STATE_MISSING' "$out_dir/doctor-empty.txt"
rg -q 'CACHE_MISSING|CACHE_EMPTY' "$out_dir/doctor-empty.txt"

set +e
env RULEOPS_EXECUTION_MODE=offline ./rf doctor --repo-root "$out_dir/java-repo" > "$out_dir/doctor-offline-env.txt" 2>&1
offline_env_rc=$?
set -e
[ "$offline_env_rc" -eq 3 ]
rg -q '^doctor=INCONCLUSIVE mode=offline$' "$out_dir/doctor-offline-env.txt"

set +e
env -u RULEOPS_EXECUTION_MODE ./rf doctor --repo-root "$out_dir/java-repo" --execution-mode standalone > "$out_dir/doctor-standalone-cli.txt" 2>&1
standalone_cli_rc=$?
set -e
[ "$standalone_cli_rc" -eq 3 ]
rg -q '^doctor=INCONCLUSIVE mode=standalone$' "$out_dir/doctor-standalone-cli.txt"

set +e
./rf doctor --repo-root "$out_dir/java-repo" --scan-probe "$out_dir/no-such-scan" > "$out_dir/doctor-scan-failed.txt" 2>&1
scan_doctor_rc=$?
set -e
[ "$scan_doctor_rc" -eq 3 ]
rg -q 'SCAN_FAILED' "$out_dir/doctor-scan-failed.txt"

mkdir -p "$out_dir/java-repo/.ruleops/cache/bundles/sha256"
printf '%s\n' 'tampered' > "$out_dir/java-repo/.ruleops/cache/bundles/sha256/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
set +e
./rf doctor --repo-root "$out_dir/java-repo" > "$out_dir/doctor-corrupt-cache.txt" 2>&1
corrupt_doctor_rc=$?
set -e
[ "$corrupt_doctor_rc" -eq 3 ]
rg -q 'CACHE_INVALID' "$out_dir/doctor-corrupt-cache.txt"

mkdir -p "$out_dir/outside-cache"
ln -s "$out_dir/outside-cache" "$out_dir/java-repo/.ruleops/trust-state"
set +e
./rf doctor --repo-root "$out_dir/java-repo" > "$out_dir/doctor-symlink-path.txt" 2>&1
symlink_doctor_rc=$?
set -e
[ "$symlink_doctor_rc" -eq 2 ]
rg -q 'symbolic link|PathUsesSymlink|CONFIG_INVALID' "$out_dir/doctor-symlink-path.txt"
unlink "$out_dir/java-repo/.ruleops/trust-state"

cargo build --quiet -p erf-fake-host -p erf-fake-resolver
./rf scan \
  --repo-root "$out_dir/java-repo" \
  --rule-id corp.java.bootstrap-smoke \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" > "$out_dir/scan.txt"
rg -q '^effective_config_digest=sha256:' "$out_dir/scan.txt"
rg -q '^workspace_descriptor_digest=sha256:' "$out_dir/scan.txt"
rg -q '^config_scan_plan_digest=sha256:' "$out_dir/scan.txt"

set +e
RULEOPS_INCLUDE=src/does-not-exist/** ./rf scan \
  --repo-root "$out_dir/java-repo" \
  --rule-id corp.java.bootstrap-empty-scope \
  --resolver "$repo_root/target/debug/erf-fake-resolver" \
  --host "$repo_root/target/debug/erf-fake-host" > "$out_dir/scan-empty-scope.txt" 2>&1
empty_scope_rc=$?
set -e
[ "$empty_scope_rc" -eq 2 ]
rg -q 'PLAN_INVALID|project config selected no resolver files' "$out_dir/scan-empty-scope.txt"

input_identity="$(sha256sum contracts/http/scan-output-v1.yaml docs/adr/0159-project-config-and-bootstrap-doctor-boundary.md runtime/project-config/src/lib.rs runtime/project-config/src/main.rs runtime/rf-cli/src/main.rs runtime/rf-runner/src/main.rs scripts/bootstrap-check.sh scripts/cli-surface-check.sh rf runtime/scan-output/src/lib.rs | sha256sum | awk '{print $1}')"
cat > "$out_dir/bootstrap.md" <<EOF
# Project Config and Bootstrap Doctor Evidence

- Acceptance command: scripts/bootstrap-check.sh $out_dir
- Contract: contracts/http/scan-output-v1.yaml
- Decision: docs/adr/0159-project-config-and-bootstrap-doctor-boundary.md
- Input identity: sha256:$input_identity
- Evidence level: EXECUTABLE
- Config: .ruleops/config.yaml, closed YAML subset, CLI > environment > project > defaults
- Scan identity: effective config, WorkspaceDescriptor and config ScanPlan digests
- Authority: configuration/doctor preflight only; no Gate, Trust initialization, source upload or SCM mutation

| Boundary | Evidence | Result |
| --- | --- | --- |
| Java workspace detection and config generation | init.txt, config-validate.txt | PASS |
| Repeated config identity is deterministic | config-show-1.txt, config-show-2.txt | PASS |
| CLI/environment precedence is explicit | config-env.txt, config-cli.txt | PASS |
| Duplicate/unsupported configuration fails closed | duplicate-config.txt, unsupported-init.txt | PASS |
| Missing config/dependency/cache/trust/scan states are classified | doctor-*.txt | PASS |
| Doctor applies Offline/Standalone precedence and rejects path symlinks | doctor-offline-env.txt, doctor-standalone-cli.txt, doctor-symlink-path.txt | PASS |
| Cache digest is rechecked and corrupt object is rejected | doctor-corrupt-cache.txt | PASS |
| Effective scope cannot silently widen to an empty/unconfigured Resolver scope | scan-empty-scope.txt | PASS |
| Scan output carries the three project identity digests | scan.txt | PASS |

Connected cache misses are reported as server-fetch-required; Offline/Standalone misses or
trust gaps remain inconclusive. A cache hit never substitutes for delivery verification of
signature, freshness, revocation snapshot or execution identity.
EOF

echo "bootstrap=PASS evidence=$out_dir/bootstrap.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
