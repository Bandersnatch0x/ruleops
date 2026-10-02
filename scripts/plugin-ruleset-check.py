#!/usr/bin/env python3
"""Opt-in offline PMD gate; never changes ERF policy or publishes raw findings.

Usage: python3 scripts/plugin-ruleset-check.py REPO PMD_HOME RULESET OUTPUT.json
Requires a local PMD 7 distribution and Java on PATH/JAVA_HOME. Scans the working
copies of tracked Java files, excluding src/test (not staged snapshots or diff).
Exit: 0 clean, 1 violations, 2 input/tool failure. OUTPUT must not already exist.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def digest(value):
    data = value if isinstance(value, bytes) else json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    return "sha256:" + hashlib.sha256(data).hexdigest()


def inventory(repo):
    result = subprocess.run(["git", "ls-files", "-z", "--", "*.java"], cwd=repo,
                            capture_output=True, check=True, timeout=10)
    names = sorted(name for name in result.stdout.decode().split("\0")
                   if name and "/src/test/" not in "/" + name)
    if not names:
        raise RuntimeError("empty_scope")
    files = {}
    for name in names:
        path = repo / name
        # PMD file lists use lines/commas; symlinks could cross the source boundary.
        if any(char in name for char in "\r\n,") or not path.resolve().is_relative_to(repo):
            raise RuntimeError("unsafe_source_path")
        if any(part.is_symlink() for part in [path, *path.parents] if part != repo):
            raise RuntimeError("symlink_source_path")
        files[name] = digest(path.read_bytes())
    return files


def summarize(result):
    try:
        data = json.loads(result.stdout)
        groups = ("files", "processingErrors", "configurationErrors", "suppressedViolations")
        if not isinstance(data["pmdVersion"], str) or any(
                not isinstance(data[key], list) for key in groups):
            raise ValueError("invalid PMD envelope")
        if any(not isinstance(item["violations"], list) for item in data["files"]):
            raise ValueError("invalid PMD violations")
        count = sum(len(item["violations"]) for item in data["files"])
    except (ValueError, KeyError, TypeError) as error:
        raise RuntimeError("invalid_engine_report") from error
    failed = bool(data["processingErrors"] or data["configurationErrors"])
    expected_rc = 4 if count else 0
    failed = failed or result.returncode != expected_rc
    return {
        "status": "tool_error" if failed else "violations" if count else "passed",
        "engineVersion": data["pmdVersion"], "violationCount": count,
        "processingErrorCount": len(data["processingErrors"]),
        "configurationErrorCount": len(data["configurationErrors"]),
        "suppressedCount": len(data["suppressedViolations"]),
        # Omit timestamp; sort findings so a repeated scan has the same identity.
        "analysisDigest": digest({key: sorted(data[key], key=lambda v: json.dumps(v, sort_keys=True))
                                  for key in groups}),
    }


def check(repo, pmd_home, ruleset):
    report = {"schemaVersion": 1, "stage": "reference-pmd", "status": "tool_error",
              "scope": "tracked-working-tree-java-excluding-src-test", "languageVersion": "java-21"}
    try:
        repo, pmd_home, ruleset = repo.resolve(strict=True), pmd_home.resolve(strict=True), ruleset.resolve(strict=True)
        launcher = pmd_home / "bin/pmd"
        launcher.read_bytes()  # Missing local tooling must fail before invoking a shell.
        report["rulesetDigest"] = digest(ruleset.read_bytes())
        report["toolDigest"] = digest({str(p.relative_to(pmd_home)): digest(p.read_bytes())
                                       for p in sorted(pmd_home.rglob("*")) if p.is_file()})
        files = inventory(repo)
        report.update(inputFileCount=len(files), inputDigest=digest(files))
        with tempfile.TemporaryDirectory(prefix="erf-pmd-") as temporary:
            file_list = Path(temporary) / "files.txt"
            file_list.write_text("\n".join(files) + "\n", encoding="utf-8")
            result = subprocess.run([
                "bash", str(launcher), "check", "--rulesets", str(ruleset),
                "--file-list", str(file_list), "--format", "json", "--no-cache",
                "--no-progress", "--threads", "1", "--use-version", "java-21",
                "--relativize-paths-with", str(repo),
            ], cwd=repo, capture_output=True, timeout=60)
        report.update(engineExitCode=result.returncode, diagnosticsDigest=digest(result.stderr),
                      diagnosticsBytes=len(result.stderr), rawReportDigest=digest(result.stdout))
        report.update(summarize(result))
        if inventory(repo) != files:
            raise RuntimeError("inputs_changed_during_scan")
    except RuntimeError as error:
        report.update(status="tool_error", errorCode=str(error))
    except (OSError, UnicodeError, subprocess.SubprocessError) as error:
        report.update(status="tool_error", errorCode=type(error).__name__, errorDigest=digest(str(error).encode()))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "pmd_home", "ruleset", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    # Exclusive creation prevents overwriting source or earlier evidence, even on failure.
    try:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x", encoding="utf-8") as output:
            report = check(args.repo, args.pmd_home, args.ruleset)
            output.write(json.dumps(report, indent=2, sort_keys=True) + "\n")
    except OSError as error:
        report = {"status": "tool_error", "errorCode": type(error).__name__}
    print(json.dumps(report, sort_keys=True))
    return {"passed": 0, "violations": 1, "tool_error": 2}[report["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
