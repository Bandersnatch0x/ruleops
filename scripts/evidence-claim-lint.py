#!/usr/bin/env python3
"""Check provenance completeness for local audit/research evidence claims.

The tool intentionally does not decide whether a claim is true.  It only makes
the provenance needed for review explicit: a source ``file:line``, a command
that can be run again, an evidence level, a commit SHA, and both blocker and
consumer responsibility.  A text-search/count command by itself is useful
triage, but is warning-only and can never result in PASS.

Input is either a JSON manifest or Markdown containing one JSON marker per
claim, for example::

    <!-- evidence-claim {"id":"C-1", "text":"...", ...} -->

Exit status is 0 for PASS, 2 for WARN, and 1 for FAIL.
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


SCHEMA = "evidence-claims-v1"
LEVELS = {
    "A",
    "B",
    "C",
    "D",
    "E",
    "STRUCTURAL",
    "EXECUTABLE",
    "INDEPENDENT",
    "REVIEWED",
    "INCONCLUSIVE",
}
SHA = re.compile(r"^[0-9a-fA-F]{7,64}$")
SOURCE = re.compile(r"^(?P<file>[^\n:]+):(?P<line>[1-9][0-9]*)$")
WEAK = {"grep", "rg", "egrep", "fgrep", "wc", "count"}


@dataclass
class Finding:
    severity: str
    claim: str
    message: str


@dataclass
class Result:
    claims: int = 0
    findings: list[Finding] = field(default_factory=list)
    input_error: str | None = None

    @property
    def status(self) -> str:
        if self.input_error or any(x.severity == "ERROR" for x in self.findings):
            return "FAIL"
        if any(x.severity == "WARN" for x in self.findings):
            return "WARN"
        return "PASS"


def load(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        return None, f"cannot read {path}: {exc}"

    try:
        data = json.loads(raw)
        if isinstance(data, list):
            return {"schemaVersion": SCHEMA, "claims": data}, None
        if isinstance(data, dict):
            return data, None
        return None, "JSON root must be an object or array"
    except json.JSONDecodeError:
        pass

    claims: list[dict[str, Any]] = []
    marker = re.compile(r"<!--\s*evidence-claim\s+(\{.*?\})\s*-->")
    for match in marker.finditer(raw):
        try:
            item = json.loads(match.group(1))
        except json.JSONDecodeError as exc:
            return None, f"invalid evidence-claim marker: {exc}"
        if not isinstance(item, dict):
            return None, "evidence-claim marker must contain a JSON object"
        claims.append(item)
    if claims:
        return {"schemaVersion": SCHEMA, "claims": claims}, None
    return None, "input is neither a JSON manifest nor a Markdown evidence-claim document"


def weak_command(command: str) -> bool:
    """Whether every command stage is a grep/count primitive."""
    stages = re.split(r"\s*(?:\|\||&&|[;|])\s*", command.strip().strip("`"))
    bases: list[str] = []
    for stage in stages:
        if not stage.strip():
            continue
        try:
            tokens = shlex.split(stage)
        except ValueError:
            tokens = stage.split()
        if not tokens:
            continue
        i = 0
        if tokens[0] in {"env", "sudo"}:
            i = 1
            while i < len(tokens) and ("=" in tokens[i] or tokens[i].startswith("-")):
                i += 1
        if i >= len(tokens):
            continue
        base = Path(tokens[i]).name.lower()
        if base == "git" and len(tokens) > i + 1:
            base = tokens[i + 1].lower()
        bases.append(base)
    return bool(bases) and all(base in WEAK for base in bases)


def source_value(c: dict[str, Any]) -> str | None:
    value = c.get("source") or c.get("fileLine") or c.get("location")
    if isinstance(value, str):
        return value
    file_value, line_value = c.get("file"), c.get("line")
    if isinstance(file_value, str) and isinstance(line_value, int):
        return f"{file_value}:{line_value}"
    return None


def nonempty(c: dict[str, Any], *keys: str) -> str | None:
    for key in keys:
        value = c.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def lint(data: dict[str, Any]) -> Result:
    result = Result()
    version = data.get("schemaVersion")
    if version not in (None, SCHEMA):
        result.input_error = f"unsupported schemaVersion {version!r}; expected {SCHEMA}"
        return result
    claims = data.get("claims")
    if not isinstance(claims, list):
        result.input_error = "manifest must contain a claims array"
        return result
    result.claims = len(claims)
    seen: set[str] = set()
    for n, value in enumerate(claims, 1):
        cid = str(value.get("id") or value.get("claimId") or f"claim-{n}") if isinstance(value, dict) else f"claim-{n}"
        if cid in seen:
            result.findings.append(Finding("ERROR", cid, "duplicate claim id"))
        seen.add(cid)
        if not isinstance(value, dict):
            result.findings.append(Finding("ERROR", cid, "claim must be a JSON object"))
            continue

        if not nonempty(value, "text", "claim"):
            result.findings.append(Finding("ERROR", cid, "missing claim text"))

        source = source_value(value)
        if not source:
            result.findings.append(Finding("ERROR", cid, "missing source file:line"))
        else:
            parsed = SOURCE.match(source)
            if not parsed:
                result.findings.append(Finding("ERROR", cid, "source must be file:line with a positive line"))
            elif Path(parsed.group("file")).is_absolute():
                result.findings.append(Finding("ERROR", cid, "source file must be repository-relative"))

        command = nonempty(value, "command", "replayCommand", "rerunCommand", "reproCommand")
        if not command:
            result.findings.append(Finding("ERROR", cid, "missing replay command"))
        elif weak_command(command):
            result.findings.append(Finding("WARN", cid, "grep/count-only command is warning-only; claim cannot PASS"))

        level = nonempty(value, "evidenceLevel", "level")
        if not level or level.upper() not in LEVELS:
            result.findings.append(Finding("ERROR", cid, f"evidence level must be one of {sorted(LEVELS)}"))

        commit = nonempty(value, "commitSha", "commit", "sha")
        if not commit or not SHA.fullmatch(commit):
            result.findings.append(Finding("ERROR", cid, "commitSha must be a 7-64 character hexadecimal git SHA"))

        if not nonempty(value, "blocker", "blockerResponsibility"):
            result.findings.append(Finding("ERROR", cid, "missing blocker responsibility (use 'none' when clear)"))
        if not nonempty(value, "consumer", "consumerResponsibility"):
            result.findings.append(Finding("ERROR", cid, "missing consumer responsibility"))
    return result


def render(path: Path, result: Result) -> str:
    lines = [
        "# Evidence Claim Lint",
        "",
        f"- input: {path}",
        f"- schema: {SCHEMA}",
        f"- claims: {result.claims}",
        f"- status: {result.status}",
        "- grep/count-only commands are warning-only and never PASS",
    ]
    if result.input_error:
        lines.append(f"- ERROR: {result.input_error}")
    for item in result.findings:
        lines.append(f"- {item.severity} [{item.claim}]: {item.message}")
    if not result.input_error and not result.findings:
        lines.append("- all required provenance fields are present")
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args(argv)
    data, error = load(args.manifest)
    result = Result(input_error=error) if error else lint(data or {})
    output = render(args.manifest, result)
    print(output, end="")
    if args.report:
        args.report.write_text(output, encoding="utf-8")
    return {"PASS": 0, "WARN": 2, "FAIL": 1}[result.status]


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
