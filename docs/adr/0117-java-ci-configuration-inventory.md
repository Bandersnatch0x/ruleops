# ADR-0117: Inventory Java engine and repository-gate configuration before live CI

- Status: Accepted
- Date: 2026-09-18
- Scope: MVP Java PMD/SpotBugs and CI configuration discovery

## Context

The MVP discovery plan requires a current PMD/SpotBugs/CI configuration
inventory. The repository already has a Java Host, a compatibility matrix, and
local gate scripts, but it does not contain an external CI provider
configuration. Treating local PASS output as external CI status would create a
false trust claim.

## Decision

Record the exact Java Host POM and compatibility matrix facts in
`specs/compatibility/java-ci-inventory.yaml`: Java compiler release 21, declared
source levels 11/17/21, PMD 7.9.0 source mode, SpotBugs 4.9.8 bytecode mode, and
their required capabilities. Record the executable local checks and explicitly
mark the current checkout's external CI configuration and status publisher as
absent.

`java-ci-inventory-check.sh` cross-checks the machine inventory against the POM,
the backend matrix, and the required entrypoint files. A future `.github`
configuration or external status integration must update the inventory before
it can be treated as an observed capability.

The trust boundary remains linux/amd64 and Shadow-only. No new backend,
network, SCM, credential, source-upload, or protected-branch path is introduced.

## Consequences

- Configuration drift between POM, matrix, and inventory fails the repository
  gate.
- Local executable checks are discoverable and reproducible without being
  mislabeled as provider-backed CI evidence.
- Other engine versions, platforms, external CI, and SCM integration remain
  explicitly untested.

## Verification

```text
bash scripts/java-ci-inventory-check.sh .scratch/java-ci-inventory
bash scripts/check.sh
```

The generated evidence identifies exact versions and the untested boundary.
