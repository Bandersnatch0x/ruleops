# Pilot Success Contract Boundary

- Status: `APPROVED`
- Date: 2026-09-18
- Authority: Shadow-only
- Machine asset: `specs/pilot/pilot-success-contract.yaml`

The contract structure covers efficiency, quality, security, operations,
outcomes, sample selection, observation window, and adjudication. The
human-owned fields were approved on 2026-09-18 and are recorded in the machine
asset and GitHub issue #79.

The security invariant is already fixed: a failure injection or incomplete run
cannot count as a pass, and the false-pass threshold is zero. Approval does not
grant protected-branch authority or constitute a live pilot measurement.

Run the boundary check with:

```text
bash scripts/pilot-success-contract-check.sh .scratch/pilot-success-contract
```

The checker accepts an explicit contract file for governed review. `APPROVED`
is accepted only when all fourteen human-owned fields are supplied, the
approval reference is non-empty, the false-pass threshold remains zero, and
authority remains `SHADOW`:

```text
bash scripts/pilot-success-contract-check.sh OUT_DIR CONTRACT_FILE
```

The state regression script validates the current approved asset and a
temporary fixture for the opposite state; it never treats a synthetic fixture
as a human approval or a live pilot result:

```text
bash scripts/pilot-success-contract-approval-check.sh .scratch/pilot-success-contract-approval
```
