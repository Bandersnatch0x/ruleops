# RuleOps Center 闭环交付

本文件把架构契约落到当前 MVP 的可执行验收面。完整图对应
`specs/center/ruleops-center-closure.yaml`，入口是：

```text
./rf center check .scratch/ruleops-center-check
```

```text
                         RuleOps Center
                                |
        +-----------------------+-----------------------+
        |                       |                       |
  Standards Center       Rule Factory            Rule Registry
        |                       |                       |
        |                 Verification Lab             |
        +-----------------------+-----------------------+
                                |
                         Policy Center
                                |
                     Signed Rule Bundle
                                |
                 +--------------+--------------+
                 |              |              |
              Repo A         Repo B         Repo C
                 |              |              |
                 +--------------+--------------+
                                |
                       Findings / Coverage
                                |
                    Feedback / Exception
                                |
                                v
                         RuleOps Center
```

## 当前交付含义

| 架构面 | 当前实现 | 验收 |
| --- | --- | --- |
| Standards Center | 100 条 Java Standard、20 条 CoverageLedger | inventory/coverage checks |
| Rule Factory | Standard → RuleIntent → Rule IR → Candidate | authoring/factory checks |
| Verification Lab | Candidate → EvidenceBundle | independent fixture verification |
| Rule Registry | 签名 RuleRelease 的 immutable lifecycle | registry lifecycle tests |
| Policy Center | PolicyProfile、ExceptionGrant、三态 Gate | governance/gate tests |
| Signed Rule Bundle | 四类输入的精确 digest 组合签名；当前与执行切片的关系为组合已验签、Shadow 注册 | `erf-rule-bundle` tests |
| Repo A/B/C | 3 个预登记 Java 仓库，只读 Shadow | live repository manifest/observation |
| Findings/Coverage | UnifiedResult、Completeness、CoverageLedger | runtime and coverage evidence |
| Feedback/Exception | Feedback ledger、regression handoff、ExceptionGrant | feedback/governance evidence |

`ENGINEERING_MVP` 的意思是闭环可以构建、验证和复跑；它不是生产强制执行。
三个仓库仍不写回，默认只保存 digest/摘要，正式质量和资源结论仍要等登记窗口。

`rf center check` 会独立验签 Rule Bundle 的四输入组合，并复跑一个
`LOCAL_UNATTESTED` 的垂直执行切片；这一区分是有意保留的。把序列化的签名
Bundle 接入受信 Runner、发布 GateAttestation 并开放受保护分支权限，属于后续
生产授权阶段，不被本次 Shadow 交付伪装成已完成。
