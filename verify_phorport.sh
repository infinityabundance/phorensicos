#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Autonomous Porting Foundry (Phase 4) Verifier
#
#  Replays the committed differential counterexample end-to-end, without the
#  fuzzing toolchain:
#
#    * compiles the deliberately defective candidate with `phorc`;
#    * proves the DESIGN corpus alone misses the defect (0 divergences);
#    * replays the committed MINIMAL counterexample on the ordinary
#      uninstrumented object and requires a divergence with the same residual
#      lineage;
#    * checks the counterexample record's content identity.
#
#  The discovery itself is an FRF-Fuzz campaign (see docs/AUTONOMOUS_PORTING.md);
#  this verifier replays its durable result, so CI does not need the nightly
#  instrumented toolchain.
#
#  Usage: ./verify_phorport.sh
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="strspn"
SRC="examples/jit_port_strspn_xor_lane.phor"
EVID="$ROOT/phost/evidence/phorport/$TARGET"
PHORPORT="$ROOT/target/debug/phorport"

echo "=== Phorensic OS — Autonomous Porting Foundry (Phase 4) Verification ==="
echo "Target:       $TARGET"
echo "Candidate:    $SRC"
echo

if [ ! -x "$PHORPORT" ]; then
    echo "--- building phorport ---"
    if ! cargo build -q -p phorport -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

fail() { echo "  [FAIL] $1"; exit 1; }

echo "--- 1. the design corpus alone misses the defect ---"
OUT="$("$PHORPORT" corpus "$TARGET" --candidate-src "$SRC" --workspace "$ROOT" 2>&1)"
echo "$OUT" | sed 's/^/    /'
echo "$OUT" | grep -q "design corpus MISSES" \
    || fail "the design corpus catches the defect (it must miss it for this demonstration)"

echo "--- 2. a committed counterexample exists ---"
CX="$(ls "$EVID"/*.json 2>/dev/null | head -1)"
[ -n "$CX" ] || fail "no committed counterexample under $EVID"
echo "    $CX"

MINIMAL="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['minimal_hex'])" "$CX")"
RESIDUAL="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['minimal_residual'])" "$CX")"
CONTENT="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['content_id'])" "$CX")"
echo "    minimal input:  $MINIMAL"
echo "    residual:       $RESIDUAL"

echo "--- 3. the minimal case replays on the uninstrumented object ---"
PROBE="$("$PHORPORT" probe "$TARGET" --candidate-src "$SRC" --workspace "$ROOT" --data "$MINIMAL" 2>&1 || true)"
echo "$PROBE" | sed 's/^/    /'
echo "$PROBE" | grep -q "matched:     false" || fail "the minimal counterexample no longer diverges"
echo "$PROBE" | grep -q "residual:    $RESIDUAL" || fail "the residual lineage was not preserved"

echo "--- 4. the record's content identity is canonical ---"
python3 - "$CX" <<'PY' || exit 1
import hashlib, json, sys
d = json.load(open(sys.argv[1]))
canon = ("target={};candidate_hash={};original_hex={};original_residual={};minimal_hex={};"
         "minimal_residual={};oracle_hex={};candidate_output_hex={};accepted={};refused={}").format(
    d["target"], d["candidate_hash"], d["original_hex"], d["original_residual"],
    d["minimal_hex"], d["minimal_residual"], d["oracle_hex"], d["candidate_output_hex"],
    d["accepted_reductions"], d["refused_reductions"])
h = hashlib.sha256(b"PHOR/PHORPORT/COUNTEREXAMPLE/v1\0" + canon.encode()).hexdigest()
if h != d["content_id"]:
    print("  [FAIL] content_id does not match the canonical record")
    sys.exit(1)
print("    content_id:  %s (canonical)" % h)
PY

echo "Status: ALL CHECKS PASSED"
exit 0
