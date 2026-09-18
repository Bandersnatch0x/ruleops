---
status: accepted
---

# Keep Gate decisions strictly three-state

GateDecision remains exactly `PASS`, `FAIL` or `INCONCLUSIVE`; degraded coverage is structured metadata on `PASS` only when Policy explicitly marks the affected checks non-required or advisory. Any incomplete required analysis is `INCONCLUSIVE`, while configuration and trust failures remain error classifications rather than additional GateDecision states.
