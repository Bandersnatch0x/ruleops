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

This sample is now ready for the approved 2026-10-01 through 2026-10-31
observation window. The current scan results are preflight evidence, not live
pilot outcome metrics.
