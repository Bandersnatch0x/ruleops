---
status: superseded by ADR-0027
---

# Use a Java modular monolith for V0.x

V0.x will be a Java 21 modular monolith with backend-neutral internal ports and a thin `rf` launcher, rather than a Go/Rust runner plus JVM sidecars or an early multi-service design. This minimizes PMD/SpotBugs integration and delivery cost within the 12-week pilot while preserving explicit module seams; untrusted Factory Worker execution remains a separate process boundary.
