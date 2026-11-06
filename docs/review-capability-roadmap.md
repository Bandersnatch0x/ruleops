# Review capability status and roadmap

This page records the review contracts that are present on the current `master`
line. These are **contract and preparation seams**, not a claim that a hosted
review service or a production gate is enabled. Each contract is versioned under
`contracts/http/` and has a focused checker under `scripts/`.

## Landed contracts by priority

| Priority | Contract | What is bound or projected | Boundary that is part of the contract |
| --- | --- | --- | --- |
| **P0** | [`review-run-v1`](../contracts/http/review-run-v1.yaml) | Revision binding (`baseSha`, `headSha`, refs and `bindingDigest`); stale base/head results are rejected. The deterministic scope plan assigns each changed file to exactly one scope (with a single-scope fallback). Finding lifecycle states and human-resolved state are preserved; incremental reviews bind their baseline digest and reviewed head. | Metadata-only trace; source content is forbidden. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; no SCM writeback, release promotion or gate replacement. |
| **P1** | [`review-ledger-v1`](../contracts/http/review-ledger-v1.yaml) | Digest-only EvidenceBundle and resumable ReviewRun, plus an append-only event chain whose event digest binds the previous event and payload. | Stale runs are rejected; source content is forbidden. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; the ledger is persistence/evidence preparation and cannot write SCM or mutate a Gate. |
| **P1** | [`review-telemetry-v1`](../contracts/http/review-telemetry-v1.yaml) | Read-only digest-only projection of queue, revision status, finding states, coverage, usage and audit-chain identities. | Raw prompts and source content are forbidden. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; no SCM writeback, release promotion or gate replacement. |
| **P1** | [`review-dashboard-v1`](../contracts/http/review-dashboard-v1.yaml) | Read-only `GET` snapshot and resumable SSE event transport with cursor/sequence ordering, snapshot/event digests and an append-only event chain. | Write methods/endpoints, source upload and full findings are forbidden. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; no SCM writeback or Gate mutation/replacement. |
| **P2** | [`review-provider-adapter-v1`](../contracts/http/review-provider-adapter-v1.yaml) | Provider-neutral GitHub pull-request/GitLab merge-request draft projection and independent rescore metadata. | Draft-only and metadata-only; raw prompts/title/body, diff and comments are forbidden. No provider network call, SCM writeback, gate replacement or release promotion. |
| **P2** | [`review-rules-config-binding-v1`](../contracts/http/review-rules-config-binding-v1.yaml) | Loads rules and config from one immutable base revision. Rules use exact raw file bytes; config uses canonical UTF-8 JSON bytes. A binding digest covers repository/base identity and both input digests. | Working-tree/current-head fallback, stale identity, and inconsistent digests are rejected. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; no SCM writeback or Gate mutation/replacement. |
| **P2** | [`review-impact-score-v1`](../contracts/http/review-impact-score-v1.yaml) | Optional digest-only Code Graph context and deterministic finding-severity score projection (display-only). | Code Graph is advisory and non-authoritative; missing graph is allowed. The score leaves `GateDecision` unchanged and cannot mutate/replace a Gate or write SCM. |
| **P2** | [`review-preparation-manifest-v1`](../contracts/http/review-preparation-manifest-v1.yaml) | One digest-addressed manifest ties revision binding, scope, run, ledger, telemetry, dashboard, rules/config binding and optional provider/impact projections to the same base/head. | Stale or cross-revision manifests are rejected. `PREPARATION_ONLY` / `LOCAL_UNATTESTED`; no SCM writeback, Gate mutation or release promotion. |

The P0/P1/P2 labels describe delivery priority only. They do not imply that a
higher-priority contract has production authority. Taken together, the review
surface remains `PREPARATION_ONLY` with `LOCAL_UNATTESTED` authority. It does
not upload source content, write back to SCM, mutate or replace a Gate, promote
a release, or publish provider comments. The provider adapter is a projection
seam; it does not call GitHub/GitLab APIs.

## Verification entry points

Each contract checker performs a small schema/fixture assertion and emits a
human-readable evidence file under the supplied output directory:

```text
bash scripts/review-run-contract-check.sh OUT/review-run
bash scripts/review-ledger-check.sh OUT/review-ledger
bash scripts/review-telemetry-check.sh OUT/review-telemetry
bash scripts/review-dashboard-check.sh OUT/review-dashboard
bash scripts/review-provider-adapter-check.sh OUT/review-provider-adapter
bash scripts/review-rules-config-binding-check.sh OUT/review-rules-config-binding
bash scripts/review-impact-score-check.sh OUT/review-impact-score
bash scripts/review-preparation-manifest-check.sh OUT/review-preparation-manifest
bash scripts/review-capability-doc-check.sh OUT/review-capability-doc
```

The preparation-manifest check validates the manifest contract markers and a
digest-bound/stale fixture; the contract declares that all referenced digests
must resolve to one base/head revision. The documentation check is intentionally
small and read-only. It fails if this page, its contract links, or the explicit
authority boundary drifts from the repository contracts.

## Full-check prerequisites

`bash scripts/check.sh` is broader than the review-contract checks. Before it
starts, the script requires:

- Java/JDK **21 or newer** (`java` must report the required major version),
  Maven (`mvn`) and the repository-pinned Rust toolchain (`cargo`,
  `rust-toolchain.toml`, `Cargo.lock`);
- `python3`, `rg`, `openssl` and `jq` on `PATH`;
- a working Java/Maven and Rust/Cargo build environment, including the local
  compiler/linker and any required Maven/Cargo dependency cache or network
  access. The Java host/protobuf phases also need the repository's `protoc`
  toolchain; the environment doctor can report this without building;
- optionally, `~/.local/opt/erf-env.sh` may export `JAVA_HOME`, `PATH` additions
  and `RUSTFLAGS` before the checks run.

The full check runs both language test suites, reproducible-build checks, host
and pilot acceptance scripts, and every checker wired into the current
`scripts/check.sh`; run the review commands above when a review-only check is
needed. It writes evidence below `.scratch/gate/`. A missing tool or unsuitable
JDK fails early with exit code 2; a blocked dependency/build phase is not
evidence of a passing review contract. The review contracts themselves are
local, digest-only checks and do not contact SCM providers or grant production
authority.

## Roadmap boundary

The next work is implementation and integration behind these seams, not a
silent expansion of authority. Any future provider integration, durable service,
Gate integration, SCM writeback or protected-branch enforcement needs its own
reviewed contract, trust/authentication design, and an explicit authority
change. Until then, consumers must preserve `PREPARATION_ONLY` /
`LOCAL_UNATTESTED` and treat all projections as non-authoritative preparation
evidence.
