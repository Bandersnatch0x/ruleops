# Java PMD/SpotBugs and CI Configuration Inventory

- Status: current repository inventory, not a live CI attestation
- Date: 2026-09-18
- Trust: `LOCAL_UNATTESTED` / `SHADOW`
- Authority: no protected-branch enforcement

## Java Host and backends

The Java Host is defined by `backend-hosts/java/pom.xml` with artifact
`erf-java-host`, main class `io.ruleops.javahost.JavaHostMain`, and compiler
release 21. The declared compatibility source levels are 11, 17, and 21.

| Backend | Version | Mode | Required capabilities | Verification entrypoint |
| --- | --- | --- | --- | --- |
| PMD | 7.9.0 | source | `java.ast`, `java.symbol-resolution` | `scripts/java-host-e2e.sh` |
| SpotBugs | 4.9.8 | bytecode | `java.bytecode` | `scripts/java-host-e2e.sh` |

The exact versions are cross-checked against
`specs/compatibility/java-backend-matrix.yaml` and the Java Host POM by
`scripts/java-ci-inventory-check.sh`.

## Executable repository checks

- `bash scripts/check.sh`: Maven, Cargo, Java Host E2E, Phase 2/3 acceptance
  (including crash/timeout/repeatability evidence), Phase 4 Shadow pilot
  composition, and inventory checks;
- `scripts/java-host-e2e.sh`: Java Host compatibility and Rust-client E2E;
- `scripts/phase2-java-shadow.sh`: representative local Shadow scan;
- `scripts/check-java-compatibility.sh`: POM/matrix version and capability
  consistency;
- `scripts/phase4-pilot-report.sh`: current Shadow-only pilot acceptance
  composition.

The checkout has no `.github` repository CI configuration. These local
entrypoints therefore document executable repository checks, not an external
provider's status check or branch-protection result.

## Explicit boundary

The current trusted test path is linux/amd64 and remains Shadow-only. The
inventory contains no SCM credential, repository write, source-upload, network
adapter, or external status-publisher capability. Other PMD/SpotBugs versions,
other trusted platforms, and external CI integration remain untested.

Run:

```text
bash scripts/java-ci-inventory-check.sh .scratch/java-ci-inventory
```
