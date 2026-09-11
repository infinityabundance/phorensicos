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
# The implementation axis: the same sealed corpora through a second implementation.
./verify_cross_implementation.sh

# --- Foundry Phase 0: the executable baseline receipt --------------------------
# Derive the four-repository baseline from the executable courts. Writes to a
# temp path here so the committed receipt (the baseline record) is not touched.
echo
echo "=== Foundry baseline (Phase 0) ==="
./scripts/integration_baseline.sh /tmp/integration_baseline_receipt.json

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
./verify_composition_court.sh --target toupper_each_slice_search --check-committed
# Phase 2: the generic (IR) composition court — composition as data.
./verify_composition_ir_court.sh --check-committed
./verify_composition_ir_court.sh --target toupper_strlen_memchr --check-committed
./verify_composition_ir_court.sh --target toupper_strlen_memchr_pair --check-committed
./verify_composition_ir_court.sh --target toupper_each --check-committed
./verify_composition_ir_court.sh --target toupper_each_strlen_memchr --check-committed
./verify_composition_ir_court.sh --target toupper_memchr_suffix --check-committed
./verify_composition_ir_court.sh --target toupper_each_slice_search --check-committed

# --- Foundry Phase 4: the differential counterexample engine -------------------
echo
echo "=== Autonomous Porting Foundry (Phase 4) ==="
./verify_phorport.sh
# Phase 7: held-out qualification, the multi-oracle policy, the FRF outer court,
# candidate containment and the AUTONOMOUS-SEAL/v1 promotion profile.
echo
echo "=== Autonomous Porting Foundry (Phase 7) ==="
./verify_autonomous_seal.sh
# Phase 3: the court-sensitivity (challenge) court — is the instrument blind?
for ch in toupper memcmp memchr strlen strrchr strspn toupper_memchr toupper_strlen_memchr toupper_strlen_memchr_pair toupper_each toupper_each_strlen_memchr toupper_memchr_suffix toupper_each_slice_search; do
    ./verify_challenge_court.sh --target "$ch" --check-committed
    ./verify_challenge_court.sh --target "$ch"
done
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
./verify_composition_court.sh --target toupper_each_slice_search
./verify_composition_ir_court.sh
./verify_composition_ir_court.sh --target toupper_strlen_memchr
./verify_composition_ir_court.sh --target toupper_strlen_memchr_pair
./verify_composition_ir_court.sh --target toupper_each
./verify_composition_ir_court.sh --target toupper_each_strlen_memchr
./verify_composition_ir_court.sh --target toupper_memchr_suffix
./verify_composition_ir_court.sh --target toupper_each_slice_search
