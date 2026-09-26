# ADR-0162: Installable `rf` Thin Client 与运行时资产解析

- Status: Accepted
- Date: 2026-09-20
- Scope: V0.4/CI、`rf` CLI、CI-neutral scan entrypoint
- Decision record: GitHub issue #112
- Production delivery follow-up: GitHub issue #123

## Context

项目配置与 Runner scope 已经冻结，但仓库内的 `rf scan` 仍要求调用方直接
拼接内部 Backend Host/Workspace Resolver 命令。这个入口适合开发 fixture，不能
作为企业 CI 的可审阅产品路径；同时，配置 CLI 覆盖必须与每次扫描的三元项目身份
使用同一套解析结果。

## Decision

`rf` 提供一个可复制的 release layout：

```text
rf
runtime-manifest.json
bin/rf-cli
bin/erf-project-config
bin/rf-backend-host
bin/rf-workspace-resolver
```

标准扫描只需要项目根、有效 `.ruleops/config.yaml`、规则身份和已安装 release
assets，不要求用户传入内部 `--host`/`--resolver`。CLI 按显式 runtime home、
`RULEOPS_RUNTIME_HOME`、安装包 `bin`、`PATH` 的顺序寻找命名资产；缺失时返回
结构化的输入错误并 fail closed。`--host`/`--resolver` 保留为本地 harness 的高级
覆盖，不能成为标准 CI 文档路径。

扫描入口重新解析 CLI > environment > project > defaults，并将同一份有效配置的
`effectiveConfigDigest`、`WorkspaceDescriptor` digest、config `ScanPlan` digest
注入闭合结果。Connected、Offline、Standalone 的 cache/trust 语义由 project
config/doctor 与 RuleBundle delivery boundary 共同约束；CLI 不上传源码、完整
Finding、SCM 凭证，也不拥有 Gate/批准/SCM 权力。

结果同时携带配置投影的 `workspaceDescriptorDigest` 与实际 Resolver 返回并被
Runner 消费的 `resolvedWorkspaceDescriptorDigest`，避免把“配置身份”误称为
“实际解析身份”。使用 `--output` 时，闭合 `ScanResult` 还带有仓库内相对的
`artifactPath`。Offline/Standalone 在没有 Candidate 或 delivery/cache 输入时
直接返回结构化 `DELIVERY_INCONCLUSIVE`、exit 3；显式 Candidate 仍保持本地未认证
预览语义，不被提升为 Gate 权威。

## Verification

```text
cargo test -q -p rf-cli --all-targets
bash scripts/ci-neutral-scan-check.sh .scratch/ci-neutral-scan-check-112
bash scripts/check.sh
```

`ci-neutral-scan-check.sh` 使用 release layout 跑真实 `rf init` 与 JSON scan，显式
检查标准 invocation 没有内部 Host/Resolver 参数、项目三元 identity 可消费、三种
execution mode 产生确定且不同的 ScanPlan identity，并检查源代码/完整 Finding/SCM
凭证默认不出现在结果中。

参考 host/resolver 只用于可复现的 contract fixture；发布 manifest 为每个资产记录
digest、compatibility identity 和 verification 状态，并明确标记
`productionReplacementRequired`。生产部署应在同名资产位置放置经过签名、兼容性
和沙箱策略验证的 Java Backend Host 与 Workspace Resolver Pack；参考资产不能冒充
生产适配器。标准 `rf scan` 将签名 RuleBundle delivery/cache、ExecutionLock
绑定和生产 Runtime Pack 接入作为独立的下一切片，见 #123；在该切片完成前，
`REFERENCE_FIXTURE` 仅能通过 CI-neutral contract fixture 验收，不能作为生产包或
Connected/Offline/Standalone 的权威执行凭据。
