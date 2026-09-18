# Representative Java Repository Manifest

当前 Phase 4 的三个 representative repository 是受控的本地 Java/Maven
fixture，不是外部生产仓库。它们分别使用唯一的 repository identity 与
commit identity，统一通过 Java PMD 的真实 Runner → TrustedContext → Status
Publisher → UnifiedResult handoff，并在每个身份上保留 crash 注入为
`INCONCLUSIVE -> ActionRequired`。

manifest 明确记录：

- 三个固定身份：`org/ruleops-shadow-1`、`org/ruleops-shadow-2`、
  `org/ruleops-shadow-3`；
- 对应 commit：`phase4-shadow-commit-1` 至 `phase4-shadow-commit-3`；
- Java/Maven、`main` source set 与本地可复现 fixture 来源；
- 无 SCM credentials、无网络、无 repository writes、无 source upload；
- `SHADOW` authority 与 `linux/amd64` 测试边界；
- `productionRepository: false`，不得将 count-based fixture qualification
  解读为 live pilot 或生产仓库覆盖。

机器可读主记录：`specs/pilot/representative-java-repositories.yaml`。
验收入口：`scripts/representative-java-repositories-check.sh`。
运行期目录由 `scripts/phase4-shadow-pilot.sh` 在 `.scratch/` 下生成，
不会成为仓库根目录的持久源码资产。
