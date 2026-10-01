#!/usr/bin/env python3
"""Boundary doubles + optional real PMD end-to-end; only temporary repositories get hooks."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts/agent-commit-check.py"
GOOD = "public class A { void run() {} }\n"
BAD = "public class A { void run() { System.exit(1); } }\n"
CONFIG = """version: ruleops.config/v1
language: java
build_system: maven
modules: ["."]
source_roots: ["src/main/java"]
test_roots: []
generated_roots: []
vendor_roots: []
include: ["src/main/java/**"]
exclude: ["target/**"]
execution_mode: offline
cache_dir: .ruleops/cache
trust_state_path: .ruleops/trust-state
"""


def run(command, cwd, env=None):
    return subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, timeout=90)


def git(repo, *args):
    p = run(["git", *args], repo)
    assert p.returncode == 0, (args, p.stderr)
    return p.stdout.strip()


def setup(repo):
    repo.mkdir()
    git(repo, "init", "-q")
    git(repo, "config", "user.name", "Hook Test")
    git(repo, "config", "user.email", "hook-test@example.invalid")
    # Test-only config: never touch the working repository's hook settings.
    git(repo, "config", "core.hooksPath", ".git/hooks")
    (repo / ".ruleops").mkdir()
    (repo / ".ruleops/config.yaml").write_text(CONFIG)
    (repo / "pom.xml").write_text("<project><modelVersion>4.0.0</modelVersion><groupId>test</groupId><artifactId>test</artifactId><version>1</version></project>")
    java = repo / "src/main/java/A.java"
    java.parent.mkdir(parents=True)
    java.write_text(GOOD)
    git(repo, "add", ".")
    git(repo, "commit", "-qm", "base")
    scripts = repo / "scripts"
    scripts.mkdir()
    shutil.copy2(CHECK, scripts / CHECK.name)
    shutil.copy2(ROOT / "scripts/hooks/pre-commit", repo / ".git/hooks/pre-commit")
    return java


def invoke(repo, env, expected, code=None, extra=()):
    p = run([sys.executable, str(CHECK), *extra], repo, env)
    result = json.loads(p.stdout)
    assert p.returncode == expected, (result, p.stderr)
    if code:
        assert result.get("code", result["action"]) == code, result
    assert result["authority"] == "NONE"
    assert "System.exit" not in p.stdout and "private bypass explanation" not in p.stdout
    assert "src/main/java" not in p.stdout
    return result


def tests(out, real_rf, candidate, runtime_home):
    with tempfile.TemporaryDirectory(prefix="ruleops-hook-test-") as temporary:
        temp = Path(temporary)
        repo = temp / "repo"
        java = setup(repo)
        fake = temp / "fake-rf"
        fake.write_text("""#!/usr/bin/env python3
import json, os, pathlib, socket, sys, time
args = sys.argv
assert args[1] == 'scan'
assert args[args.index('--execution-mode') + 1] == 'offline'
assert args[args.index('--format') + 1] == 'json'
root = pathlib.Path(args[args.index('--repo-root') + 1])
files = list(root.rglob('*.java'))
assert len(files) == 1, files
assert socket.if_nameindex() == [(1, 'lo')]  # network namespace: loopback only
assert 'RULEOPS_DELIVERY_DESCRIPTOR' not in os.environ
mode = os.environ.get('TEST_SCAN_MODE', '')
if mode == 'timeout': time.sleep(30)
if mode == 'bad-json': print('not json'); sys.exit(0)
if mode == 'duplicate': print('{"kind":"ScanFailure","kind":"ScanResult"}'); sys.exit(0)
state = 'FAIL' if 'System.exit' in files[0].read_text() else 'PASS'
if mode in ('UNKNOWN', 'INCONCLUSIVE'): state = mode
result = dict(apiVersion='rulefactory.io/v1', kind='ScanResult', schemaVersion='scan-output-v1',
    gateDecision=state, authority='LOCAL_UNATTESTED', trust='LOCAL_UNATTESTED',
    status='LOCAL_UNATTESTED' if state == 'PASS' else state,
    planned=1, executed=1, failed=0, resultDigest='sha256:' + 'a'*64,
    completeness=dict(planned=1, accounted=1, failed=0, missing=0, observationErrors=0, complete=True))
if mode == 'incomplete': result['completeness']['complete'] = False
if mode == 'missing': del result['completeness']
if mode == 'unknown-status': result['status'] = 'UNKNOWN'
if mode == 'zero-plan': result['completeness']['planned'] = 0
print(json.dumps(result))
sys.exit(int(mode) if mode in ('2', '3', '4') else 0)
""")
        fake.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if not k.startswith(('RULEOPS_', 'GIT_'))}
        env.update(RULEOPS_COMMIT_RF=str(fake), RULEOPS_COMMIT_CANDIDATE=str(temp),
                   RULEOPS_DELIVERY_DESCRIPTOR='/must-not-be-used')
        java.write_text(BAD)
        git(repo, "add", str(java))
        java.write_text(GOOD)  # unstaged fix must not hide the staged violation
        bad = invoke(repo, env, 1, "FAIL")
        assert bad["scanExitCode"] == 0
        p = run(["git", "commit", "-qm", "must-block"], repo, env)
        assert p.returncode != 0 and '"code": "FAIL"' in p.stderr, (p.stdout, p.stderr)
        git(repo, "add", str(java))
        # Stage a real compliant change; equality with HEAD would be NOT_APPLICABLE.
        java.write_text(GOOD + "// compliant change\n")
        git(repo, "add", str(java))
        good = invoke(repo, env, 0, "ALLOW_LOCAL")
        unavailable = invoke(repo, {**env, "RULEOPS_COMMIT_RF": str(temp / "missing")}, 1, "VALIDATOR_MISSING")
        for mode in ("UNKNOWN", "INCONCLUSIVE", "bad-json", "duplicate", "incomplete", "missing", "unknown-status", "zero-plan", "2", "3", "4"):
            invoke(repo, {**env, "TEST_SCAN_MODE": mode}, 1)
        invoke(repo, {**env, "TEST_SCAN_MODE": "timeout"}, 1, "SCAN_TIMEOUT", ("--timeout", "0.2"))
        invoke(repo, {**env, "RULEOPS_COMMIT_SKIP_REASON": ""}, 1, "SKIP_REASON_REQUIRED")
        adapter = temp / "claude-pretool.py"
        example = (ROOT / "docs/agent-commit-hook.md").read_text().split("```python\n", 1)[1].split("```", 1)[0]
        adapter.write_text(example)
        payload = json.dumps(dict(tool_name="Bash", tool_input=dict(command="git commit -m test"), cwd=str(repo)))
        for scanner, expected in ((str(fake), 0), (str(temp / "missing"), 2)):
            p = subprocess.run([sys.executable, str(adapter)], input=payload, text=True,
                               capture_output=True, timeout=90,
                               env={**env, "RULEOPS_COMMIT_CHECK": str(CHECK), "RULEOPS_COMMIT_RF": scanner})
            assert p.returncode == expected, (p.returncode, p.stderr)
        skipped = invoke(repo, {**env, "RULEOPS_COMMIT_SKIP_REASON": "private bypass explanation"}, 0, "SKIPPED")
        assert skipped["reasonDigest"].startswith("sha256:")
        logs = (repo / ".git/ruleops-commit-audit.jsonl").read_text()
        assert 'private bypass explanation' not in logs and 'System.exit' not in logs
        audit = repo / ".git/ruleops-commit-audit.jsonl"
        backup = repo / ".git/saved-audit"
        audit.rename(backup)
        audit.symlink_to(backup)
        invoke(repo, env, 1, "AUDIT_UNAVAILABLE")
        audit.unlink()
        backup.rename(audit)
        p = run(["git", "commit", "-qm", "compliant"], repo, env)
        assert p.returncode == 0, (p.stdout, p.stderr)
        (repo / "README.md").write_text("non-Java change")
        git(repo, "add", "README.md")
        invoke(repo, {**env, "RULEOPS_COMMIT_RF": "missing"}, 0, "NOT_APPLICABLE")
        java.write_text(GOOD + "// another change\n")
        git(repo, "add", str(java))
        other = java.with_name("B.java")
        other.write_text("class B {}")
        git(repo, "add", str(other))
        result = invoke(repo, env, 0, "ALLOW_LOCAL")
        assert result["fileCount"] == len(result["resultDigests"]) == 2
        other.unlink()
        other.symlink_to(java)
        git(repo, "add", str(other))
        invoke(repo, env, 1, "UNSUPPORTED_INDEX_MODE")
        evidence = {"boundaryChecks": "passed", "violation": bad, "compliant": good, "unavailable": unavailable}
        if real_rf:
            real = temp / "real-repo"
            real_java = setup(real)
            real_env = {**env, "RULEOPS_COMMIT_RF": str(Path(real_rf).resolve()),
                        "RULEOPS_COMMIT_CANDIDATE": str(Path(candidate).resolve())}
            if runtime_home:
                real_env["RULEOPS_RUNTIME_HOME"] = str(Path(runtime_home).resolve())
            real_java.write_text(BAD)
            git(real, "add", str(real_java))
            real_java.write_text(GOOD)
            real_bad = invoke(real, real_env, 1, "FAIL")
            p = run(["git", "commit", "-qm", "real-violation"], real, real_env)
            assert p.returncode != 0, (p.stdout, p.stderr)
            real_bad_commit_rc = p.returncode
            real_java.write_text(GOOD + "// compliant change\n")
            git(real, "add", str(real_java))
            real_good = invoke(real, real_env, 0, "ALLOW_LOCAL")
            excluded = real / "outside/B.java"
            excluded.parent.mkdir()
            excluded.write_text("class B {}")
            git(real, "add", str(excluded))
            invoke(real, real_env, 1)  # another successful file must not hide excluded coverage
            git(real, "reset", "-q", "HEAD", "--", str(excluded))
            p = run(["git", "commit", "-qm", "real-compliant"], real, real_env)
            assert p.returncode == 0, (p.stdout, p.stderr)
            real_java.write_text(BAD)
            git(real, "add", str(real_java))
            real_missing = invoke(real, {**real_env, "RULEOPS_COMMIT_RF": str(temp / "missing")}, 1, "VALIDATOR_MISSING")
            evidence["realPmd"] = dict(violation=real_bad, compliant=real_good, unavailable=real_missing,
                                       violationCommitRc=real_bad_commit_rc, compliantCommitRc=p.returncode)
        out.mkdir(parents=True, exist_ok=True)
        (out / "results.json").write_text(json.dumps(evidence, indent=2) + "\n")
        print("agent-commit-hook-check=PASS real_pmd=" + str(bool(real_rf)).lower())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--real-rf")
    parser.add_argument("--candidate")
    parser.add_argument("--runtime-home")
    args = parser.parse_args()
    if args.real_rf and not args.candidate:
        parser.error("--real-rf requires --candidate")
    tests(args.output, args.real_rf, args.candidate, args.runtime_home)
