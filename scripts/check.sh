#!/usr/bin/env bash
# Repo-level check: builds and tests both language sides of ruleops.
# - Java side: Java 21 modular monolith (control-plane/*) via Maven, requires JDK >= 21
# - Rust side: runtime workspace (runtime/*) via Cargo, version pinned by rust-toolchain.toml
# Optional environment file: ~/.local/opt/erf-env.sh
#   contract: exports JAVA_HOME, PATH additions, and optionally RUSTFLAGS
#   (e.g. `-C linker=...` when no system C toolchain exists). Loaded if present.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi

if ! command -v java >/dev/null 2>&1; then
  echo "error: java not found. Set JAVA_HOME to a JDK >= 21 or provide ~/.local/opt/erf-env.sh" >&2
  exit 2
fi
JAVA_MAJOR="$(java -version 2>&1 | head -1 | sed -n 's/.*version "\([0-9][0-9]*\)\..*/\1/p')"
if [ -z "$JAVA_MAJOR" ] || [ "$JAVA_MAJOR" -lt 21 ]; then
  echo "error: JDK >= 21 required, got: $(java -version 2>&1 | head -1)" >&2
  exit 2
fi

echo "== [Java] mvn verify (modular monolith) =="
(cd "$REPO_ROOT" && mvn -q verify)
echo "== [Java] Reproducible clean-build artifacts =="
"$REPO_ROOT/scripts/reproducible-java-build-check.sh" "$REPO_ROOT/.scratch/reproducible-java-build"
echo "== [Rust] cargo test (runtime workspace) =="
(cd "$REPO_ROOT" && cargo test --quiet --workspace --all-targets)

echo "== [E2E] Java Backend Host driven by the Rust client =="
"$REPO_ROOT/scripts/java-host-e2e.sh"
echo "== [Phase 2] Independent local-slice acceptance =="
"$REPO_ROOT/scripts/phase2-independent-acceptance.sh" "$REPO_ROOT/.scratch/phase2-independent-acceptance"
echo "== [Phase 2] Five-rule Java local exit pack =="
"$REPO_ROOT/scripts/phase2-java-local-demo.sh" "$REPO_ROOT/.scratch/phase2-java-local-demo"
echo "== [Phase 3] Trusted CI acceptance =="
"$REPO_ROOT/scripts/phase3-trusted-ci.sh" "$REPO_ROOT/.scratch/phase3-trusted-ci"
echo "== [Phase 3] Real Runner handoff acceptance =="
"$REPO_ROOT/scripts/phase3-trusted-scan.sh" "$REPO_ROOT/.scratch/phase3-trusted-scan"
echo "== [Phase 3] Failure and repeatability evidence =="
"$REPO_ROOT/scripts/phase3-incomplete-evidence.sh"
"$REPO_ROOT/scripts/phase3-timeout-evidence.sh"
"$REPO_ROOT/scripts/phase3-repeatability.sh"
echo "== [Authoring] Orchestrator and retrieval baseline =="
"$REPO_ROOT/scripts/authoring-capability-check.sh" "$REPO_ROOT/.scratch/authoring-capability-check"
echo "== [Authoring] Java Standard inventory =="
bash "$REPO_ROOT/scripts/standard-inventory-check.sh" "$REPO_ROOT/.scratch/standard-inventory-check"
echo "== [Inventory] Java PMD/SpotBugs and CI configuration =="
bash "$REPO_ROOT/scripts/java-ci-inventory-check.sh" "$REPO_ROOT/.scratch/java-ci-inventory-check"
echo "== [Governance] Pilot Success Contract boundary =="
bash "$REPO_ROOT/scripts/pilot-success-contract-check.sh" "$REPO_ROOT/.scratch/pilot-success-contract-check"
echo "== [Governance] Pilot Success Contract state validation =="
"$REPO_ROOT/scripts/pilot-success-contract-approval-check.sh" "$REPO_ROOT/.scratch/pilot-success-contract-approval-check"
echo "== [Governance] Phase 0 candidate/template selection evidence =="
"$REPO_ROOT/scripts/phase0-template-selection-check.sh" "$REPO_ROOT/.scratch/phase0-template-selection-check"
echo "== [Pilot] Representative Java repository manifest =="
"$REPO_ROOT/scripts/representative-java-repositories-check.sh" "$REPO_ROOT/.scratch/representative-java-repositories-check"
echo "== [Pilot] Metrics baseline definition =="
"$REPO_ROOT/scripts/pilot-metrics-baseline-check.sh" "$REPO_ROOT/.scratch/pilot-metrics-baseline-check"
echo "== [CLI] V0.1 rf command surface =="
"$REPO_ROOT/scripts/cli-surface-check.sh" "$REPO_ROOT/.scratch/cli-surface-check"
echo "== [Security] Threat model =="
"$REPO_ROOT/scripts/threat-model-check.sh" "$REPO_ROOT/.scratch/threat-model-check"
echo "== [Phase 4] Aggregated Shadow pilot acceptance =="
"$REPO_ROOT/scripts/phase4-pilot-report.sh" "$REPO_ROOT/.scratch/phase4-pilot-report"

echo "check: all sides green"
