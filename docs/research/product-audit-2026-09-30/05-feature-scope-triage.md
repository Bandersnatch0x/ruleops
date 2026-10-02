---
title: Feature Scope Triage
date: 2026-09-30
repository: ruleops
head: ae61fc494057a4328dfc51369971ded84f50d2a0
branch: codex/phase1-contract-factory
status: proposal-only
positioning_source: docs/research/product-audit-2026-09-30/01-product-positioning-audit.md
---

# 特性范围分诊

## 1. 特性表

### 1.1 固定定位与评分口径

以下定位逐字取自01报告第一个text引用块，并与当前README开篇比对一致，不另造定位：

```text
Enterprise Rule Factory (ERF) — 面向平台工程与安全/合规工程团队的**可信规则执行层**：用冻结
ScanPlan 的 Completeness、PASS/FAIL/INCONCLUSIVE 三态 Gate 与签名执行证明，取代「0 告警即通过」，
把企业代码规范转化为可验证、可审计、可演进的机器检查资产。
```

沿用02的 **pause to validate** 与03的 Java CI 完整性复核主用户。全部裁决是待用户批准的投资/维护范围建议，**Keep不等于现在开工**；本次不删代码/接口/文档，不推翻ADR-0167的monorepo、experimental与Node协议探针裁定。真实需求与付费仍为hypothesis/E，前序审计不是新客户证词。

粒度：rf每个公开子命令、每个HTTP方法+路由、每个已注册IDE动作逐项列出；三种扫描结果消费格式分列，参数/同义入口并入所在行，避免靠拆布尔参数凑分母。包括安装资产、集成协议与公开的低层工具；纯内部库函数、各检查脚本和测试例程不冒充产品特性。产品Web页面未发现证据；IntelliJ设置服务类不等于已注册设置页面，不虚构页面。

评分均为1–5的审计估计：positioning fit=1去掉不影响定位、3间接支持/可替代、5缺失破坏核心承诺；evidence=1未发现直接需求、2具体一手问题、3独立任务行为、4重复投入、5付费/持续采用；maintenance cost=1薄层、3跨文件与检查、5高协议/构建/运维负担；coupling=1孤立、3多调用者、5共享契约/状态/跨组件。本次所有evidence=1：未访问外部issue，未发现独立客户需求；代码/测试多不能抬高需求分。

体量 `N L* / Gx` 是该特性直接归属源码组的物理行数上界代理（包含同文件内联测试），不是该功能净新增LOC；**共享组重复出现不可相加**，也不包括所有传递依赖。last_touch取 `git log -1 --format=%cs -- <组内源码文件>`，是源码路径最近日期，不伪称某个函数的精确修改日。测试列引用组级测试/检查入口，逐功能覆盖率与当前通过状态未知，不能把共享测试当成每个路由的专项覆盖。

文档标签：M=README主线；A=README创作区；I=README内部/预发布；C=契约/专用README或检查说明；U=实现可达但未发现独立宣传；IDE=插件manifest/本地README。Defer仅建议退出产品首屏/营销叙事，**保留内部runbook、兼容说明与已登记窗口指引**；公开文档与接口变更仍须批准。Keep-as-is冻结扩张，不禁止必要安全修复。

### 1.2 源码体量、日期与测试证据索引

源码模式均实际展开，测试路径均核对存在；日期只按源码，不把文档修改冒充实现更新。以下组索引供每条特性行交叉引用，未声称测试运行通过。

| group | source paths | test / check anchors | physical lines | latest source date |
| --- | --- | --- | --- | --- |
| G0 | `rf` | `scripts/cli-surface-check.sh` | 278 | 2026-09-26 |
| G1 | `runtime/project-config/src/*.rs` | `scripts/bootstrap-check.sh`; `runtime/project-config/src/lib.rs` | 2032 | 2026-09-27 |
| G2 | `runtime/rf-cli/src/*.rs`; `runtime/rf-runner/src/*.rs`; `runtime/unified-result/src/*.rs` | `scripts/scan-output-check.sh`; `scripts/cli-surface-check.sh` | 5853 | 2026-09-26 |
| G3 | `control-plane/authoring/src/main/**/*.java` | `control-plane/authoring/src/test/java/io/ruleops/authoring/GoldenFixtureTest.java`; `scripts/authoring-capability-check.sh` | 1800 | 2026-09-18 |
| G4 | `control-plane/factory/src/main/**/*.java` | `control-plane/factory/src/test/java/io/ruleops/factory/CandidateCompilerTest.java` | 669 | 2026-09-18 |
| G5 | `control-plane/verification/src/main/**/*.java` | `control-plane/verification/src/test/java/io/ruleops/verification/LocalVerificationTest.java` | 323 | 2026-09-18 |
| G6 | `control-plane/coverage/src/main/**/*.java`; `scripts/phase4-coverage-ledger.sh` | `control-plane/coverage/src/test/java/io/ruleops/coverage/CoverageAnalyzerTest.java` | 316 | 2026-09-18 |
| G7 | `runtime/pilot-observer/src/*.rs` | `scripts/pilot-observation-check.sh`; `runtime/pilot-observer/src/main.rs` | 1002 | 2026-09-26 |
| G8 | `runtime/pilot-evidence/src/*.rs` | `scripts/pilot-evidence-check.sh`; `runtime/pilot-evidence/src/lib.rs` | 910 | 2026-09-26 |
| G9 | `runtime/fast-decision/src/*.rs` | `scripts/fast-decision-check.sh`; `runtime/fast-decision/src/lib.rs` | 1402 | 2026-09-26 |
| G10 | `runtime/status/src/*.rs` | `scripts/status-publication-check.sh`; `scripts/status-adapter-deployment-check.sh` | 1077 | 2026-09-26 |
| G11 | `control-plane/server/src/main/**/*.java` | `control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java`; `control-plane/server/src/test/java/io/ruleops/server/RuleOpsJournalTest.java` | 2430 | 2026-09-26 |
| G12 | `clients/vscode/src/*.js`; `clients/vscode/package.json` | `clients/vscode/test/core.test.js`; `scripts/ide-client-check.sh` | 583 | 2026-09-26 |
| G13 | `clients/intellij/src/main/**/*.java`; `clients/intellij/src/main/resources/META-INF/plugin.xml` | `clients/intellij/selftest/io/ruleops/ide/core/CoreSelfTest.java`; `scripts/ide-client-check.sh` | 951 | 2026-09-26 |
| G14 | `scripts/package-rf.sh`; `scripts/production-runtime-pack-check.sh` | `scripts/ci-neutral-scan-check.sh`; `scripts/delivery-cli-vertical-check.sh` | 392 | 2026-09-26 |
| G15 | `contracts/proto/backend_host.proto`; `runtime/host-client/src/*.rs` | `runtime/conformance/README.md`; `scripts/phase2-independent-acceptance.sh` | 892 | 2026-09-26 |
| G16 | `contracts/proto/workspace_resolution.proto`; `runtime/resolver-client/src/*.rs` | `scripts/phase2-independent-acceptance.sh`; `scripts/affected-scope-check.sh` | 788 | 2026-09-26 |
| G17 | `backend-hosts/fake/src/*.rs`; `workspace-resolvers/fake/src/*.rs` | `scripts/phase2-independent-acceptance.sh`; `workspace-resolvers/fake/tests/integration.rs` | 984 | 2026-09-26 |
| G18 | `backend-hosts/node-fake/*.js` | `backend-hosts/node-fake/README.md`; `scripts/phase2-independent-acceptance.sh` | 112 | 2026-09-27 |
| G19 | `workspace-resolvers/static/src/*.rs` | `scripts/production-runtime-pack-check.sh`; `scripts/affected-scope-check.sh` | 975 | 2026-09-26 |
| G20 | `runtime/conformance/src/*.rs` | `scripts/phase2-independent-acceptance.sh`; `runtime/conformance/README.md` | 1978 | 2026-09-27 |
| G21 | `runtime/verification-worker/src/*.rs` | `scripts/verification-worker-check.sh` | 501 | 2026-09-26 |
| G22 | `runtime/git-scope/src/*.rs`; `runtime/affected-scope/src/*.rs`; `runtime/scan-plan/src/*.rs` | `scripts/git-change-scope-check.sh`; `scripts/scan-plan-check.sh`; `scripts/incremental-baseline-check.sh` | 3233 | 2026-09-26 |
| G23 | `runtime/rule-bundle/src/*.rs`; `runtime/bundle-cache/src/*.rs`; `runtime/bundle-delivery/src/*.rs` | `scripts/bundle-delivery-check.sh`; `scripts/bundle-delivery-e2e-check.sh` | 2032 | 2026-09-26 |
| G24 | `scripts/phase4-pilot-report.sh`; `scripts/pilot-delivery-check.sh`; `scripts/ruleops-center-check.sh` | `scripts/check.sh` | 349 | 2026-09-26 |
| G25 | `runtime/thin-client/src/*.rs` | `scripts/thin-client-check.sh`; `scripts/thin-client-submit-check.sh`; `scripts/submission-status-check.sh`; `scripts/rich-explain-check.sh` | 3078 | 2026-09-26 |
| G26 | `backend-hosts/java/src/main/**/*.java` | `scripts/java-host-e2e.sh`; `scripts/production-runtime-pack-check.sh` | 1184 | 2026-09-26 |

### 1.3 逐项清单与唯一裁决

| ID / feature | entrypoint | estimated code | tests / coverage | last_touch | docs | positioning fit | evidence | maintenance cost | coupling | verdict | one-line reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| L01 launcher/help | rf --help; rf help | 278 L* / G0 | 源码/检查入口 G0；覆盖率未知、未重跑 | 2026-09-26 | M | 3 | 1 | 1 | 4 | Keep-as-is | 入口发现已足够，不另建命令导航层。 |
| L02 参考安装包 | scripts/package-rf.sh OUTPUT | 392 L* / G14 | 源码/检查入口 G14；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 2 | 4 | Keep-as-is | 仅保留reference/契约用途，不当真实Java安装包。 |
| L03 真实Java Runtime装配/安装资产 | scripts/production-runtime-pack-check.sh OUTPUT; packaged rf | 392 L* / G14 | 源码/检查入口 G14；覆盖率未知、未重跑 | 2026-09-26 | C | 5 | 1 | 4 | 5 | Keep | 主用户需要可用运行资产；仅在验证授权后补最短交接，非扩大产品。 |
| C01 项目初始化 | rf init; --non-interactive/--force/--output | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | M | 5 | 1 | 3 | 5 | Keep | 可审阅配置和明确范围是完整性判断前提。 |
| C02 诊断 | rf doctor; --scan-probe; --format text/json | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | M | 4 | 1 | 3 | 4 | Keep | 让真实任务缺前置条件时有可行动诊断。 |
| C03 配置校验 | rf config validate | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | M | 5 | 1 | 3 | 5 | Keep | 固定执行范围与身份，不跳过校验。 |
| C04 配置投影 | rf config show; --format text/json | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | M | 3 | 1 | 2 | 4 | Keep-as-is | 已有可复核投影，不扩配置中心。 |
| C05 Standard校验 | rf standard validate | 1800 L* / G3 | 源码/检查入口 G3；覆盖率未知、未重跑 | 2026-09-18 | A | 3 | 1 | 3 | 4 | Keep-as-is | 保留规则供应安全，不作为扫描新手任务。 |
| C06 RuleIntent校验 | rf rule validate | 1800 L* / G3 | 源码/检查入口 G3；覆盖率未知、未重跑 | 2026-09-18 | A | 3 | 1 | 3 | 4 | Keep-as-is | 既有资产边界够用，不扩创作产品。 |
| C07 三类Candidate生成 | rf rule generate | 669 L* / G4 | 源码/检查入口 G4；覆盖率未知、未重跑 | 2026-09-18 | A | 3 | 1 | 4 | 4 | Keep-as-is | 规则供应已实现，冻结三模板而非另起AI作者平台。 |
| C08 Candidate验证 | rf rule test | 323 L* / G5 | 源码/检查入口 G5；覆盖率未知、未重跑 | 2026-09-18 | A | 4 | 1 | 4 | 5 | Keep-as-is | 保留质量入口，不让主用户先学工厂。 |
| C09 固定覆盖账本分析 | rf coverage analyze | 316 L* / G6 | 源码/检查入口 G6；覆盖率未知、未重跑 | 2026-09-18 | A | 2 | 1 | 3 | 3 | Keep-as-is | 保留内部固定资产检查，不包装通用覆盖产品。 |
| C10 汇总Pilot报告 | rf pilot report | 349 L* / G24 | 源码/检查入口 G24；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 4 | 4 | Defer | 退出产品首屏，仍供已登记观察流程运行。 |
| C11 Pilot交付检查 | rf pilot delivery-check | 349 L* / G24 | 源码/检查入口 G24；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 3 | 4 | Keep-as-is | 保留内部门槛，不把预检当用户价值。 |
| C12 Pilot历史基线 | rf pilot baseline | 1002 L* / G7 | 源码/检查入口 G7；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 3 | 4 | Defer | 保留内部观测；新手先走单次判读。 |
| C13 Shadow观测 | rf pilot observe; local-simulation/synthetic-shadow/live-shadow | 1002 L* / G7 | 源码/检查入口 G7；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 4 | 4 | Defer | 保留已批准窗口工具，移出普通用户介绍而非取消窗口。 |
| C14 Pilot人工反馈 | rf pilot feedback | 1002 L* / G7 | 源码/检查入口 G7；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 3 | 4 | Defer | 质量研究角色使用，不是每次扫描必填项。 |
| C15 Pilot治理快照 | rf pilot governance | 1002 L* / G7 | 源码/检查入口 G7；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 3 | 4 | Defer | 治理记录不挤入主用户执行路径。 |
| C16 模式限定窗口聚合 | rf pilot window-report | 1002 L* / G7 | 源码/检查入口 G7；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 4 | 4 | Defer | 保留窗口取证，不将日常扫描等同正式GO。 |
| C17 Pilot证据封包 | rf pilot evidence package | 910 L* / G8 | 源码/检查入口 G8；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 3 | 4 | Defer | 内部裁决材料，不作为安装前置。 |
| C18 Pilot裁决 | rf pilot evidence adjudicate | 910 L* / G8 | 源码/检查入口 G8；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 4 | 4 | Defer | 保留正式治理角色，不下放批准权。 |
| C19 实验路由证据 | rf decision evaluate | 1402 L* / G9 | 源码/检查入口 G9；覆盖率未知、未重跑 | 2026-09-26 | I/experimental | 1 | 1 | 3 | 2 | Defer | 已有experimental裁定，不重复删除或升级为主产品。 |
| C20 工程中心检查 | rf center check | 349 L* / G24 | 源码/检查入口 G24；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 4 | 4 | Defer | 留在工程验收文档，不建设用户运营台。 |
| C21 Finding解释 | rf explain; --rule-meta/--result/--authority-trust/--assessment; text/json | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 3 | 4 | Defer | 已有实现但安装交接未闭合，先验证用户需要。 |
| C22 反馈草稿 | rf feedback draft | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 3 | 4 | Defer | 无独立使用证据，不强制主用户走治理申请。 |
| C23 例外申请 | rf exception request | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 3 | 4 | Defer | 保留摘要边界，移出首屏而非开放批准。 |
| C24 反馈提交 | rf feedback submit | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 4 | 4 | Defer | 依赖显式Server，不成为首次价值前置。 |
| C25 例外提交 | rf exception submit | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 2 | 1 | 4 | 4 | Defer | 不把RECEIVED_ONLY包装成批准或治理闭环。 |
| C26 未知提交恢复 | rf submission status | 3078 L* / G25 | 源码/检查入口 G25；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 4 | 4 | Keep-as-is | 保留申请人的安全恢复，不扩自动重试。 |
| C27 状态发布契约校验 | rf status contract-check | 1077 L* / G10 | 源码/检查入口 G10；覆盖率未知、未重跑 | 2026-09-26 | I | 4 | 1 | 3 | 5 | Keep-as-is | 保留权威边界，不当SCM发布功能宣传。 |
| C28 外部adapter部署契约 | rf status deployment-check | 1077 L* / G10 | 源码/检查入口 G10；覆盖率未知、未重跑 | 2026-09-26 | I | 3 | 1 | 3 | 4 | Keep-as-is | 保留精确权限约束，不实施新provider集成。 |
| C29 本地/Shadow扫描 | rf scan --repo-root; --rule-id/--candidate/--delivery-descriptor | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 5 | 1 | 5 | 5 | Keep | 直接产出主用户要复核的范围、结果和不完整状态。 |
| S01 可读扫描结果 | rf scan --format text | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 5 | 1 | 2 | 4 | Keep-as-is | 已有可读首个价值，不需额外页面。 |
| S02 结构化结果/失败 | rf scan --format json | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 5 | 1 | 3 | 5 | Keep | 机器与人工复核身份和三态的核心交接。 |
| S03 SARIF导出 | rf scan --format sarif | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 3 | 1 | 3 | 3 | Keep-as-is | 复用既有消费格式，不再造格式。 |
| S04 进度流/静默 | rf scan --progress text/json/none | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 2 | 1 | 2 | 3 | Keep-as-is | 保留有界stderr进度，不扩动画或面板。 |
| S05 仓库内结果文件 | rf scan --output FILE | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | M | 4 | 1 | 2 | 4 | Keep-as-is | 支持下一位消费者，保留路径约束。 |
| S06 Git变更/增量范围 | rf scan --base REV --head REV; FULL/MODULE/INCREMENTAL_PREVIEW | 3233 L* / G22 | 源码/检查入口 G22；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 5 | 5 | Keep-as-is | 保留范围闭包与回退，不以优化替代需求验证。 |
| S07 历史/比较/质量基线 | rf scan --baseline-kind debt/comparison/quality --baseline-fingerprint FP | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 4 | Keep-as-is | 存量债务已有演练，不新增基线引擎。 |
| S08 例外/阻断集合输入 | rf scan --exception-fingerprint FP --blocking-rule-id ID | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Keep-as-is | 保留既有输入语义，不扩为客户端自助批准。 |
| S09 交付/离线缓存模式 | rf config/scan --execution-mode; --cache-dir; --trust-state-path | 2032 L* / G23 | 源码/检查入口 G23；覆盖率未知、未重跑 | 2026-09-26 | M/C | 4 | 1 | 5 | 5 | Keep-as-is | 保留离线可用性和签名验证，不降级信任。 |
| S10 模块/路径配置覆盖 | rf init/config/scan --language/--build-system/--module/--source-root/--test-root/--generated-root/--vendor-root/--include/--exclude | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | M/C | 4 | 1 | 4 | 5 | Keep-as-is | 保持单一配置优先级，不另造scope入口。 |
| S11 Host/Resolver与执行参数 | rf scan --host/--host-arg/--resolver/--resolver-arg/--deadline-ms/--execution-lock/--runtime-home | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留harness接缝；主用户消费已配置资产而非拼内部命令。 |
| R01 配置身份诊断 | erf-project-config identity | 2032 L* / G1 | 源码/检查入口 G1；覆盖率未知、未重跑 | 2026-09-27 | U | 3 | 1 | 2 | 3 | Keep-as-is | 已有机器诊断入口，不与config show并列扩产品。 |
| R02 低层Runner harness | rf-runner | 5853 L* / G2 | 源码/检查入口 G2；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留集成消费面，不推出第二套用户CLI。 |
| R03 独立Verification Worker | erf-verification-worker --host --resolver --output; digest/sandbox options | 501 L* / G21 | 源码/检查入口 G21；覆盖率未知、未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 独立验证是边界，fixture入口不当正式服务卖点。 |
| R04 协议一致性验收 | erf-conformance --host --resolver --cancel-window-ms | 1978 L* / G20 | 源码/检查入口 G20；覆盖率未知、未重跑 | 2026-09-27 | C | 4 | 1 | 4 | 5 | Keep-as-is | 支撑既有backend-neutral契约，不以suite数量充用户价值。 |
| R05 Rust reference进程/注入 | erf-fake-host; erf-fake-resolver; FAKE_* | 984 L* / G17 | 源码/检查入口 G17；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 3 | 5 | Keep-as-is | 保留故障与协议证明，不能替代真实Java分析。 |
| R06 Node reference进程/注入 | node backend-hosts/node-fake/host.js; resolver.js; FAKE_* | 112 L* / G18 | 源码/检查入口 G18；覆盖率未知、未重跑 | 2026-09-27 | C | 2 | 1 | 3 | 4 | Keep-as-is | ADR已恢复协议探针，不重复建议删除或误称JS引擎。 |
| R07 真实Java Backend进程 | java -jar erf-java-host-0.1.0-shaded.jar | 1184 L* / G26 | 源码/检查入口 G26；覆盖率未知、未重跑 | 2026-09-26 | C | 5 | 1 | 5 | 5 | Keep-as-is | 真实执行依赖已有，保留兼容安全维护而非扩引擎。 |
| R08 静态Workspace Resolver | erf-static-resolver | 975 L* / G19 | 源码/检查入口 G19；覆盖率未知、未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 共享范围事实保留，不伪装ATTESTED构建导出。 |
| P01 Backend Host协议 Hello/HelloAck | contracts/proto/backend_host.proto :: Hello/HelloAck | 892 L* / G15 | 协议suite源码 G15；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 冻结版本化接缝；删除会影响Host/Runner/conformance。 |
| P02 Backend Host协议 Describe | contracts/proto/backend_host.proto :: Describe | 892 L* / G15 | 协议suite源码 G15；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 冻结版本化接缝；删除会影响Host/Runner/conformance。 |
| P03 Backend Host协议 Plan | contracts/proto/backend_host.proto :: Plan | 892 L* / G15 | 协议suite源码 G15；未重跑 | 2026-09-26 | C | 5 | 1 | 4 | 5 | Keep-as-is | 冻结版本化接缝；删除会影响Host/Runner/conformance。 |
| P04 Backend Host协议 Execute/events/complete | contracts/proto/backend_host.proto :: Execute/events/complete | 892 L* / G15 | 协议suite源码 G15；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 冻结版本化接缝；删除会影响Host/Runner/conformance。 |
| P05 Backend Host协议 Cancel/CancelAck | contracts/proto/backend_host.proto :: Cancel/CancelAck | 892 L* / G15 | 协议suite源码 G15；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 冻结版本化接缝；删除会影响Host/Runner/conformance。 |
| P06 Workspace协议 Hello/HelloAck | contracts/proto/workspace_resolution.proto :: Hello/HelloAck | 788 L* / G16 | 协议suite源码 G16；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留解析和取消协议，不让各Backend重新猜工作区。 |
| P07 Workspace协议 Resolve/WorkspaceDescriptor | contracts/proto/workspace_resolution.proto :: Resolve/WorkspaceDescriptor | 788 L* / G16 | 协议suite源码 G16；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留解析和取消协议，不让各Backend重新猜工作区。 |
| P08 Workspace协议 Cancel/CancelAck | contracts/proto/workspace_resolution.proto :: Cancel/CancelAck | 788 L* / G16 | 协议suite源码 G16；未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留解析和取消协议，不让各Backend重新猜工作区。 |
| H01 GET /v1/standards | GET /v1/standards | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H02 GET /v1/standards/{id} | GET /v1/standards/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H03 GET /v1/coverage | GET /v1/coverage | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H04 GET /v1/coverage/{id} | GET /v1/coverage/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H05 GET /v1/rules | GET /v1/rules | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H06 GET /v1/rules/{id} | GET /v1/rules/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H07 GET /v1/factory-jobs | GET /v1/factory-jobs | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H08 GET /v1/factory-jobs/{id} | GET /v1/factory-jobs/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H09 GET /v1/verification-jobs | GET /v1/verification-jobs | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H10 GET /v1/verification-jobs/{id} | GET /v1/verification-jobs/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H11 GET /v1/releases | GET /v1/releases | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H12 GET /v1/releases/{id} | GET /v1/releases/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H13 GET /v1/policies | GET /v1/policies | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H14 GET /v1/policies/{id} | GET /v1/policies/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H15 GET /v1/scans | GET /v1/scans | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H16 GET /v1/scans/{id} | GET /v1/scans/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H17 GET /v1/findings | GET /v1/findings | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H18 GET /v1/findings/{id} | GET /v1/findings/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H19 GET /v1/feedback | GET /v1/feedback | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H20 GET /v1/feedback/{id} | GET /v1/feedback/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H21 GET /v1/exceptions | GET /v1/exceptions | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 已有读取/分页；冻结服务面，不将读接口当完整用户工作台。 |
| H22 GET /v1/exceptions/{id} | GET /v1/exceptions/{id} | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | U | 2 | 1 | 4 | 5 | Defer | 通用版本读取分派可达；实际资源可用性未单独验证，不扩资源中心。 |
| H23 POST /v1/standards | POST /v1/standards | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H24 POST /v1/coverage | POST /v1/coverage | 2430 L* / G11 | G11共享测试；该直接写路由专项覆盖未发现证据 | 2026-09-26 | U | 1 | 1 | 4 | 5 | Cut | 显式动作路由已覆盖同类登记用途；建议收撤未单列的通用写入口，须批准。 |
| H25 POST /v1/rules | POST /v1/rules | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H26 POST /v1/factory-jobs | POST /v1/factory-jobs | 2430 L* / G11 | G11共享测试；该直接写路由专项覆盖未发现证据 | 2026-09-26 | U | 1 | 1 | 4 | 5 | Cut | 显式动作路由已覆盖同类登记用途；建议收撤未单列的通用写入口，须批准。 |
| H27 POST /v1/verification-jobs | POST /v1/verification-jobs | 2430 L* / G11 | G11共享测试；该直接写路由专项覆盖未发现证据 | 2026-09-26 | U | 1 | 1 | 4 | 5 | Cut | 显式动作路由已覆盖同类登记用途；建议收撤未单列的通用写入口，须批准。 |
| H28 POST /v1/releases | POST /v1/releases | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H29 POST /v1/policies | POST /v1/policies | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H30 POST /v1/scans | POST /v1/scans | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H31 POST /v1/feedback | POST /v1/feedback | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H32 POST /v1/exceptions | POST /v1/exceptions | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 保留治理/执行记录登记，缺真实部署需求时不扩服务面。 |
| H33 POST /v1/coverage/analyze | POST /v1/coverage/analyze | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 这是资源/作业登记路由，不仅凭名称认定后台执行闭环。 |
| H34 POST /v1/rules/{id}/generate | POST /v1/rules/{id}/generate | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 这是资源/作业登记路由，不仅凭名称认定后台执行闭环。 |
| H35 POST /v1/rules/{id}/evaluations | POST /v1/rules/{id}/evaluations | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 2 | 1 | 4 | 5 | Defer | 这是资源/作业登记路由，不仅凭名称认定后台执行闭环。 |
| H36 POST /v1/releases/{id}/promote | POST /v1/releases/{id}/promote | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H37 POST /v1/releases/{id}/revoke | POST /v1/releases/{id}/revoke | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H38 POST /v1/policies/{id}/promote | POST /v1/policies/{id}/promote | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H39 POST /v1/policies/{id}/revoke | POST /v1/policies/{id}/revoke | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H40 POST /v1/exceptions/{id}/promote | POST /v1/exceptions/{id}/promote | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H41 POST /v1/exceptions/{id}/revoke | POST /v1/exceptions/{id}/revoke | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 保留版本化生命周期与撤回安全；本地Server不授予生产SCM权威。 |
| H42 GET /v1/health | GET /v1/health | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 3 | 1 | 4 | 5 | Keep-as-is | 已有运维探针足够，不扩仪表盘。 |
| H43 GET /v1/audit | GET /v1/audit | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留只读审计链，不能为缩面丢掉追溯。 |
| H44 GET /v1/rule-bundles/{bundleDigest}/delivery | GET /v1/rule-bundles/{bundleDigest}/delivery | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留签名资产交接；真实发行仍需实证。 |
| H45 GET /v1/submissions/status | GET /v1/submissions/status | 2430 L* / G11 | 源码/共享测试 G11；覆盖率未知、未重跑 | 2026-09-26 | C | 4 | 1 | 4 | 5 | Keep-as-is | 保留UNKNOWN后的查询，不扩自动重试。 |
| V01 ERF: 显示 Finding | erf.showFinding | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V02 ERF: 跳转到 Finding 位置 | erf.navigateToLocation | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V03 ERF: 解释 Finding | erf.explainFinding | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V04 ERF: 起草反馈（PROPOSED） | erf.draftFeedback | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V05 ERF: 申请例外（REQUESTED） | erf.requestException | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V06 ERF: 提交反馈到 Server（RECEIVED_ONLY） | erf.submitFeedback | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V07 ERF: 提交例外到 Server（RECEIVED_ONLY） | erf.submitException | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V08 ERF: 查询提交状态 | erf.submissionStatus | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 3 | 4 | Defer | 已有核/绑定，但实际安装采用未证实，不先扩IDE产品。 |
| V09 VS Code项目配置 | erf.rfPath, erf.serverEndpoint, erf.telemetryEnabled, erf.requestTimeoutMs | 583 L* / G12 | 源码/共享测试 G12；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 2 | 4 | Keep-as-is | 保留显式rf路径/endpoint/超时与遥测默认关闭，不加自动发现。 |
| I01 展示 Finding | io.ruleops.ide.ShowFinding | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I02 导航到位置 | io.ruleops.ide.NavigateToLocation | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I03 解释 Finding | io.ruleops.ide.ExplainFinding | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I04 起草 Feedback | io.ruleops.ide.DraftFeedback | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I05 申请 Exception | io.ruleops.ide.RequestException | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I06 提交 Feedback | io.ruleops.ide.SubmitFeedback | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I07 提交 Exception | io.ruleops.ide.SubmitException | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |
| I08 查询提交状态 | io.ruleops.ide.SubmissionStatus | 951 L* / G13 | 源码/共享测试 G13；覆盖率未知、未重跑 | 2026-09-26 | IDE | 2 | 1 | 4 | 4 | Defer | 冻结已有薄绑定，未证实真实IDE验收前不扩用户入口。 |

补充覆盖说明：HTTP以JSON契约/摘要响应为共同输出；协议面为Protobuf会话与事件。Candidate目录、验证报告、Pilot journal/Markdown证据包、Thin Client JSON请求/回执属于对应子命令的耦合产物，随该行裁决，不把每个内部文件凑成新功能。JSON/SARIF/text扫描消费面则独立列为S01–S03。

Server事实锚点：`RuleOpsHttpServer.java:220–265,287–314,740–750` 与 `RuleOpsStore.java:31–42`。合同显式端点以外的通用list/detail/POST也已清点；路由可达不等于有持久数据或真实部署。`RuleOpsHttpServerTest.java`可定位health/lifecycle/audit与`coverage/analyze`、`rules/{id}/generate`，三个直接POST的专项测试未发现证据。

## 2. 裁决计数

| verdict | count | meaning |
| --- | --- | --- |
| Keep | 6 | 核心投资候选，仍受验证/用户批准约束 |
| Keep-as-is | 52 | 保留实现与安全维护，冻结扩张 |
| Defer | 60 | 保留代码/内部兼容说明，退出对外主线叙事 |
| Cut | 3 | 提议收撤三个隐式通用写入口，未执行 |
| Spin-out | 0 | 未发现独立需求，不启动拆仓或新插件产品 |

合计 **121** 项；Keep **6/121 = 5.0%**，小于1/3。分母是本次细粒度可见入口，不是01的12个任务组、完成率或市场需求数；API逐方法展开也不代表有45项独立购买需求。每行只有一个裁决。

Keep清单：L03、C01、C02、C03、C29、S02。其余中安全恢复、协议与验证保留但冻结，治理/研究/IDE多数退居内部；没有因需求未验证而删安全基础，也不因耦合高便判Keep继续扩建。

## 3. 新手10分钟路径：install → first value → second step

**现状：未发现可下载且已由陌生用户完成安装的正式发行物；从零开始的10分钟路径尚未证实。** 下面是待验证的恢复路径，不是假冒已跑通的教程；不用reference fake包冒充Java价值，不让新手编译整仓、生成规则或填Pilot YAML。

| step / target time | visible action and acceptance | allowed feature IDs | stop condition |
| --- | --- | --- | --- |
| install / 0–3分钟（假设） | 发行负责人先提供已验收的真实Java Runtime包与Candidate并声明JDK/平台要求；用户解包后运行rf --help。包与安装本次未验证，不编造下载URL。 | L03 Keep；L01 Keep-as-is | 资产缺失/平台不符就判入口未交付，回到发行owner，不换fake或携源码硬跑。 |
| first value / 3–8分钟（假设） | 初始化目标Java工作区并校验配置，运行scan，用户能指出完整性/违规结果与LOCAL_UNATTESTED限制。 | C01/C03/C29 Keep；S01 Keep-as-is | exit2/3/4保留诊断，必要时用C02 Keep；不完整/信任失败不得冒称通过。 |
| second step / 8–10分钟（假设） | 将同一任务结果输出为JSON/SARIF交项目负责人复核，不要求解释/反馈/Server或IDE安装。 | S02 Keep；S03/S05 Keep-as-is | 接收者无法区分范围/完整性/违规/权威则记录交接失败，不能只看exit0当Gate PASS。 |

目标命令（本次未执行扫描）；`RF`、`REPO`、`CANDIDATE`均须来自上一步确认的资产与目录，不是隐藏前置：

```bash
"$RF" --help
"$RF" init --repo-root "$REPO" --non-interactive
"$RF" config validate --repo-root "$REPO"
"$RF" scan --repo-root "$REPO" --candidate "$CANDIDATE" --format text
"$RF" scan --repo-root "$REPO" --candidate "$CANDIDATE" --format sarif --output "$REPO/.ruleops/scan.sarif"
```

时间是未验证假设，不是性能实测/SLA；获取runtime和规则资产的耗时必须单列，不能藏到“10分钟”计时之外。未取得资产就不能宣称整条路径达标。路径中只有Keep/Keep-as-is，没有Defer/Cut；可读text/JSON/SARIF是当前交接，不要求新增GUI或审批系统。

## 4. 成本与迁移台账（全部Cut/Spin-out）

三项Cut仅建议收撤API可见入口，不删Coverage/Factory/Verification领域、存量数据或资源读取。它们的共享代码/兼容耦合高，客户与SDK使用情况未发现证据，因此必须先核对调用方，再请用户批准，不能直接删除上线。

| Cut ID | target | what breaks: docs / tests / users | migration moves | rollback / stop |
| --- | --- | --- | --- | --- |
| H24 | `POST /v1/coverage` | 文档：合同endpoints未单列该POST，潜在SDK仍可能依赖；测试：共享create、journal/idempotency需回归，专项覆盖未知；用户：调用方/数量未发现证据，不假定无人使用。 | 人工确认调用清单与兼容窗口后，迁至 `POST /v1/coverage/analyze`；核对相同envelope、版本、资源摘要与coverage读取；只关闭指定HTTP入口，保留ResourceType/store/list/detail和显式动作，不做数据删除；批准后补替代/拒绝/重放测试及迁移说明。 | 若payload不等价或调用方不可迁移，停止Cut并回到用户裁决；保留旧版本回滚能力，不制造静默成功fallback。 |
| H26 | `POST /v1/factory-jobs` | 文档：合同endpoints未单列该POST，潜在SDK仍可能依赖；测试：共享create、journal/idempotency需回归，专项覆盖未知；用户：调用方/数量未发现证据，不假定无人使用。 | 人工确认调用清单与兼容窗口后，迁至 `POST /v1/rules/{payload.ruleId}/generate`；校验payload.ruleId与路径一致，保留job id/版本/摘要；只关闭指定HTTP入口，保留ResourceType/store/list/detail和显式动作，不做数据删除；批准后补替代/拒绝/重放测试及迁移说明。 | 若payload不等价或调用方不可迁移，停止Cut并回到用户裁决；保留旧版本回滚能力，不制造静默成功fallback。 |
| H27 | `POST /v1/verification-jobs` | 文档：合同endpoints未单列该POST，潜在SDK仍可能依赖；测试：共享create、journal/idempotency需回归，专项覆盖未知；用户：调用方/数量未发现证据，不假定无人使用。 | 人工确认调用清单与兼容窗口后，迁至 `POST /v1/rules/{payload.ruleId}/evaluations`；校验规则身份、作业状态与现有查询契约；只关闭指定HTTP入口，保留ResourceType/store/list/detail和显式动作，不做数据删除；批准后补替代/拒绝/重放测试及迁移说明。 | 若payload不等价或调用方不可迁移，停止Cut并回到用户裁决；保留旧版本回滚能力，不制造静默成功fallback。 |

Spin-out **0项**：未发现足以支撑独立产品的需求证据；ADR-0167已裁定单一monorepo与seam，不因目录独立就启动拆仓。

## 5. 需人工批准的删除项（单列）

| approval item | proposed deletion / public withdrawal | current status |
| --- | --- | --- |
| A1 / H24 | 收撤 `POST /v1/coverage` 的隐式写入口，不删领域/数据 | 待用户批准；未执行 |
| A2 / H26 | 收撤 `POST /v1/factory-jobs` 的隐式写入口，不删领域/数据 | 待用户批准；未执行 |
| A3 / H27 | 收撤 `POST /v1/verification-jobs` 的隐式写入口，不删领域/数据 | 待用户批准；未执行 |

**需人工批准的删除/入口收撤项：3；拟直接删除文件：0；已删除：0。**

此外，所有Defer项退出公开用户文档/首屏的改动、Cut迁移通知和任何新安装承诺同样必须批准，但不计成额外删除文件。内部runbook/契约/兼容说明/安全检查保留；不得借Defer取消已登记窗口，不借Cut重开既有商定范围。

## 6. 检查范围与验收

本项完整读取指定feature-scope-triage技能；固定引用01，沿用02–04的范围、用户与指标。核对rf路由、rf-cli/project-config参数分派、HTTP契约/Server通用分派、两端IDE注册、Rust binary manifests、Host/Resolver proto及Conformance/Node说明；源码行数与日期由程序逐组统计。外部issue、真实用户/付费、真实部署未访问或未发现证据，不将实现存在当需求信号。

覆盖：C01–C29列出rf全部29个公开子命令；45个HTTP方法/路由覆盖合同所有显式端点并登记通用额外面；VS Code与IntelliJ各8个action逐项列出，VS Code四个配置键合并V09。erf-project-config→C01–C04/R01，rf-cli→C29/S组，rf-client→C21–C26，pilot-observer/evidence/fast-decision→C10–C19，底层别名不双计。明确拒绝的rf release/gate与客户端exception approval不算可用特性，保留拒绝，不当待开发需求。

直接锚点：`README.md:64–171`、`rf`、`contracts/http/ruleops-api-v1.yaml`、`control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:220–314,740–750`、`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:31–42`、`clients/vscode/package.json`、`clients/intellij/src/main/resources/META-INF/plugin.xml`、`contracts/proto/backend_host.proto`、`contracts/proto/workspace_resolution.proto`；实现/测试逐组见§1.2。

前序报告只读基线：

- `01-product-positioning-audit.md`: `1d874ff0cd92d9acfb813d3fa4f9a85435d3d71f304d79bbc0d315e3e50cf39d`
- `02-product-scope-audit.md`: `e9897e69e783273f0e7bc3014cce4e248ea0e79f2d071d259ff5063884b75856`
- `03-target-user-and-jtbd.md`: `81b32c10a319045cbe5bfe1a00719b4d6cee66bc531f8d100dae76f9a6b221b4`
- `04-goals-and-success-metrics.md`: `0746621b4dbe1668de0bf338350be10c1021f1c8357a46b7b1bbed75f1d3b0d4`

最终报告验收已通过：Python断言核对固定定位逐字一致、121项唯一ID/裁决、逐项四分评分及体量/测试/日期/宣传记录、全部显式HTTP端点和两端各8动作、Keep 6/Keep-as-is 52/Defer 60/Cut 3/Spin-out 0、主线无Defer/Cut、3项Cut迁移与人工批准记录；27个源码组重新统计行数和git日期一致，前四报告SHA-256不变，git无已跟踪产品差异。未运行打包/IDE/Server，不把源码测试索引写成行为通过。

本次实际尝试 `rtk proxy bash scripts/check.sh`，exit1，PATH中找不到bash，未执行到仓库门禁脚本；未归因、修复或另启写入型构建。当前全量产品门禁状态未建立证据，报告验收不等于产品验收通过。没有删除、修改产品/合同/ADR/前报告、外部写入或commit/push。
