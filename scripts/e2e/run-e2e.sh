#!/usr/bin/env bash
# Entry point for the ERF end-to-end suite. Loads the repo runtime env then runs
# the Python runner. See docs/e2e/README.md.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi
exec python3 scripts/e2e/e2e_runner.py "$@"
