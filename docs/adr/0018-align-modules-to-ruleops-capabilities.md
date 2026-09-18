---
status: accepted
---

# Align modules to RuleOps capabilities

The modular monolith is divided by lifecycle capability—contracts, coverage, authoring, factory, verification, release, runtime, gate and adapters—rather than by global controller/service/repository layers. Each capability may layer internally, but cross-module access goes through explicit contracts so ownership and invariants follow the RuleOps lifecycle.
