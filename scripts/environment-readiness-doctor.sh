#!/usr/bin/env bash
# Read-only toolchain and Maven-environment diagnostic.
#
# This script deliberately does not invoke a build, resolve a dependency, write
# a Maven/Cargo cache, change DNS/proxy settings, or load any trust material. It
# only runs version commands and read-only filesystem/DNS observations, then
# writes a rerunnable report and the command stdout/stderr under OUT_DIR.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/environment-readiness-doctor}"
STRICT=0
MAVEN_DNS_HOST="${MAVEN_DNS_HOST:-repo.maven.apache.org}"
TIMEOUT_PATH="$(type -P timeout 2>/dev/null || true)"

if [ "${1:-}" = "--strict" ]; then
  STRICT=1
  OUT_DIR="${2:-$REPO_ROOT/.scratch/environment-readiness-doctor}"
elif [ "${2:-}" = "--strict" ]; then
  STRICT=1
fi

usage() {
  cat >&2 <<'USAGE'
usage: scripts/environment-readiness-doctor.sh [--strict] [OUT_DIR]

The default mode is diagnostic-only: it exits zero even when the report is
BLOCKED. --strict exits 3 when any required environment fact is BLOCKED.
MAVEN_DNS_HOST may override the DNS name used for the read-only Maven DNS
observation (default: repo.maven.apache.org).
USAGE
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

# Never follow an output-directory symlink. The output directory is the only
# location this diagnostic is allowed to create or replace files in.
if [ -L "$OUT_DIR" ]; then
  echo "refusing symlink output directory: $OUT_DIR" >&2
  exit 2
fi
mkdir -p -- "$OUT_DIR"
if [ -L "$OUT_DIR" ]; then
  echo "refusing symlink output directory: $OUT_DIR" >&2
  exit 2
fi

CHECK_DIR="$OUT_DIR/checks"
mkdir -p -- "$CHECK_DIR"
CHECKS_TSV="$OUT_DIR/checks.tsv"
COMMAND_LOG="$OUT_DIR/commands.log"
DNS_TSV="$OUT_DIR/dns.tsv"
CACHE_TSV="$OUT_DIR/cache.tsv"
REPORT="$OUT_DIR/environment-readiness.json"
: >"$CHECKS_TSV"
: >"$COMMAND_LOG"
: >"$DNS_TSV"
: >"$CACHE_TSV"

quote_command() {
  printf '%q ' "$@"
}

resolve_external_command() {
  # `type -P` avoids aliases/functions and returns an executable PATH entry.
  type -P -- "$1" 2>/dev/null || true
}

run_bounded() {
  if [ -n "$TIMEOUT_PATH" ]; then
    "$TIMEOUT_PATH" 10s "$@"
  else
    # Version and resolver probes are bounded by their own small operations;
    # keep the doctor usable on hosts without GNU coreutils timeout.
    "$@"
  fi
}

version_major() {
  # Handles `openjdk version "21..."` and `javac 21`, without executing any
  # additional command supplied by the environment.
  sed -n -E \
    -e 's/.*version "([0-9]+).*/\1/p' \
    -e 's/^javac ([0-9]+).*/\1/p' \
    -e 's/^([0-9]+)(\.|$).*/\1/p' | head -n 1
}

run_version_check() {
  local name="$1" command_name="$2" argument="$3"
  local stdout="$CHECK_DIR/$name.stdout.log" stderr="$CHECK_DIR/$name.stderr.log"
  local path rc status major detail
  path="$(resolve_external_command "$command_name")"
  if [ -z "$path" ]; then
    : >"$stdout"
    : >"$stderr"
    printf '%s\n' "tool=$command_name (missing from PATH)" >>"$COMMAND_LOG"
    printf '%s\tBLOCKED\t%s\t%s\t%s\t%s\t%s\n' "$name" "$command_name" "" "127" "$stdout" "$stderr" >>"$CHECKS_TSV"
    return 0
  fi

  printf '$ ' >>"$COMMAND_LOG"
  quote_command "$path" "$argument" >>"$COMMAND_LOG"
  printf '\n' >>"$COMMAND_LOG"
  # Version commands must not fetch dependencies. Cargo is additionally put in
  # offline mode so a rustup/cargo wrapper cannot turn this probe into a fetch.
  set +e
  if [ "$command_name" = "cargo" ]; then
    CARGO_NET_OFFLINE=true run_bounded "$path" "$argument" >"$stdout" 2>"$stderr"
  else
    run_bounded "$path" "$argument" >"$stdout" 2>"$stderr"
  fi
  rc=$?
  set -e
  major="$(cat "$stdout" "$stderr" | version_major)"
  status=READY
  detail=version-command-succeeded
  if [ "$rc" -ne 0 ]; then
    status=BLOCKED
    detail="version-command-exit-$rc"
  elif { [ "$name" = "java" ] || [ "$name" = "javac" ]; } && { [ -z "$major" ] || [ "$major" -lt 21 ]; }; then
    status=BLOCKED
    detail="jdk-21-or-newer-required"
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$status" "$command_name" "$path" "$rc" "$stdout" "$stderr" >>"$CHECKS_TSV"
  printf 'detail[%s]=%s major[%s]=%s\n' "$name" "$detail" "$name" "${major:-unknown}" >>"$COMMAND_LOG"
}

for spec in \
  'java:java:-version' \
  'javac:javac:--version' \
  'cargo:cargo:--version' \
  'mvn:mvn:--version' \
  'protoc:protoc:--version'; do
  IFS=: read -r name command_name argument <<<"$spec"
  run_version_check "$name" "$command_name" "$argument"
done

resolve_maven_cache() {
  if [ -n "${MAVEN_REPO_LOCAL:-}" ]; then
    printf '%s' "$MAVEN_REPO_LOCAL"
    return
  fi
  if [ -f "$HOME/.m2/settings.xml" ] && [ ! -L "$HOME/.m2/settings.xml" ]; then
    # Maven's localRepository is a simple scalar in the supported settings
    # shape. Do not execute Maven or parse arbitrary XML here.
    local configured
    configured="$(sed -n -E 's#.*<localRepository>[[:space:]]*([^<]+)[[:space:]]*</localRepository>.*#\1#p' "$HOME/.m2/settings.xml" | head -n 1)"
    if [ -n "$configured" ]; then
      case "$configured" in
        ~/*) configured="$HOME/${configured#~/}" ;;
        /*) : ;;
        *) configured="$HOME/.m2/$configured" ;;
      esac
      printf '%s' "$configured"
      return
    fi
  fi
  printf '%s' "$HOME/.m2/repository"
}

MAVEN_CACHE="$(resolve_maven_cache)"
cache_status=BLOCKED
cache_detail=cache-directory-missing
artifact_count=0
last_updated_count=0
cache_kind=missing
if [ -L "$MAVEN_CACHE" ]; then
  cache_kind=symlink
  cache_detail=cache-path-is-symlink
elif [ -d "$MAVEN_CACHE" ]; then
  cache_kind=directory
  artifact_count="$(find "$MAVEN_CACHE" -type f \
    ! -name '*.lastUpdated' ! -name '_remote.repositories' \
    ! -name 'resolver-status.properties' -print 2>/dev/null | wc -l | tr -d ' ')"
  last_updated_count="$(find "$MAVEN_CACHE" -type f -name '*.lastUpdated' -print 2>/dev/null | wc -l | tr -d ' ')"
  if [ "$artifact_count" -gt 0 ] && [ "$last_updated_count" -eq 0 ]; then
    cache_status=READY
    cache_detail=cache-contains-artifacts-without-lastUpdated-markers
  elif [ "$last_updated_count" -gt 0 ]; then
    cache_detail=cache-has-incomplete-lastUpdated-markers
  else
    cache_detail=cache-directory-is-empty
  fi
elif [ -e "$MAVEN_CACHE" ]; then
  cache_kind=non-directory
  cache_detail=cache-path-is-not-a-directory
fi
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$cache_status" "$cache_kind" "$MAVEN_CACHE" "$artifact_count" "$last_updated_count" "$cache_detail" "$HOME" >"$CACHE_TSV"
printf 'cache path=%q status=%s artifacts=%s lastUpdated=%s detail=%s\n' \
  "$MAVEN_CACHE" "$cache_status" "$artifact_count" "$last_updated_count" "$cache_detail" >>"$COMMAND_LOG"

# DNS is a read-only resolver query. No proxy, hosts file, resolver config, or
# network setting is modified. Prefer libc getent, then common resolver tools,
# and finally Python's getaddrinfo as a last read-only fallback.
dns_status=BLOCKED
dns_resolver=none
dns_rc=127
dns_stdout="$OUT_DIR/maven-dns.stdout.log"
dns_stderr="$OUT_DIR/maven-dns.stderr.log"
: >"$dns_stdout"
: >"$dns_stderr"
for resolver in getent host nslookup; do
  resolver_path="$(resolve_external_command "$resolver")"
  [ -n "$resolver_path" ] || continue
  dns_resolver="$resolver"
  printf '$ ' >>"$COMMAND_LOG"
  if [ "$resolver" = "getent" ]; then
    quote_command "$resolver_path" ahosts "$MAVEN_DNS_HOST" >>"$COMMAND_LOG"
    printf '\n' >>"$COMMAND_LOG"
    set +e
    run_bounded "$resolver_path" ahosts "$MAVEN_DNS_HOST" >"$dns_stdout" 2>"$dns_stderr"
    dns_rc=$?
    set -e
  else
    quote_command "$resolver_path" "$MAVEN_DNS_HOST" >>"$COMMAND_LOG"
    printf '\n' >>"$COMMAND_LOG"
    set +e
    run_bounded "$resolver_path" "$MAVEN_DNS_HOST" >"$dns_stdout" 2>"$dns_stderr"
    dns_rc=$?
    set -e
  fi
  if [ "$dns_rc" -eq 0 ] && [ -s "$dns_stdout" ]; then
    dns_status=READY
  fi
  break
done
if [ "$dns_resolver" = "none" ]; then
  py_path="$(resolve_external_command python3)"
  if [ -n "$py_path" ]; then
    dns_resolver=python3-getaddrinfo
    printf '$ %q -c %q\n' "$py_path" "import socket; print(*socket.getaddrinfo('$MAVEN_DNS_HOST', 443), sep='\\n')" >>"$COMMAND_LOG"
    set +e
    MAVEN_DNS_HOST="$MAVEN_DNS_HOST" run_bounded "$py_path" -c 'import os, socket; print(*socket.getaddrinfo(os.environ["MAVEN_DNS_HOST"], 443), sep="\\n")' >"$dns_stdout" 2>"$dns_stderr"
    dns_rc=$?
    set -e
    if [ "$dns_rc" -eq 0 ] && [ -s "$dns_stdout" ]; then
      dns_status=READY
    fi
  fi
fi
printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$dns_status" "$dns_resolver" "$MAVEN_DNS_HOST" "$dns_rc" "$dns_stdout" "$dns_stderr" >"$DNS_TSV"

# JSON assembly is kept in Python so paths, diagnostics, and command output are
# escaped correctly. If Python itself is absent, this is a diagnostic blocker;
# no false READY report is emitted.
PYTHON_BIN="$(resolve_external_command python3)"
if [ -z "$PYTHON_BIN" ]; then
  echo 'python3 is required only to render the JSON report; no report was emitted' >&2
  exit 2
fi
"$PYTHON_BIN" - "$CHECKS_TSV" "$DNS_TSV" "$CACHE_TSV" "$COMMAND_LOG" "$REPORT" "$OUT_DIR" "$MAVEN_DNS_HOST" "$REPO_ROOT" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

checks_file, dns_file, cache_file, command_log, report_file, out_dir = map(pathlib.Path, sys.argv[1:7])
dns_host = sys.argv[7]
repo_root = pathlib.Path(sys.argv[8])

def first_line(path):
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.strip():
                return line.strip()
    except OSError:
        pass
    return ""

checks = []
for raw in checks_file.read_text(encoding="utf-8").splitlines():
    name, status, command, path, rc, stdout, stderr = raw.split("\t", 6)
    argument = "-version" if name == "java" else "--version"
    checks.append({
        "id": name,
        "status": status,
        "command": f"{command} {argument}",
        "path": path or None,
        "exitCode": int(rc),
        "version": first_line(pathlib.Path(stdout)) or first_line(pathlib.Path(stderr)) or None,
        "detail": (
            "command-not-found" if not path else
            ("version-command-exit-%s" % rc if int(rc) else
             ("jdk-21-or-newer-required" if name in ("java", "javac") else "version-command-succeeded"))
        ),
        "stdoutLog": str(pathlib.Path(stdout).relative_to(out_dir)),
        "stderrLog": str(pathlib.Path(stderr).relative_to(out_dir)),
    })

dns_status, resolver, host, dns_rc, dns_stdout, dns_stderr = dns_file.read_text(encoding="utf-8").strip().split("\t", 5)
dns = {
    "id": "maven-dns",
    "status": dns_status,
    "host": host,
    "resolver": resolver,
    "exitCode": int(dns_rc),
    "addressesOrOutput": first_line(pathlib.Path(dns_stdout)) or None,
    "stdoutLog": str(pathlib.Path(dns_stdout).relative_to(out_dir)),
    "stderrLog": str(pathlib.Path(dns_stderr).relative_to(out_dir)),
}

cache_status, cache_kind, cache_path, artifacts, stale, cache_detail, home = cache_file.read_text(encoding="utf-8").strip().split("\t", 6)
cache = {
    "id": "maven-cache",
    "status": cache_status,
    "path": cache_path,
    "kind": cache_kind,
    "artifactFileCount": int(artifacts),
    "lastUpdatedFileCount": int(stale),
    "detail": cache_detail,
}

all_checks = checks + [dns, cache]
overall = "READY" if all(item["status"] == "READY" for item in all_checks) else "BLOCKED"
report = {
    "schemaVersion": "environment-readiness-v1",
    "kind": "EnvironmentReadinessReport",
    "generatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
    "repository": str(repo_root),
    "overall": overall,
    "readOnly": True,
    "networkChanges": False,
    "trust": "NOT_EVALUATED",
    "gate": "NOT_RUN",
    "diagnosticOnly": True,
    "mavenDnsHost": dns_host,
    "checks": all_checks,
    "rerun": {
        "commandLog": str(pathlib.Path(command_log).relative_to(out_dir)),
        "stdoutStderrLogs": "checks/ and maven-dns.*.log",
        "report": str(pathlib.Path(report_file).relative_to(out_dir)),
    },
}
report_file.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2, sort_keys=True))
PY

printf 'environment-readiness=%s report=%s command_log=%s\n' \
  "$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1]))["overall"])' "$REPORT")" \
  "$REPORT" "$COMMAND_LOG" >&2

if [ "$STRICT" -eq 1 ] && grep -q '"overall": "BLOCKED"' "$REPORT"; then
  exit 3
fi
exit 0
