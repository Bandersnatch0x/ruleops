# GitHub Java repository selection — 2026-09-18

## Result

Three public GitHub repositories were selected for the approved Shadow-only
pilot sample. Each is non-fork, non-archived, has a root `pom.xml`, uses an
open license, is below the small-project limits, and was pinned to the exact
commit acquired during this run.

| Repository | GitHub source | Pinned commit | Java files / tracked files | Java level | Maven tests | ERF scan |
| --- | --- | --- | ---: | ---: | ---: | --- |
| `davidmoten/word-wrap` | [GitHub repository](https://github.com/davidmoten/word-wrap) | `53fbb8aec1b2c6f6f6150b7405aec94dcabdb364` | 11 / 22 | 8 | 93 passed | complete, 7 findings, `FAIL` |
| `gitbucket/markedj` | [GitHub repository](https://github.com/gitbucket/markedj) | `4a158d00562be394462ba1f6d088d1a1e485efa7` | 41 / 86 | 8 | 40 passed | complete, 4 findings, `FAIL` |
| `jacekkow/keycloak-protocol-cas` | [GitHub repository](https://github.com/jacekkow/keycloak-protocol-cas) | `5d5b68094fbbd611b3d2215ce256c7f83fedbd5` | 46 / 64 | 17 | 19 passed | complete, 4 findings, `FAIL` |

GitHub describes `word-wrap` as a Java word-wrapping library and documents
Maven use; `markedj` is a JVM Markdown processor with an Apache-2.0 license;
`keycloak-protocol-cas` is an Apache-2.0 Keycloak CAS protocol provider. The
repository pages are the primary sources for those project descriptions and
licenses.

## Selection method

The GitHub API was used to filter for public, non-fork, non-archived Java
repositories with a GitHub size below 500 KB. Candidates were then shallow
cloned and checked locally for a root Maven build, source size, Java level and
testability. The final sample uses distinct owners and deliberately includes a
Java 17 project alongside two Java 8 projects.

The Maven preflight command was:

```text
mvn -B -ntp test
```

The ERF scan used the fixed `corp.java.avoid-duplicate-literals` Candidate,
the SAFE_STATIC Maven resolver, and the real Java PMD Backend Host. Each
selected repository was scanned twice at the same commit; both runs had
`planned=1`, `executed=1`, `failed=0`, `complete=true`, and identical plan and
semantic-result identities.

## Evidence

All acquisition, build logs and scan logs are under
`.scratch/github-java-repositories-20260918/`. The machine-readable selection
record is [`specs/pilot/live-java-repositories.yaml`](../../specs/pilot/live-java-repositories.yaml).

The original local-fixture manifest remains separate and unchanged. These
repositories are a selected external sample, not protected-branch authority;
the live measurement window and its digest-only outcome review remain governed
by the approved Pilot Success Contract.

## Rejected or deferred candidates

- `jhalterman/expiringmap` was not selected because its Maven test run fails
  under the current Surefire/TestNG compatibility requirement (`TestNG 6.8.8`
  is below the required `6.14.3`).
- `Cosium/git-code-format-maven-plugin` was not selected because its Maven
  integration tests require a generated workspace-state file and fail from the
  current non-ASCII checkout path; it was not needed once three clean samples
  were available.
- `poetix/protonpack` and `spring-projects/spring-plugin` both passed their
  local Maven tests, but were kept as backups after the final sample reached
  the requested count with better source-level and domain spread.
