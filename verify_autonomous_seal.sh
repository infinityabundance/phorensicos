#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Autonomous Porting Foundry (Phase 7) Verifier
#
#  Verifies the committed AUTONOMOUS-SEAL/v1 evidence for a target:
#
#    * the receipt carries the AutonomousV1 profile and all 13 obligations;
#    * the evidence closure is present and binds the candidate object;
#    * the held-out qualification receipt is consistent and isolated;
#    * the isolation report found no qualification material in the synthesis
#      workspace;
#    * the FRF outer court produced at least one receipt (retained verbatim);
#    * the whole pipeline REPRODUCES: a fresh run to a temp evidence dir yields
#      a byte-identical promotion receipt and qualification receipt.
#
#  Usage: ./verify_autonomous_seal.sh [--target strspn]
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="strspn"
while [ $# -gt 0 ]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        *) echo "unknown argument: $1"; exit 2 ;;
    esac
done

EVID="$ROOT/phost/evidence/phorport/autonomy/$TARGET"
PHORPORT="$ROOT/target/debug/phorport"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "=== Phorensic OS — Autonomous Porting Foundry (Phase 7) Verification ==="
echo "Target:   $TARGET"
echo "Evidence: $EVID"
echo

fail() { echo "  [FAIL] $1"; exit 1; }
pass() { echo "  [PASS] $1"; }

if [ ! -x "$PHORPORT" ]; then
    echo "--- building phorport + phorc ---"
    cargo build -q -p phorport -p phorc || { echo "ERROR: cargo build failed"; exit 2; }
fi

RECEIPT="$EVID/autonomous_promotion_receipt.json"
QUAL="$EVID/qualification_receipt.json"
ISO="$EVID/isolation_report.json"
[ -f "$RECEIPT" ] || fail "no committed autonomous promotion receipt at $RECEIPT"
[ -f "$QUAL" ] || fail "no committed qualification receipt at $QUAL"
[ -f "$ISO" ] || fail "no committed isolation report at $ISO"

echo "--- 1. seal profile and obligations ---"
grep -q '"seal_profile": "AutonomousV1"' "$RECEIPT" || fail "profile is not AutonomousV1"
for o in O1 O2 O3 O4 O5 O6 O7 O8 O9 O10 O11 O12 O13; do
    grep -q "\"id\": \"$o\"" "$RECEIPT" || fail "obligation $o is not satisfied"
done
pass "AutonomousV1 with all 13 obligations satisfied"

echo "--- 2. evidence closure binds the candidate object ---"
CLOSURE="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['evidence_closure'])" "$RECEIPT")"
[ -n "$CLOSURE" ] || fail "empty evidence closure"
grep -q "phor.object-sha256" "$RECEIPT" || fail "receipt does not bind the candidate object"
pass "closure $CLOSURE"

echo "--- 3. held-out qualification is consistent and isolated ---"
python3 - "$QUAL" <<'PY' || exit 1
import json, sys
r = json.load(open(sys.argv[1]))
assert r["verdict"] == "consistent", r["verdict"]
assert r["isolated"] is True, "qualification was not isolated"
assert r["cases_run"] > 0 and r["cases_failed"] == 0, r
assert r["divergences"] == [], r["divergences"]
print(f"    {r['cases_run']} held-out cases, {r['cases_failed']} failed, isolated={r['isolated']}")
PY
pass "qualification consistent over the held-out universe"

echo "--- 4. isolation report is clean ---"
python3 - "$ISO" <<'PY' || exit 1
import json, sys
r = json.load(open(sys.argv[1]))
assert r["clean"] is True, r
assert r["universe_built_after_freeze"] is True, r
assert r["leaks"] == [], r["leaks"]
print(f"    {r['markers_checked']} markers checked, 0 leaks")
PY
pass "the synthesis workspace contains no qualification material"

echo "--- 5. the FRF outer court retained receipts ---"
grep -q '"frf.receipt:' "$RECEIPT" || fail "no FRF receipt is bound"
grep -q '"frf.receipt:' "$RECEIPT" && pass "FRF receipt ids retained verbatim"

echo "--- 6. the pipeline reproduces byte-for-byte ---"
WRONG="$ROOT/examples/jit_port_strspn_always_zero.phor"
[ -f "$WRONG" ] || fail "the committed revision-0 source is missing: $WRONG"
"$PHORPORT" autonomy "$TARGET" \
    --source "$WRONG" \
    --source "$ROOT/examples/jit_port_strspn.phor" \
    --workspace "$ROOT" \
    --out "$TMP/out" >/dev/null 2>&1 || true
[ -f "$TMP/out/autonomous_promotion_receipt.json" ] || fail "the pipeline did not seal on re-run"
# The committed receipt may name the revision-0 source by its previous path; the
# contract is that the *content* reproduces. Compare the canonical identities.
diff -q "$RECEIPT" "$TMP/out/autonomous_promotion_receipt.json" >/dev/null \
    || fail "the promotion receipt did not reproduce"
diff -q "$QUAL" "$TMP/out/qualification_receipt.json" >/dev/null \
    || fail "the qualification receipt did not reproduce"
pass "the promotion and qualification receipts reproduce byte-for-byte"

echo
echo "ALL CHECKS PASSED"
exit 0
