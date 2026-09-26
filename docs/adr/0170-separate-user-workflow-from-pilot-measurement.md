# ADR-0170：分离用户工作流与 Pilot 取证流程

- 状态：Accepted
- 日期：2026-09-26
- 决策者：ERF 产品与治理边界

## 背景

LIVE_SHADOW 的作者成本合同要求记录手写/模板主动时间、机器耗时、UTC 起止时间、作者身份、
路径顺序和证据引用。这些字段服务于内部 Pilot 的可审计性，却不属于规则作者或扫描使用者的
正常任务。把它们直接暴露为操作步骤，会让用户误以为每次使用都需要维护 YAML 证据记录，
并把内部研究流程误当成产品工作流。

## 决策

ERF 明确分为三条路径：

1. **日常用户路径**：初始化项目、校验配置、运行扫描、查看三态 Gate 与 Completeness；
   不要求作者编号、路径顺序、手工计时或填写证据路径。
2. **Pilot 观察路径**：观察负责人运行登记的只读 Shadow 任务，系统自动记录 repository、
   commit、rule-bundle digest、机器耗时和 journal；观察员只处理运行结果与质量复核。
3. **内部研究取证路径**：只有在正式 Pilot 需要验证作者成本假设时，才启用
   `AuthorCostRecord` 的手写/模板配对记录。该记录属于内部审计附件，不是产品使用入口。

现有 PilotSuccessContract、阈值和缺测时的 `INCONCLUSIVE` 语义不变；简化用户路径不得被解释
为降低验证标准，也不得用自动采集的机器耗时替代作者主动时间。

## 后果

- README 与 Pilot 文档必须先引导用户到轻量主线，内部取证文档必须显式标注为非日常流程。
- 正常使用不再被 `AuthorCostRecord` 阻塞；缺少研究证据只影响 Pilot 裁决，不影响本地执行层的
  使用说明。
- 后续 IDE/Thin Client 只需呈现 Finding 导航、解释和反馈动作，不应暴露审计 YAML 字段。
- 当前交付仍保持 Shadow 权限边界；本决策不授予 SCM 写入、生产 Gate 或受保护分支权力。

## 参考

- `docs/pilot/operator-quickstart.md`
- `docs/pilot/live-shadow-window-execution-checklist.md`
- `docs/pilot/author-cost-baseline.md`
- `specs/pilot/pilot-success-contract.yaml`
