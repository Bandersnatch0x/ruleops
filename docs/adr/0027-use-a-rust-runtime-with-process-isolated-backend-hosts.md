---
status: accepted
---

# Use a Rust Runtime with process-isolated Backend Hosts

The distributed `rf` CLI and `rf-runner` are implemented in Rust and own bundle verification, caching, planning, process orchestration, normalization, Completeness, Baseline and Gate evaluation. Analysis engines run behind a small versioned Backend Host interface in separate processes—initially a Java host for PMD and SpotBugs, with future JavaScript or other hosts—while the Control Plane and Factory remain a Java 21 lifecycle-aligned modular monolith.

This supersedes ADR-0011: the extra protocol and toolchain cost is accepted to keep the execution Runtime independent of any analysis language or VM and to support future non-JVM Backends without rewriting its trusted core.
