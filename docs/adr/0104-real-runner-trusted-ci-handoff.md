---
status: accepted
---

# Hand real Runner scans into Trusted CI

The trusted CI boundary must consume the actual atomic `rf-runner::scan`
result, not a process exit code or a separately reconstructed finding list.
`erf-trusted-ci` therefore keeps the Runner responsible for resolution,
execution and staging, derives the Gate preview from its `ScanResult`, and
only then signs an attestation for `TrustedContextVerifier` and the
transport-neutral Status Publisher.

The `phase3-trusted-scan.sh` harness runs two scans of the same generated
snapshot and one injected host-crash scan. It checks equal semantic digests on
the successful runs and requires the failed run to remain `INCONCLUSIVE` with
an `ActionRequired` status. Workspace identity is measured by the resolver and
carried by `ScanResult`; no SCM credential or publication adapter is included.
