---
status: accepted
---

# Local canonical result and baseline preview

Phase 2 的本地 Gate preview 与未来 Trusted Gate 共用同一组语义边界：
`CanonicalFindingSet v1` 按 fingerprint 稳定排序并去重，
`SemanticResultDigest` 只编码 fingerprint、rule identity、repository-relative
path 与 semantic anchor，不编码 message 等展示字段。这样渲染文案变化不会伪造
新的语义结果。

Baseline 只产生 `New` 或 `Historical` 分类，Exception 只对精确 fingerprint
产生 `Exempted` 决策；二者均不得删除 Finding 或改变 fingerprint。任何 required
unit 缺失、失败或 observation error 都优先产生 `INCONCLUSIVE`，不能被 baseline
或 exception 绕过。Policy 通过显式 blocking rule identity 集合决定哪些新的、未
豁免 finding 能产生 `FAIL`；没有该集合时，本地兼容策略将所有 finding 视为
blocking。Phase 2 的输入仍是 unsigned、`LOCAL_UNATTESTED`/`SHADOW`，不具备
protected branch 阻断权。
