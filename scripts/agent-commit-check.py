#!/usr/bin/env python3
"""Opt-in local index check; consumes rf JSON, never issues Gate authority."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time


class Blocked(Exception):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, timeout=10).stdout


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Blocked("DUPLICATE_JSON_FIELD")
        value[key] = item
    return value


def verdict(raw, rc):
    if rc:
        raise Blocked("SCAN_TIMEOUT" if rc in (124, 137) else "VALIDATOR_UNAVAILABLE")
    result = json.loads(raw, object_pairs_hook=unique_object)
    if not isinstance(result, dict):
        raise Blocked("INVALID_SCAN_RESULT")
    if (result.get("kind") != "ScanResult" or result.get("schemaVersion") != "scan-output-v1"
            or result.get("apiVersion") != "rulefactory.io/v1"
            or result.get("authority") not in ("LOCAL_UNATTESTED", "TRUSTED")):
        raise Blocked("INVALID_SCAN_RESULT")
    state = result.get("gateDecision")
    if state != "PASS":
        raise Blocked(state if state in ("FAIL", "INCONCLUSIVE") else "UNKNOWN")
    expected_status = "LOCAL_UNATTESTED" if result["authority"] == "LOCAL_UNATTESTED" else "PASS"
    if result.get("status") != expected_status or result.get("trust") != result["authority"]:
        raise Blocked("UNKNOWN_STATUS")
    if any(type(result.get(key)) is not int for key in ("planned", "executed", "failed")):
        raise Blocked("INVALID_COMPLETENESS")
    c = result.get("completeness")
    if not isinstance(c, dict) or c.get("complete") is not True:
        raise Blocked("INCOMPLETE")
    for key in ("planned", "accounted", "failed", "missing", "observationErrors"):
        if type(c.get(key)) is not int or c[key] < 0:
            raise Blocked("INVALID_COMPLETENESS")
    if (c["planned"] < 1 or c["planned"] != c["accounted"]
            or any(c[key] for key in ("failed", "missing", "observationErrors"))
            or result.get("planned") != c["planned"]
            or result.get("executed") != c["planned"] or result.get("failed") != 0):
        raise Blocked("INCOMPLETE")
    result_digest = result.get("resultDigest")
    if not isinstance(result_digest, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", result_digest):
        raise Blocked("INVALID_RESULT_DIGEST")
    return result_digest


def materialize(snapshot, entries, names):
    for name in names:
        mode, oid = entries[name]
        if mode not in ("100644", "100755"):
            raise Blocked("UNSUPPORTED_INDEX_MODE")
        target = snapshot / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git("cat-file", "blob", oid))


def check(args, record):
    tree = git("write-tree").strip()
    record["indexDigest"] = digest(tree)
    changed = git("diff", "--cached", "--name-only", "--diff-filter=ACMRT", "-z").decode().split("\0")
    names = sorted(name for name in changed if name.endswith(".java"))
    if git("write-tree").strip() != tree:
        raise Blocked("INDEX_CHANGED")
    record["fileCount"] = len(names)
    if args.skip_reason is not None:
        if not args.skip_reason.strip():
            raise Blocked("SKIP_REASON_REQUIRED")
        record["reasonDigest"] = digest(args.skip_reason.encode())
        return "SKIPPED"
    if not names:
        return "NOT_APPLICABLE"
    if not args.candidate:
        raise Blocked("CANDIDATE_REQUIRED")
    candidate = str(Path(args.candidate).resolve(strict=True))
    rf = shutil.which(args.rf)
    if not rf:
        raise Blocked("VALIDATOR_MISSING")
    rf = str(Path(rf).resolve())
    entries = {}
    for item in git("ls-tree", "-r", "-z", tree.decode()).decode().split("\0"):
        if item:
            meta, name = item.split("\t", 1)
            mode, _, oid = meta.split()
            entries[name] = (mode, oid)
    metadata = {name for name in entries if Path(name).name in
                ("pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts", "gradle.properties")}
    metadata.add(".ruleops/config.yaml")
    if not metadata.issubset(entries):
        raise Blocked("STAGED_CONFIG_REQUIRED")
    # CLI --include is CSV/glob based: reject names it cannot represent exactly.
    if any(name != name.strip() or any(c in name for c in ",*?[]\\")
           or any(ord(c) < 32 for c in name) for name in names):
        raise Blocked("UNREPRESENTABLE_PATH")
    deadline = time.monotonic() + args.timeout
    record["resultDigests"] = []
    # ponytail: one scan per file proves no file was silently excluded; batch once rf exposes file coverage.
    for name in names:
        with tempfile.TemporaryDirectory(prefix="ruleops-index-") as temporary:
            snapshot = Path(temporary)
            materialize(snapshot, entries, metadata | {name})
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise Blocked("SCAN_TIMEOUT")
            command = ["timeout", "--kill-after=2s", str(remaining), "unshare", "--user",
                       "--map-root-user", "--net", "--", rf, "scan", "--repo-root", temporary,
                       "--project-config", str(snapshot / ".ruleops/config.yaml"),
                       "--candidate", candidate, "--execution-mode", "offline",
                       "--include", name, "--format", "json", "--progress", "none"]
            if args.runtime_home:
                command += ["--runtime-home", str(Path(args.runtime_home).resolve(strict=True))]
            env = os.environ.copy()
            # Candidate-only local execution must not inherit a delivery or server request.
            env.pop("RULEOPS_DELIVERY_DESCRIPTOR", None)
            scan = subprocess.run(command, capture_output=True, env=env)
            record["scanExitCode"] = scan.returncode
            record["outputDigest"] = digest(scan.stdout + scan.stderr)
            record["resultDigests"].append(verdict(scan.stdout, scan.returncode))
    if git("write-tree").strip() != tree:
        raise Blocked("INDEX_CHANGED")
    return "ALLOW_LOCAL"


def audit(record):
    path = Path(os.fsdecode(git("rev-parse", "--git-path", "ruleops-commit-audit.jsonl").strip()))
    fd = os.open(path, os.O_APPEND | os.O_CREAT | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a", encoding="utf-8") as output:
        output.write(json.dumps(record, sort_keys=True) + "\n")
        output.flush()
        os.fsync(output.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rf", default=os.environ.get("RULEOPS_COMMIT_RF", "rf"))
    parser.add_argument("--candidate", default=os.environ.get("RULEOPS_COMMIT_CANDIDATE"))
    parser.add_argument("--runtime-home", default=os.environ.get("RULEOPS_RUNTIME_HOME"))
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--skip-reason", default=os.environ.get("RULEOPS_COMMIT_SKIP_REASON"))
    args = parser.parse_args()
    record = {"kind": "LocalCommitCheck", "authority": "NONE", "timestamp": int(time.time())}
    try:
        if not math.isfinite(args.timeout) or args.timeout <= 0:
            raise Blocked("INVALID_TIMEOUT")
        os.chdir(os.fsdecode(git("rev-parse", "--show-toplevel").strip()))
        record["action"] = check(args, record)
    except (Blocked, OSError, ValueError, subprocess.SubprocessError) as error:
        record["action"] = "BLOCK"
        record["code"] = str(error) if isinstance(error, Blocked) else type(error).__name__
        record["errorDigest"] = digest(str(error).encode())
    try:
        audit(record)
    except (OSError, subprocess.SubprocessError) as error:
        record.update(action="BLOCK", code="AUDIT_UNAVAILABLE", errorDigest=digest(str(error).encode()))
    print(json.dumps(record, sort_keys=True))
    return 1 if record["action"] == "BLOCK" else 0


if __name__ == "__main__":
    sys.exit(main())
