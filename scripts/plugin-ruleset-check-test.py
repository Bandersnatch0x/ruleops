#!/usr/bin/env python3
"""Offline integration check using REAL local PMD, no mocks or downloads.

Usage: python3 scripts/plugin-ruleset-check-test.py PMD_HOME NEW_OUTPUT_DIRECTORY
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

GATE = Path(__file__).with_name("plugin-ruleset-check.py")


def main():
    pmd_home, out = (Path(value).resolve() for value in sys.argv[1:])
    out.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="fixture-", dir=out) as temporary:
        repo = Path(temporary)
        subprocess.run(["git", "init", "-q", str(repo)], check=True, timeout=10)
        main = repo / "src/main/java/Clean.java"
        test = repo / "src/test/java/Test.java"
        main.parent.mkdir(parents=True)
        test.parent.mkdir(parents=True)
        good = "package sample; public enum Clean { VALUE }\n"
        bad = "package sample; public class Bad { boolean same(Object other) { return other.equals(null); } }\n"
        main.write_text(good)
        test.write_text("invalid Java must never reach the engine")
        subprocess.run(["git", "add", "."], cwd=repo, check=True, timeout=10)
        (main.parent / "Untracked.java").write_text(bad)
        rules = repo / "rules.xml"
        rules.write_text('<ruleset name="test" xmlns="http://pmd.sourceforge.net/ruleset/2.0.0">'
                         '<description>Gate fixture only</description>'
                         '<rule ref="category/java/errorprone.xml/EqualsNull"/></ruleset>')

        def run(name, expected_rc, home=pmd_home):
            output = out / (name + ".json")
            process = subprocess.run([sys.executable, str(GATE), str(repo), str(home), str(rules), str(output)],
                                     capture_output=True, timeout=70)
            assert process.returncode == expected_rc, (name, process.returncode, process.stdout, process.stderr)
            report = json.loads(output.read_text())
            text = output.read_text()
            assert all(secret not in text for secret in (str(repo), "Clean.java", "other.equals"))
            assert all(not isinstance(value, (dict, list)) for value in report.values())
            return report

        passed = run("passed", 0)
        assert passed["inputFileCount"] == 1 and passed["violationCount"] == 0
        repeat = run("repeat", 0)
        for key in ("inputDigest", "rulesetDigest", "toolDigest", "analysisDigest"):
            assert repeat[key] == passed[key], key
        main.write_text(bad)
        violated = run("violations", 1)
        assert violated["violationCount"] == 1 and violated["engineExitCode"] == 4
        assert violated["inputDigest"] != passed["inputDigest"]
        main.write_text("invalid Java must fail explicitly")
        failed = run("processing-error", 2)
        assert failed["processingErrorCount"] > 0
        main.write_text(good)
        rules.write_text("not XML")
        assert run("invalid-ruleset", 2)["status"] == "tool_error"
        assert run("missing-tool", 2, repo / "absent")["errorCode"] == "FileNotFoundError"
        subprocess.run(["git", "rm", "--cached", "-q", str(main)], cwd=repo, check=True, timeout=10)
        assert run("empty-scope", 2)["errorCode"] == "empty_scope"
        assert main.read_text() == good and test.read_text().startswith("invalid Java")
        assert not (repo / ".pmdCache").exists()

        collision = subprocess.run([sys.executable, str(GATE), str(repo), str(pmd_home), str(rules),
                                    str(out / "passed.json")], capture_output=True, timeout=10)
        assert collision.returncode == 2
        assert json.loads((out / "passed.json").read_text()) == passed

    print("plugin-ruleset-check=PASS clean/repeat/violation/processing/config/missing/empty/no-overwrite/privacy")


if __name__ == "__main__":
    main()
