#!/usr/bin/env bash
# Host CI — compiler + runtime tests, then the JIT-Porting Court verifier.
#
# Runs inside the `host` Docker service (docker/Dockerfile.host) but also works
# directly on a Linux host with a Rust toolchain, python3 and jq.
set -euo pipefail

# Make cargo available whether this runs in the rust image (non-login shell) or
# on a developer host.
export PATH="$HOME/.cargo/bin:/usr/local/cargo/bin:$PATH"

cd "$(dirname "$0")/.."

echo "=== cargo test (phorc + phost) ==="
cargo test

echo
echo "=== JIT-Porting Court ==="
cargo build -q -p phost
./verify_jit_porting_court.sh
