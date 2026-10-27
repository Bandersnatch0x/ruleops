# Evidence-claim linter (local research/audit aid)

`scripts/evidence-claim-lint.sh` checks a small, repository-local
`evidence-claims-v1` manifest. It does **not** change runtime trust, gate, or
protocol behavior and does not decide whether a claim is true.

Every claim must carry:

| Field | Meaning |
| --- | --- |
| `source` | repository-relative `file:line` location for the claim or its evidence |
| `command` | command a reviewer can run again (aliases `replayCommand`/`rerunCommand` are accepted) |
| `evidenceLevel` | declared level (`A`–`E`, `STRUCTURAL`, `EXECUTABLE`, `INDEPENDENT`, `REVIEWED`, or `INCONCLUSIVE`) |
| `commitSha` | the commit whose files were inspected (7–64 hexadecimal characters) |
| `blocker` | who/what blocks the claim, or `none` |
| `consumer` | the person/role that consumes the claim |

The command may be a test, build, parser, or other replayable check. A command
made only of `grep`, `rg`, `wc`, or `count` stages is useful for triage but is
reported as `WARN` and **can never be `PASS`**. Missing or malformed fields are
`FAIL`. Exit codes are 0 (`PASS`), 2 (`WARN`), and 1 (`FAIL`).

JSON is the canonical format:

```json
{
  "schemaVersion": "evidence-claims-v1",
  "claims": [{
    "id": "C-1",
    "text": "...",
    "source": "docs/research/audit.md:42",
    "command": "python3 scripts/check.py",
    "evidenceLevel": "EXECUTABLE",
    "commitSha": "0123456789abcdef0123456789abcdef01234567",
    "blocker": "none",
    "consumer": "audit reviewer"
  }]
}
```

Run the shell regression without Java, Maven, Cargo, network, or production
trust assets:

```sh
scripts/evidence-claim-lint-test.sh
```
