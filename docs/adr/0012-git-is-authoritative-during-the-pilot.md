---
status: accepted
---

# Git is authoritative during the pilot

During the pilot, Standard, Scenario, RuleIntent, Policy and release manifests are governed in Git through reviewable changes; binary artifacts and large evidence live in an artifact repository and are referenced by digest. CLI and API workflows create Git changes instead of dual-writing a database, avoiding split authority until a later server migration is explicitly designed.
