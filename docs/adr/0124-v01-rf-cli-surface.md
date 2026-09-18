# ADR-0124: Publish a thin V0.1 `rf` CLI surface

- Status: Accepted
- Date: 2026-09-18
- Scope: V0.1 local CLI authoring and scan entrypoints

## Context

Architecture §27 defines an `rf` command surface, while the repository had
separate Java and Rust entrypoints (`authoring-validate.sh`,
`candidate-compile.sh`, `candidate-verify.sh`, coverage/report scripts and
`erf-cli`). A caller had no single, tested route for the implemented slice, and
an unimplemented command could be mistaken for an authority path.

## Decision

Publish executable `./rf` as a thin dispatcher. It exposes Standard/Rule
validation, Candidate generation/test, Coverage analysis, Pilot report and
Runtime scan. It delegates to existing verified entrypoints and contains no
duplicate authoring, verification or gate logic.

Release promotion/revocation, gate authority, feedback submission and
exception requests fail with exit code 2 and do not mutate governance state.
The dispatcher therefore exposes the current capability perimeter without
claiming the future SCM or human-approval surfaces.

Runtime scans without a Candidate require an explicit `--rule-id`; when
`--candidate` is supplied, the validated Candidate manifest is authoritative
and combining both identity inputs fails closed. The runner reports the
effective identity in its result so downstream evidence can bind findings to
the selected rule.

The process-level execution semantics are defined separately in
ADR-0126: a complete scan returns zero even when GateDecision is `FAIL`, while
incomplete execution returns three and trust verification failures return
four.

## Consequences

- Local users have one discoverable, deterministic CLI entrypoint for the
  implemented V0.1 path.
- The CLI remains a composition layer; each delegated command keeps its own
  evidence and failure semantics.
- Packaging/installing the command outside this checkout and enabling external
  authority remain future work.

## Verification

Run `scripts/cli-surface-check.sh .scratch/cli-surface` and then
`bash scripts/check.sh`; the repository gate also executes the Phase 2/3
acceptance suites and the Phase 4 Shadow-only aggregate.
