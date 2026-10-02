# Live Java Repository Sample

- Status: selected for the Shadow-only pilot
- Selection date: 2026-09-18
- Authority: `SHADOW`
- Build: Maven
- Fixed sample: three public repositories, exact commits
- Machine record: `specs/pilot/live-java-repositories.yaml`
- Selection evidence: `docs/research/github-java-repository-selection-20260918.md`

| Repository | Fixed commit | Java level | Java files | Maven tests | ERF scan |
| --- | --- | ---: | ---: | ---: | --- |
| `davidmoten/word-wrap` | `53fbb8a` | 8 | 11 | 93 passed | 7 findings / complete `FAIL` |
| `gitbucket/markedj` | `4a158d0` | 8 | 41 | 40 passed | 4 findings / complete `FAIL` |
| `jacekkow/keycloak-protocol-cas` | `5d5b680` | 17 | 46 | 19 passed | 4 findings / complete `FAIL` |

The repositories were acquired from GitHub into `.scratch/`, analyzed locally,
and never modified remotely. The ERF scan used the pinned commit and produced
complete, repeatable Shadow evidence. No SCM credential, repository write,
source upload, or protected-branch status publication is part of this run.

样本继续用于 2026-10-02T00:00:00Z 至 2026-10-08T23:59:59Z（UTC inclusive）
的合同 1.1.0 窗口；治理批准、实现验收、人员/基线就绪均须在起点前完成，
否则不得激活或追溯生效（ADR-0176）。现有扫描仍是预检证据，不是 live pilot 结果。
