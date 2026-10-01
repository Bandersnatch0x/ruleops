#!/usr/bin/env bash
# External GitHub adapter; the frozen runtime stays credential/network-free.
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "$#" -ne 2 ]; then
  echo "usage: $0 StatusPublication.json StatusAdapterDeployment.json" >&2
  exit 2
fi
if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  . "$HOME/.local/opt/erf-env.sh"
fi
BUILD="$ROOT/.scratch/scm-status-adapter-build"
mkdir -p "$BUILD"
# One build/ledger writer per checkout. Separate deployments must not share a context.
exec 9>"$BUILD/build.lock"
flock -x 9
cat > "$BUILD/Cargo.toml" <<EOF
[package]
name = "erf-github-contract-check"
version = "0.1.0"
edition = "2021"
[workspace]
[dependencies]
erf-status = { path = "$ROOT/runtime/status" }
serde_json = "1"
[[bin]]
name = "erf-github-contract-check"
path = "$ROOT/scripts/scm-status-adapter-github-contract.rs"
EOF
CARGO_TARGET_DIR="$ROOT/target" env -u ERF_STATUS_TOKEN cargo build --offline --quiet --manifest-path "$BUILD/Cargo.toml"
flock -u 9
exec 9>&-
exec python3 "$ROOT/scripts/scm-status-adapter-github.py" "$1" "$2" "$ROOT/target/debug/erf-github-contract-check"
