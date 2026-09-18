#!/usr/bin/env bash
# Run the registered repositories in read-only LIVE_SHADOW mode.
# Usage: scripts/pilot-live-observation.sh OUT_DIR [--offline SOURCE_ROOT]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-live-observation.sh OUT_DIR [--offline SOURCE_ROOT]}"
shift
[ ! -e "$out_dir" ] || { echo "output directory already exists: $out_dir" >&2; exit 2; }
offline=false
source_root=""
if [ "${1:-}" = "--offline" ]; then
  offline=true
  source_root="${2:?--offline requires SOURCE_ROOT}"
  shift 2
fi
[ "$#" -eq 0 ] || { echo "unexpected argument: $1" >&2; exit 2; }
mkdir -p "$out_dir/repositories" "$out_dir/baselines" "$out_dir/runs"
cd "$repo_root"

java_host="${ERF_JAVA_HOST:-$repo_root/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar}"
resolver="$repo_root/target/debug/erf-static-resolver"
if [ ! -s "$java_host" ]; then
  mvn -q -B -ntp -f backend-hosts/java/pom.xml package -DskipTests
fi
[ -s "$java_host" ] || { echo "missing Java Host: $java_host" >&2; exit 1; }
if [ ! -x "$resolver" ]; then
  cargo build -q -p erf-static-resolver
fi
compiled="$(scripts/candidate-compile.sh specs/rule-intent/corp.java.avoid-duplicate-literals.yaml "$out_dir/candidate-build")"
candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate" ] || { echo "candidate path missing" >&2; exit 1; }

manifest="$repo_root/specs/pilot/live-java-repositories.yaml"
mapfile -t records < <(awk '
  /^  - id:/ { if (repo != "") print repo "\t" url "\t" branch "\t" commit; repo=url=branch=commit="" }
  /^    repository:/ { repo=$2 }
  /^    url:/ { url=$2 }
  /^    defaultBranch:/ { branch=$2 }
  /^    commit:/ { commit=$2 }
  END { if (repo != "") print repo "\t" url "\t" branch "\t" commit }
' "$manifest")
[ "${#records[@]}" -eq 3 ] || { echo "expected three registered repositories" >&2; exit 1; }

window_start="$(date -u +%s)"
for record in "${records[@]}"; do
  IFS=$'\t' read -r repository url branch registered_commit <<< "$record"
  key="${repository//\//__}"
  destination="$out_dir/repositories/$key"
  if "$offline"; then
    source="$source_root/${repository##*/}"
    [ -d "$source/.git" ] || { echo "missing offline source clone: $source" >&2; exit 1; }
    git clone --local --no-hardlinks "$source" "$destination" >/dev/null
  else
    git clone --depth=1 --branch "$branch" "$url" "$destination" >/dev/null
  fi
  commit="$(git -C "$destination" rev-parse HEAD)"
  baseline="$out_dir/baselines/$key.baseline"
  ./rf pilot baseline \
    --repository "$repository" --commit "$commit" \
    --repo-root "$destination" --candidate "$candidate" \
    --host java --host-arg -jar --host-arg "$java_host" \
    --resolver "$resolver" --output "$baseline" \
    > "$out_dir/runs/$key-baseline.console" 2>&1
  set +e
  ./rf pilot observe \
    --repository "$repository" --commit "$commit" \
    --repo-root "$destination" --candidate "$candidate" \
    --host java --host-arg -jar --host-arg "$java_host" \
    --resolver "$resolver" --baseline "$baseline" \
    --journal "$out_dir/metrics.journal" --mode live-shadow \
    --measurement-id "LIVE_SHADOW:$repository:$commit" \
    --observed-at "$(date -u +%s)" --reason "registered read-only Shadow observation" \
    > "$out_dir/runs/$key-observe.console" 2>&1
  status=$?
  set -e
  if [ "$status" -ne 0 ] && [ "$status" -ne 3 ]; then
    cat "$out_dir/runs/$key-observe.console" >&2
    exit "$status"
  fi
  printf '%s\t%s\t%s\t%s\n' "$repository" "$registered_commit" "$commit" "$key" >> "$out_dir/observed-commits.tsv"
done

window_end="$(date -u +%s)"
set +e
./rf pilot window-report \
  --journal "$out_dir/metrics.journal" \
  --output "$out_dir/live-shadow-report.md" \
  --window-start "$window_start" --window-end "$((window_end + 1))" \
  --expected-repositories 3 \
  --mode live-shadow \
  > "$out_dir/runs/window-report.console" 2>&1
report_status=$?
set -e
[ "$report_status" -eq 3 ] || { cat "$out_dir/runs/window-report.console" >&2; exit 1; }
rg -q '^window-report=INCONCLUSIVE ' "$out_dir/runs/window-report.console"

cat > "$out_dir/live-shadow.md" <<EOF
# Live Shadow Observation Run

- Mode: LIVE_SHADOW
- Source: registered public repositories, read-only acquisition
- Registered sample: $manifest
- Journal: $out_dir/metrics.journal
- Report: $out_dir/live-shadow-report.md
- Registered commit identities are recorded in observed-commits.tsv.

This is a pre-window rehearsal when run before 2026-10-01. The report remains
INCONCLUSIVE until the registered window supplies operational baselines and
human quality adjudication.
EOF
echo "live-shadow=PASS evidence=$out_dir/live-shadow.md"
