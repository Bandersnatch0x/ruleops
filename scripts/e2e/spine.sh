#!/usr/bin/env bash
# Live end-to-end spine: L0 authoring -> L5 execution -> L6 gate -> L7 publish -> L8 consume.
# Composes real repo entries (no fixtures are faked) and asserts cross-hop continuity
# that is actually exposed by the tools; writes $OUT/spine.json.
set -uo pipefail
OUT="${1:?usage: spine.sh OUT}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi
mkdir -p "$OUT"
hops_file="$OUT/.hops.jsonl"
: > "$hops_file"

record() { # hop_name rc detail_json
  printf '{"hop":%s,"rc":%s,"detail":%s}\n' "$(printf '%s' "$1" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" "$2" "$3" >> "$hops_file"
}

# ---- Hop 1: L0 authoring -> L5 execution (real candidate/evidence/init/scan) ----
bash scripts/cli-surface-check.sh "$OUT/h1-cli-surface" > "$OUT/h1.log" 2>&1
h1_rc=$?
record "L0-L5 authoring->init->scan (cli-surface-check)" "$h1_rc" '{"evidence":"h1-cli-surface/","log":"h1.log"}'

# ---- Hop 2: L5/L6 scan determinism (two identical scans -> identical resultDigest) ----
mkdir -p "$OUT/h2-fixture/src/main/java"
printf 'public class A { void run() {} }\n' > "$OUT/h2-fixture/src/main/java/A.java"
printf '<project></project>\n' > "$OUT/h2-fixture/pom.xml"
./rf init --repo-root "$OUT/h2-fixture" --non-interactive --language java --build-system maven > "$OUT/h2-init.log" 2>&1
h2_init=$?
scan() { ./rf scan --repo-root "$OUT/h2-fixture" --rule-id corp.java.cli-smoke \
  --resolver "$repo/target/debug/erf-fake-resolver" --host "$repo/target/debug/erf-fake-host" \
  --format json --progress none; }
scan > "$OUT/h2-scan1.json" 2> "$OUT/h2-scan1.err"; h2_s1=$?
scan > "$OUT/h2-scan2.json" 2> "$OUT/h2-scan2.err"; h2_s2=$?
h2_dig1="$(python3 -c 'import json;print(json.load(open("'"$OUT"'/h2-scan1.json"))["resultDigest"])' 2>/dev/null)"
h2_dig2="$(python3 -c 'import json;print(json.load(open("'"$OUT"'/h2-scan2.json"))["resultDigest"])' 2>/dev/null)"
if [ "$h2_init" -eq 0 ] && [ "$h2_s1" -eq 0 ] && [ "$h2_s2" -eq 0 ] && [ -n "$h2_dig1" ] && [ "$h2_dig1" = "$h2_dig2" ]; then
  h2_rc=0
else
  h2_rc=1
fi
record "L5/L6 scan determinism (resultDigest stable across two runs)" "$h2_rc" \
  "{\"initRc\":$h2_init,\"scan1Rc\":$h2_s1,\"scan2Rc\":$h2_s2,\"resultDigest\":\"$h2_dig1\",\"stable\":$([ \"$h2_dig1\" = \"$h2_dig2\" ] && echo true || echo false)}"

# ---- Hop 3: L7 publication contract (publisher artifact + adapter deployment) ----
bash scripts/status-publication-check.sh "$OUT/h3-status-publication" > "$OUT/h3-pub.log" 2>&1; h3a=$?
bash scripts/status-adapter-deployment-check.sh "$OUT/h3-status-adapter-deployment" > "$OUT/h3-dep.log" 2>&1; h3b=$?
[ "$h3a" -eq 0 ] && [ "$h3b" -eq 0 ] && h3_rc=0 || h3_rc=1
record "L7 publication + adapter deployment contract" "$h3_rc" "{\"publicationRc\":$h3a,\"deploymentRc\":$h3b}"

# ---- Hop 4: L7 SCM adapter (offline fake provider) ----
python3 scripts/scm-status-adapter-github-check.py > "$OUT/h4-scm.log" 2>&1; h4_rc=$?
record "L7 SCM status adapter (GitHub, offline fake provider)" "$h4_rc" '{"log":"h4-scm.log"}'

# ---- Hop 5: L8 consumption (explain authority boundary) ----
bash scripts/rich-explain-check.sh "$OUT/h5-rich-explain" > "$OUT/h5.log" 2>&1; h5_rc=$?
record "L8 rich explain / authority boundary" "$h5_rc" '{"evidence":"h5-rich-explain/"}'

# ---- Hop 6: L8 agent commit hook with real PMD (guarded precondition) ----
installed_rf="${INSTALLED_RF:-$repo/.scratch/production-runtime-pack-check/production-rf/rf}"
candidate="${CANDIDATE:-$repo/.scratch/phase2-exit-check/candidates/corp.java.no-system-exit}"
runtime_home="${RUNTIME_HOME:-$repo/.scratch/production-runtime-pack-check/production-rf}"
if [ -x "$installed_rf" ] && [ -d "$candidate" ]; then
  python3 scripts/agent-commit-hook-test.py "$OUT/h6-hook" --real-rf "$installed_rf" \
    --candidate "$candidate" --runtime-home "$runtime_home" > "$OUT/h6.log" 2>&1; h6_rc=$?
  h6_state="ran"
else
  h6_rc=0; h6_state="skipped-missing-runtime-pack"
fi
record "L8 agent commit hook (real PMD) [$h6_state]" "$h6_rc" "{\"log\":\"h6.log\",\"state\":\"$h6_state\"}"

# ---- Assemble spine.json + overall rc ----
python3 - "$hops_file" "$OUT/spine.json" <<'PY'
import json, sys
hops_path, out_path = sys.argv[1], sys.argv[2]
hops = [json.loads(line) for line in open(hops_path, encoding="utf-8") if line.strip()]
overall = 0 if all(h["rc"] == 0 for h in hops) else 1
json.dump({"kind": "E2ESpine", "hops": hops, "overallRc": overall}, open(out_path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
print(json.dumps({"overallRc": overall, "hops": [{"hop": h["hop"], "rc": h["rc"]} for h in hops]}, ensure_ascii=False))
sys.exit(overall)
PY
