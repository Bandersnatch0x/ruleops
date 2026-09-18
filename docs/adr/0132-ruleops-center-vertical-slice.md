# ADR-0132: 以 RuleOps Center 闭环作为 MVP 交付验收面

- 状态：已接受
- 日期：2026-09-18
- 决策者：platform-engineering

## 背景

前一版 MVP 已分别实现 Standards、Factory、Verification、Registry、Policy、
Runtime、Finding、Coverage、Feedback 和 Exception 的深模块，但交付门禁主要按
阶段验收，不能直接证明用户约定的 RuleOps Center 闭环已经连通。尤其是
`Signed Rule Bundle` 的组合签名没有独立的实现与验收入口。

## 决策

V0.x 将用户提供的闭环图固化为 `RuleOpsCenterClosure` 交付契约，并提供
`rf center check OUT_DIR` 作为单一验收入口。该入口通过 Git + CLI + CI 组合
现有模块，不引入一个薄的“中心服务”去转发每个模块的方法。

`Signed Rule Bundle` 由 `runtime/rule-bundle` 负责：它绑定已分别验证的
RuleRelease、PolicyProfile、ExecutionLock 和 OntologyContractPack 的精确 digest、
兼容身份与平台，并由 `rule-bundle-signing` 角色签名。它不替代各输入资产自身的
签名，也不授予受保护分支权力。

验收入口会独立验证这一组合签名，然后复跑一个 `LOCAL_UNATTESTED` 的执行切片；
当前不是把尚未序列化接入 Runner 的组合对象冒充成生产可信执行。Bundle 接入受信
Runner、生成 GateAttestation 和启用受保护分支，是下一阶段的授权性工作。

三份外部 Java 仓库继续使用 `LIVE_SHADOW` 只读执行。闭环验收证明工程链路、
摘要和三态结果；正式产品指标仍必须在登记窗口补齐，不能由演练数据代替。

## 验收边界

- `rf center check` 通过，表示中心之间的工程 seam 和关键不变量可复跑；
- `bash scripts/check.sh` 仍是仓库级最终门禁；
- `ENGINEERING_MVP` 不等于 `PRODUCTION_ENFORCEMENT`；
- SCM 写入、源码上传、状态发布和受保护分支权限仍由本地切片拒绝。
