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
for tool in mvn cargo python3 rg openssl jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: $tool not found. Install it or provide ~/.local/opt/erf-env.sh" >&2
    exit 2
  fi
done
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
echo "== [Phase 2] Independent Verification Worker acceptance =="
bash "$REPO_ROOT/scripts/verification-worker-check.sh" "$REPO_ROOT/.scratch/verification-worker-check"
echo "== [Phase 2] Five-rule Java local exit pack =="
"$REPO_ROOT/scripts/phase2-java-local-demo.sh" "$REPO_ROOT/.scratch/phase2-java-local-demo"
echo "== [Delivery] Deterministic multi-rule RuleBundle pack =="
"$REPO_ROOT/scripts/rule-bundle-pack-check.sh" "$REPO_ROOT/.scratch/rule-bundle-pack-check"
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
echo "== [Pilot] Evidence package and adjudication boundary =="
"$REPO_ROOT/scripts/pilot-evidence-check.sh" "$REPO_ROOT/.scratch/pilot-evidence-check"
echo "== [Decision] Fast Decision Evidence shadow boundary =="
"$REPO_ROOT/scripts/fast-decision-check.sh" "$REPO_ROOT/.scratch/fast-decision-check"
echo "== [Pilot] Author active-time baseline definition =="
"$REPO_ROOT/scripts/pilot-author-cost-baseline-check.sh" "$REPO_ROOT/.scratch/pilot-author-cost-baseline-check"
echo "== [Pilot] Observation contract and observer seam =="
"$REPO_ROOT/scripts/pilot-observation-check.sh" "$REPO_ROOT/.scratch/pilot-observation-check"
echo "== [Pilot] Shadow MVP delivery manifest =="
"$REPO_ROOT/scripts/pilot-delivery-check.sh" "$REPO_ROOT/.scratch/pilot-delivery-check"
echo "== [CLI] V0.1 rf command surface =="
"$REPO_ROOT/scripts/cli-surface-check.sh" "$REPO_ROOT/.scratch/cli-surface-check"
echo "== [Bootstrap] Project config and doctor boundary =="
"$REPO_ROOT/scripts/bootstrap-check.sh" "$REPO_ROOT/.scratch/bootstrap-check"
echo "== [CLI] Installable CI-neutral rf scan =="
"$REPO_ROOT/scripts/ci-neutral-scan-check.sh" "$REPO_ROOT/.scratch/ci-neutral-scan-check"
echo "== [Runtime] Production Java Runtime Pack =="
"$REPO_ROOT/scripts/production-runtime-pack-check.sh" "$REPO_ROOT/.scratch/production-runtime-pack-check"
echo "== [Runtime] Signed RuleBundle delivery CLI vertical =="
"$REPO_ROOT/scripts/delivery-cli-vertical-check.sh" "$REPO_ROOT/.scratch/production-runtime-pack-check"
echo "== [Server] V0.2 HTTP/API contract =="
"$REPO_ROOT/scripts/server-contract-check.sh" "$REPO_ROOT/.scratch/server-contract-check"
echo "== [Server] V0.2 persistence/recovery boundary =="
"$REPO_ROOT/scripts/server-persistence-check.sh" "$REPO_ROOT/.scratch/server-persistence-check"
echo "== [Quality] Feature evidence status contract =="
"$REPO_ROOT/scripts/feature-evidence-check.sh" "$REPO_ROOT/.scratch/feature-evidence-check"
echo "== [Delivery] RuleBundle distribution/cache/offline contract =="
"$REPO_ROOT/scripts/bundle-delivery-check.sh" "$REPO_ROOT/.scratch/bundle-delivery-check"
echo "== [Delivery] RuleOps Server to rf-runner vertical closure =="
"$REPO_ROOT/scripts/bundle-delivery-e2e-check.sh" "$REPO_ROOT/.scratch/bundle-delivery-e2e-check"
echo "== [Clients] V0.3 thin-client action contract =="
"$REPO_ROOT/scripts/thin-client-check.sh" "$REPO_ROOT/.scratch/thin-client-check"
echo "== [Clients] Thin Client to Server submission transport =="
"$REPO_ROOT/scripts/thin-client-submit-check.sh" "$REPO_ROOT/.scratch/thin-client-submit-check"
echo "== [Clients] UNKNOWN submission status recovery =="
"$REPO_ROOT/scripts/submission-status-check.sh" "$REPO_ROOT/.scratch/submission-status-check"
echo "== [Clients] Rich Explain 与权威状态边界 =="
"$REPO_ROOT/scripts/rich-explain-check.sh" "$REPO_ROOT/.scratch/rich-explain-check"
echo "== [Clients] IDE Thin Client 双端纯核与跨端一致性 =="
"$REPO_ROOT/scripts/ide-client-check.sh" "$REPO_ROOT/.scratch/ide-client-check"
echo "== [Governance] requestId 内容寻址跨语言一致性 =="
"$REPO_ROOT/scripts/request-identity-check.sh" "$REPO_ROOT/.scratch/request-identity-check"
echo "== [CLI] Consumable scan result, diagnostics and progress =="
"$REPO_ROOT/scripts/scan-output-check.sh" "$REPO_ROOT/.scratch/scan-output-check"
echo "== [Authority] Status Publisher publication artifact =="
"$REPO_ROOT/scripts/status-publication-check.sh" "$REPO_ROOT/.scratch/status-publication-check"
echo "== [Authority] Status Adapter deployment/permission contract =="
"$REPO_ROOT/scripts/status-adapter-deployment-check.sh" "$REPO_ROOT/.scratch/status-adapter-deployment-check"
echo "== [Trust] Persisted TrustCheckpoint handoff =="
"$REPO_ROOT/scripts/trust-checkpoint-check.sh" "$REPO_ROOT/.scratch/trust-checkpoint-check"
echo "== [Runner] Git ChangedSet scope =="
"$REPO_ROOT/scripts/git-change-scope-check.sh" "$REPO_ROOT/.scratch/git-change-scope-check"
echo "== [Runner] Java affected scope closure =="
"$REPO_ROOT/scripts/affected-scope-check.sh" "$REPO_ROOT/.scratch/affected-scope-check"
echo "== [Runner] Frozen incremental ScanPlan and cache =="
"$REPO_ROOT/scripts/scan-plan-check.sh" "$REPO_ROOT/.scratch/scan-plan-check"
echo "== [Pilot] Incremental fallback and performance baseline =="
"$REPO_ROOT/scripts/incremental-baseline-check.sh" "$REPO_ROOT/.scratch/incremental-baseline-check"
echo "== [Security] Threat model =="
"$REPO_ROOT/scripts/threat-model-check.sh" "$REPO_ROOT/.scratch/threat-model-check"
echo "== [Phase 4] Aggregated Shadow pilot acceptance =="
"$REPO_ROOT/scripts/phase4-pilot-report.sh" "$REPO_ROOT/.scratch/phase4-pilot-report"
echo "== [RuleOps Center] End-to-end closure =="
"$REPO_ROOT/scripts/ruleops-center-check.sh" "$REPO_ROOT/.scratch/ruleops-center-check"

echo "check: all sides green"
