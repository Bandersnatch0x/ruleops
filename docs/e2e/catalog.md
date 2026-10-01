# ERF E2E 用例目录（自动生成，源：scripts/e2e/e2e-cases.json）

状态列由 `scripts/e2e/run-e2e.sh` 生成；本目录仅描述用例。

| 用例 ID | 标题 | 生命周期 | 形态 | 域 | 类型 | 期望 rc | 入口 | 证据 | 离线替身 |
|---|---|---|---|---|---|---:|---|---|---|
| E2E-SPINE-001 | 全生命周期主线 L0→L8（真实产物串联） | SPINE | CLI+Server+Adapter+IDE+MCP | core | run | 0 | `scripts/e2e/spine.sh` | spine.json | — |
| E2E-BLD-001 | Java 模块化单体构建/测试 | L0-authoring | ControlPlane | core | run | 0 | `pom.xml` | (maven stdout) | — |
| E2E-BLD-002 | Rust runtime workspace 测试 | L2-packaging | Runner | core | run | 0 | `runtime/` | (cargo stdout) | — |
| E2E-L0-001 | CLI 命令面 + 创作→扫描最小纵切（standard/rule validate、generate、test、init、scan） | L0-authoring | CLI | core | run | 0 | `scripts/cli-surface-check.sh` | cli-surface/ | — |
| E2E-L0-002 | 创作编排与检索基线（Ontology/IR/Orchestrator/Retrieval） | L0-authoring | Factory | core | run | 0 | `scripts/authoring-capability-check.sh` | authoring-capability/ | — |
| E2E-L0-003 | Java Standard 库存完整性 | L0-authoring | Standards | core | run | 0 | `scripts/standard-inventory-check.sh` | standard-inventory/ | — |
| E2E-L0-004 | 规范→规则抽取（原文样例/分母/摘要/needs_review/拒覆盖） | L0-authoring | Factory | core | run | 0 | `scripts/standard-rule-extract-test.py` | standard-extract/ | — |
| E2E-L0-005 | Pilot Success Contract 边界 | L0-authoring | Governance | core | run | 0 | `scripts/pilot-success-contract-check.sh` | pilot-success-contract/ | — |
| E2E-L0-006 | Pilot Success Contract 状态校验 | L0-authoring | Governance | core | run | 0 | `scripts/pilot-success-contract-approval-check.sh` | pilot-success-contract-approval/ | — |
| E2E-L0-007 | Phase 0 候选/模板选择证据 | L0-authoring | Governance | core | run | 0 | `scripts/phase0-template-selection-check.sh` | phase0-template-selection/ | — |
| E2E-L0-008 | 代表 Java 仓库清单 | L0-authoring | Pilot | core | run | 0 | `scripts/representative-java-repositories-check.sh` | representative-java-repositories/ | — |
| E2E-L1-001 | Phase 2 独立本地切片验收（Gate 判定） | L1-verification | Runner | core | run | 0 | `scripts/phase2-independent-acceptance.sh` | phase2-independent-acceptance/ | — |
| E2E-L1-002 | 独立 Verification Worker 验收 | L1-verification | Factory | core | run | 0 | `scripts/verification-worker-check.sh` | verification-worker/ | — |
| E2E-L1-003 | 五条 Java 规则本地出口包 | L1-verification | Runner | core | run | 0 | `scripts/phase2-java-local-demo.sh` | phase2-java-local-demo/ | — |
| E2E-L2-001 | 确定性多规则 RuleBundle 打包 | L2-packaging | SignedBundle | core | run | 0 | `scripts/rule-bundle-pack-check.sh` | rule-bundle-pack/ | — |
| E2E-L3-004 | 生产 Java Runtime Pack（签名发布形态） | L3-delivery | BackendRuntimePack | core | run | 0 | `scripts/production-runtime-pack-check.sh` | — | — |
| E2E-L2-002 | 签名 RuleBundle delivery CLI 纵切（复用 Runtime Pack 产物） | L2-packaging | SignedBundle | core | run | 0 | `scripts/delivery-cli-vertical-check.sh` | — | — |
| E2E-L2-003 | Java 可复现洁净构建（两次构件身份一致） | L2-packaging | ControlPlane | core | run | 0 | `scripts/reproducible-java-build-check.sh` | reproducible-java-build/ | — |
| E2E-L3-001 | RuleBundle 分发/缓存/离线契约 | L3-delivery | SignedBundle | core | run | 0 | `scripts/bundle-delivery-check.sh` | bundle-delivery/ | — |
| E2E-L3-002 | RuleOps Server → rf-runner 交付闭环 | L3-delivery | Server+Runner | core | run | 0 | `scripts/bundle-delivery-e2e-check.sh` | bundle-delivery-e2e/ | — |
| E2E-L3-003 | 可安装 Release 打包 + CI-neutral scan | L3-delivery | Release | core | run | 0 | `scripts/ci-neutral-scan-check.sh` | ci-neutral-scan/ | — |
| E2E-L4-001 | RuleOps Server HTTP/API 契约 | L4-server | Server | core | run | 0 | `scripts/server-contract-check.sh` | server-contract/ | — |
| E2E-L4-002 | RuleOps Server 持久化/重启恢复 | L4-server | Server | core | run | 0 | `scripts/server-persistence-check.sh` | server-persistence/ | — |
| E2E-L4-003 | 多端取规则（真实 Java Server + rf registry list/show） | L4-server | CLI+Server | core | run | 0 | `scripts/registry-client-check.py` | registry/ | — |
| E2E-L4-004 | 提交状态 UNKNOWN 恢复 | L4-server | CLI+Server | core | run | 0 | `scripts/submission-status-check.sh` | submission-status/ | — |
| E2E-L4-005 | 薄客户端 → Server 提交传输 | L4-server | ThinClient+Server | core | run | 0 | `scripts/thin-client-submit-check.sh` | thin-client-submit/ | — |
| E2E-L5-001 | 项目配置与 doctor 边界 | L5-execution | CLI | core | run | 0 | `scripts/bootstrap-check.sh` | bootstrap/ | — |
| E2E-L5-002 | Java Backend Host 端到端（PMD 真实驱动） | L5-execution | BackendHost | core | run | 0 | `scripts/java-host-e2e.sh` | java-host-e2e/ | — |
| E2E-L5-003 | scan 可消费结果/诊断/进度 | L5-execution | CLI | core | run | 0 | `scripts/scan-output-check.sh` | scan-output/ | — |
| E2E-L5-004 | Git ChangedSet 作用域 | L5-execution | Runner | core | run | 0 | `scripts/git-change-scope-check.sh` | git-change-scope/ | — |
| E2E-L5-005 | Java affected scope 闭合 | L5-execution | Runner | core | run | 0 | `scripts/affected-scope-check.sh` | affected-scope/ | — |
| E2E-L5-006 | 冻结增量 ScanPlan 与缓存 | L5-execution | Runner | core | run | 0 | `scripts/scan-plan-check.sh` | scan-plan/ | — |
| E2E-L5-007 | 增量 fallback 与性能基线 | L5-execution | Pilot | core | run | 0 | `scripts/incremental-baseline-check.sh` | incremental-baseline/ | — |
| E2E-L6-001 | Trusted CI 验收 | L6-gate | Runner | core | run | 0 | `scripts/phase3-trusted-ci.sh` | phase3-trusted-ci/ | — |
| E2E-L6-002 | Real Runner handoff 验收 | L6-gate | Runner | core | run | 0 | `scripts/phase3-trusted-scan.sh` | phase3-trusted-scan/ | — |
| E2E-L6-003 | 失败证据：不完整扫描 | L6-gate | Runner | core | run | 0 | `scripts/phase3-incomplete-evidence.sh` | phase3-incomplete/ | — |
| E2E-L6-004 | 失败证据：超时 | L6-gate | Runner | core | run | 0 | `scripts/phase3-timeout-evidence.sh` | phase3-timeout/ | — |
| E2E-L6-005 | 可重复性证据 | L6-gate | Runner | core | run | 0 | `scripts/phase3-repeatability.sh` | phase3-repeat/ | — |
| E2E-L6-006 | TrustCheckpoint 持久化交接 | L6-gate | Runner | core | run | 0 | `scripts/trust-checkpoint-check.sh` | trust-checkpoint/ | — |
| E2E-L6-007 | 威胁模型契约 | L6-gate | Security | core | run | 0 | `scripts/threat-model-check.sh` | threat-model/ | — |
| E2E-L6-008 | requestId 内容寻址跨语言一致性 | L6-gate | Multi | core | run | 0 | `scripts/request-identity-check.sh` | request-identity/ | — |
| E2E-L7-001 | Status Publisher 发布产物 | L7-publish | Publisher | core | run | 0 | `scripts/status-publication-check.sh` | status-publication/ | — |
| E2E-L7-002 | Status Adapter 部署/权限契约 | L7-publish | Adapter | core | run | 0 | `scripts/status-adapter-deployment-check.sh` | status-adapter-deployment/ | — |
| E2E-L7-003 | SCM Status Adapter（GitHub 状态发布，离线 fake provider） | L7-publish | SCMAdapter | core | run | 0 | `scripts/scm-status-adapter-github-check.py` | (stdout json) | — |
| E2E-L8-001 | Rich Explain 与权威状态边界 | L8-consume | CLI | core | run | 0 | `scripts/rich-explain-check.sh` | rich-explain/ | — |
| E2E-L8-002 | 薄客户端动作契约 | L8-consume | ThinClient | core | run | 0 | `scripts/thin-client-check.sh` | thin-client/ | — |
| E2E-L8-003 | IDE 双端纯核与跨端一致性（VS Code ↔ IntelliJ） | L8-consume | IDE | core | run | 0 | `scripts/ide-client-check.sh` | ide-client/ | — |
| E2E-L8-004 | VS Code 扩展纯逻辑核单测 | L8-consume | IDE | core | run | 0 | `clients/vscode/test/core.test.js` | (node test stdout) | — |
| E2E-L8-005 | IntelliJ 插件纯逻辑核自测 | L8-consume | IDE | core | run | 0 | `clients/intellij/selftest/io/ruleops/ide/core/CoreSelfTest.java` | (selftest stdout) | — |
| E2E-L8-006 | 后端 MCP server（stdio/HTTP/工具白名单/边界） | L8-consume | MCP | core | run | 0 | `clients/mcp/test/` | (node test stdout) | — |
| E2E-L8-007 | WebMCP 查看面（页面注册/HTTP 安全边界） | L8-consume | WebMCP | core | run | 0 | `clients/mcp/webmcp/test/` | (node test stdout) | — |
| E2E-L8-008 | agent commit hook 真实 PMD 消费（违规阻断/合规放行/非 Java 不适用） | L8-consume | Hook | core | run | 0 | `scripts/agent-commit-hook-test.py` | hook/results.json | — |
| E2E-L9-001 | Pilot 指标基线 | L9-observe | Pilot | core | run | 0 | `scripts/pilot-metrics-baseline-check.sh` | pilot-metrics-baseline/ | — |
| E2E-L9-002 | 作者主动时间基线 | L9-observe | Pilot | core | run | 0 | `scripts/pilot-author-cost-baseline-check.sh` | pilot-author-cost-baseline/ | — |
| E2E-L9-003 | Pilot 证据包与裁决边界 | L9-observe | Pilot | core | run | 0 | `scripts/pilot-evidence-check.sh` | pilot-evidence/ | — |
| E2E-L9-004 | Fast Decision shadow 边界 | L9-observe | Pilot | core | run | 0 | `scripts/fast-decision-check.sh` | fast-decision/ | — |
| E2E-L9-005 | Observation 契约与 observer seam | L9-observe | Pilot | core | run | 0 | `scripts/pilot-observation-check.sh` | pilot-observation/ | — |
| E2E-L9-006 | Shadow MVP 交付清单 | L9-observe | Pilot | core | run | 0 | `scripts/pilot-delivery-check.sh` | pilot-delivery/ | — |
| E2E-L9-007 | Phase 4 聚合 Shadow 验收 | L9-observe | Pilot | core | run | 0 | `scripts/phase4-pilot-report.sh` | phase4-pilot-report/ | — |
| E2E-L9-008 | RuleOps Center 端到端闭合 | L9-observe | Center | core | run | 0 | `scripts/ruleops-center-check.sh` | ruleops-center/ | — |
| E2E-L9-009 | Feature Evidence 状态契约 | L9-observe | Governance | core | run | 0 | `scripts/feature-evidence-check.sh` | feature-evidence/ | — |
| E2E-L9-010 | Java PMD/SpotBugs/CI 配置库存 | L9-observe | Inventory | core | run | 0 | `scripts/java-ci-inventory-check.sh` | java-ci-inventory/ | — |
| E2E-L9-011 | 参考 PMD 补充门禁（对当前 tracked Java 的真实扫描） | L9-observe | ReferencePMD | reference-pmd | run | 1 | `scripts/plugin-ruleset-check.py` | plugin.json | — |
| E2E-L9-012 | 参考 PMD 门禁工具正负边界 | L9-observe | ReferencePMD | reference-pmd | run | 0 | `scripts/plugin-ruleset-check-test.py` | plugin-tests/ | — |
| E2E-L9-013 | Pilot 本地离线合成观察（观测 seam 的离线替身） | L9-observe | Pilot | core | run | 0 | `scripts/pilot-local-simulation.sh` | pilot-local-simulation/local-simulation.md | — |
| E2E-MAN-001 | 真实 IDE UI 交互（VS Code/IntelliJ 内导航→解释→草稿→提交） | L8-consume | IDE | core | manual | — | `docs/e2e/catalog.md#manual` | — | E2E-L8-003, E2E-L8-004, E2E-L8-005 |
| E2E-MAN-002 | 真实访问窗口 Pilot 观察 | L9-observe | Pilot | core | manual | — | `scripts/pilot-live-observation.sh` | — | E2E-L9-013, E2E-L9-007 |
| E2E-MAN-003 | 真实 GitHub 状态发布（联网 + 受控凭证） | L7-publish | SCMAdapter | core | manual | — | `scripts/scm-status-adapter-github.sh` | — | E2E-L7-003 |
| E2E-MAN-004 | 生产可信（TRUSTED）Gate | L6-gate | Governance | core | manual | — | `docs/runbooks/` | — | — |
