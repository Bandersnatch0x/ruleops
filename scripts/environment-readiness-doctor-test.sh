#!/usr/bin/env bash
# Shell regression test for the diagnostic-only environment readiness doctor.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
DOCTOR="$REPO_ROOT/scripts/environment-readiness-doctor.sh"
OUT="$(mktemp -d "${TMPDIR:-/tmp}/ruleops-environment-readiness.XXXXXX")"
trap 'rm -rf -- "$OUT"' EXIT

bash -n "$DOCTOR"
grep -q 'environment-readiness-v1' "$DOCTOR"
grep -q 'diagnosticOnly' "$DOCTOR"
grep -q 'networkChanges' "$DOCTOR"

fake_bin="$OUT/bin"
cache="$OUT/m2/repository"
mkdir -p "$fake_bin" "$cache/org/example/app/1.0"
printf '%s\n' 'fixture artifact' >"$cache/org/example/app/1.0/app-1.0.jar"
cat >"$fake_bin/java" <<'TOOL'
#!/bin/sh
printf '%s\n' 'openjdk version "21.0.1" 2026-01-01' >&2
TOOL
cat >"$fake_bin/javac" <<'TOOL'
#!/bin/sh
printf '%s\n' 'javac 21.0.1'
TOOL
cat >"$fake_bin/cargo" <<'TOOL'
#!/bin/sh
printf '%s\n' 'cargo 1.90.0 (fixture)'
TOOL
cat >"$fake_bin/mvn" <<'TOOL'
#!/bin/sh
printf '%s\n' 'Apache Maven 3.9.9 (fixture)'
TOOL
cat >"$fake_bin/protoc" <<'TOOL'
#!/bin/sh
printf '%s\n' 'libprotoc 27.1'
TOOL
chmod +x "$fake_bin"/*

python3_bin="$(command -v python3)"
python3_dir="$(dirname "$python3_bin")"
PATH="$fake_bin:/usr/bin:/bin:$python3_dir" \
  MAVEN_REPO_LOCAL="$cache" \
  MAVEN_DNS_HOST=localhost \
  "$DOCTOR" "$OUT/ready" >"$OUT/ready.stdout" 2>"$OUT/ready.stderr"
python3 - "$OUT/ready/environment-readiness.json" "$OUT/ready/commands.log" <<'PY'
import json
import pathlib
import sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert report["schemaVersion"] == "environment-readiness-v1"
assert report["kind"] == "EnvironmentReadinessReport"
assert report["overall"] == "READY", report
assert report["diagnosticOnly"] is True
assert report["readOnly"] is True
assert report["networkChanges"] is False
assert report["trust"] == "NOT_EVALUATED"
assert report["gate"] == "NOT_RUN"
assert {check["id"] for check in report["checks"]} == {"java", "javac", "cargo", "mvn", "protoc", "maven-dns", "maven-cache"}
assert all(check["status"] == "READY" for check in report["checks"])
assert pathlib.Path(sys.argv[2]).stat().st_size > 0
assert "PASS" not in json.dumps(report)
PY

# A stale Maven marker remains a truthful BLOCKED fact, not a fabricated READY.
printf '%s\n' 'download failed' >"$cache/org/example/app/1.0/app-1.0.jar.lastUpdated"
PATH="$fake_bin:/usr/bin:/bin:$python3_dir" \
  MAVEN_REPO_LOCAL="$cache" \
  MAVEN_DNS_HOST=localhost \
  "$DOCTOR" "$OUT/blocked" >"$OUT/blocked.stdout" 2>"$OUT/blocked.stderr"
python3 - "$OUT/blocked/environment-readiness.json" <<'PY'
import json
import pathlib
import sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert report["overall"] == "BLOCKED"
cache = next(check for check in report["checks"] if check["id"] == "maven-cache")
assert cache["status"] == "BLOCKED"
assert cache["lastUpdatedFileCount"] == 1
PY

set +e
PATH="$fake_bin:/usr/bin:/bin:$python3_dir" \
  MAVEN_REPO_LOCAL="$cache" \
  MAVEN_DNS_HOST=localhost \
  "$DOCTOR" --strict "$OUT/strict" >"$OUT/strict.stdout" 2>"$OUT/strict.stderr"
strict_rc=$?
set -e
[ "$strict_rc" -eq 3 ]

echo "environment-readiness-doctor-test=PASS"
