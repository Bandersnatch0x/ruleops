# ADR-0128: Enforce byte-identical Java build artifacts

- Status: Accepted
- Date: 2026-09-18
- Scope: Control Plane build reproducibility

## Context

The repository pins the Java toolchain-facing plugin versions and
`project.build.outputTimestamp` so an unchanged working tree can be rebuilt
deterministically. A prose claim alone does not detect a future plugin,
resource, or packaging change that introduces nondeterministic bytes.

## Decision

Add `scripts/reproducible-java-build-check.sh` to run `mvn clean verify` twice
on the same working tree. The check copies every JAR produced under
`control-plane/**/target`, compares the path set and SHA-256 manifest, and then
uses byte comparison for every matching artifact. Any build failure, missing
artifact, path change, digest change, or byte change fails closed. Logs and the
artifact manifest are retained under `.scratch/`.

The repository gate runs this check after the ordinary Java verification. The
check proves packaging reproducibility only; it does not assert trusted runtime
identity, sign an artifact, or grant production authority.

## Consequences

- Changes to Java packaging determinism become visible in the repository gate.
- The evidence is independently inspectable without committing generated JARs.
- The check adds two clean Java builds to the gate, trading runtime for a
  concrete reproducibility guarantee.

## Verification

```text
scripts/reproducible-java-build-check.sh .scratch/reproducible-java-build
bash scripts/check.sh
```
