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
# Build the compiler binary first: the host-side execution court tests and the
# verifier both need `target/debug/phorc` to be present.
cargo build -q -p phorc -p phost
cargo test

echo
echo "=== Persistent Sealed Port Store ==="
cargo build -q -p phost -p phorc
# Validate the committed store against committed evidence FIRST, before any
# regenerating pass touches the evidence dirs.
./verify_store.sh
./verify_session.sh

echo
echo "=== JIT-Porting Court ==="
cargo build -q -p phost -p phorc
# Validate the committed evidence against a fresh run FIRST (writes only to temp),
# so the check is not vacuous, then prove generation is deterministic.
./verify_jit_porting_court.sh --target toupper --check-committed
./verify_jit_porting_court.sh --target memcmp --check-committed
./verify_jit_porting_court.sh --target memchr --check-committed
./verify_jit_porting_court.sh --target strlen --check-committed
./verify_jit_porting_court.sh --target strrchr --check-committed
./verify_jit_porting_court.sh --target strspn --check-committed
./verify_composition_court.sh --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr_pair --check-committed
./verify_composition_court.sh --target toupper_each --check-committed
./verify_composition_court.sh --target toupper_each_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_memchr_suffix --check-committed
./verify_jit_porting_court.sh --target toupper
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memchr
./verify_jit_porting_court.sh --target strlen
./verify_jit_porting_court.sh --target strrchr
./verify_jit_porting_court.sh --target strspn
./verify_composition_court.sh
./verify_composition_court.sh --target toupper_strlen_memchr
./verify_composition_court.sh --target toupper_strlen_memchr_pair
./verify_composition_court.sh --target toupper_each
./verify_composition_court.sh --target toupper_each_strlen_memchr
./verify_composition_court.sh --target toupper_memchr_suffix
