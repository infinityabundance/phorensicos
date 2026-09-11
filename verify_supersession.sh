#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Contract-Provenance Supersession Verifier
#
#  A correction of a PortSpec's contract provenance must be a *new identity*,
#  never a silent rename. This verifies the one declared correction
#  (`posix:strspn` -> `libc:strspn`), which repairs a mistaken provenance claim
#  in the repository's own history:
#
#    * the correction record is declared, consistent, and machine-readable;
#    * the historical target and its PortSpecId are PRESERVED (bare-symbol
#      resolution and the committed historical evidence still address it);
#    * the historical evidence still verifies and was not rewritten;
#    * the successor has its own distinct PortSpecId and its own autonomous
#      seal, and its evidence verifies too;
#    * the two evidence closures differ (no identity collapse).
#
#  See docs/CONTRACT_PROVENANCE_MIGRATION.md.
#  Usage: ./verify_supersession.sh
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

PHORPORT="$ROOT/target/debug/phorport"

HIST_ID="posix:strspn:c-locale:u64:v1"
SUCC_ID="libc:strspn:c-locale:u64:v1"
SUCC_SLUG="libc-strspn-c-locale-u64-v1"

# The golden content identities. The historical id must never drift (existing
# seals bind it); the successor id is new and distinct.
HIST_SPEC_ID="bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23"
SUCC_SPEC_ID="a4ee309a8959b40a1f8d32bf3944156a70fbc15b0a4a8e85a10de5344bd1e697"

HIST_EVID="$ROOT/phost/evidence/phorport/autonomy/strspn"
SUCC_EVID="$ROOT/phost/evidence/phorport/autonomy/$SUCC_SLUG"

fail() { echo "  [FAIL] $1"; exit 1; }
pass() { echo "  [PASS] $1"; }

if [ ! -x "$PHORPORT" ]; then
    echo "--- building phorport + phorc ---"
    cargo build -q -p phorport -p phorc || { echo "ERROR: cargo build failed"; exit 2; }
fi

echo "=== Phorensic OS — Contract-Provenance Supersession Verification ==="
echo

echo "--- 1. the correction record is declared and consistent ---"
JSON="$("$PHORPORT" supersessions)" || fail "supersessions did not run"
echo "$JSON" | grep -q "\"historical_target\": \"$HIST_ID\"" \
    || fail "the historical id is missing from the record"
echo "$JSON" | grep -q "\"successor_target\": \"$SUCC_ID\"" \
    || fail "the successor id is missing from the record"
echo "$JSON" | grep -q "\"historical_spec_id\": \"$HIST_SPEC_ID\"" \
    || fail "the historical PortSpecId is not the preserved one"
echo "$JSON" | grep -q "\"successor_spec_id\": \"$SUCC_SPEC_ID\"" \
    || fail "the successor PortSpecId is not the expected distinct one"
echo "$JSON" | grep -qE '"supersession_id": "[0-9a-f]{64}"' \
    || fail "the record has no content identity"
pass "declared, content-addressed, and both PortSpecIds bound"

echo "--- 2. the historical target is preserved, not renamed ---"
[ -d "$HIST_EVID" ] || fail "the historical evidence directory was removed"
grep -q "phor.portspec:$HIST_SPEC_ID" "$HIST_EVID/autonomous_promotion_receipt.json" \
    || fail "the historical seal no longer binds the historical PortSpecId"
pass "historical evidence preserved at $HIST_EVID"

echo "--- 3. the historical seal still verifies ---"
"$ROOT/verify_autonomous_seal.sh" >/dev/null 2>&1 \
    || fail "the historical autonomous seal no longer verifies (evidence rewritten?)"
pass "historical evidence still reproduces"

echo "--- 4. the successor carries its own distinct seal ---"
[ -d "$SUCC_EVID" ] || fail "no successor evidence at $SUCC_EVID"
grep -q "phor.portspec:$SUCC_SPEC_ID" "$SUCC_EVID/autonomous_promotion_receipt.json" \
    || fail "the successor seal does not bind the successor PortSpecId"
grep -q '"seal_profile": "AutonomousV1"' "$SUCC_EVID/autonomous_promotion_receipt.json" \
    || fail "the successor is not sealed under AutonomousV1"
pass "successor sealed under its own PortSpecId"

echo "--- 5. the successor seal verifies ---"
"$ROOT/verify_autonomous_seal.sh" --target strspn --target-id "$SUCC_ID" \
    --evidence-slug "$SUCC_SLUG" >/dev/null 2>&1 \
    || fail "the successor autonomous seal does not verify"
pass "successor evidence reproduces"

echo "--- 6. the two identities do not collapse ---"
HIST_CLOSURE="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['evidence_closure'])" "$HIST_EVID/autonomous_promotion_receipt.json")"
SUCC_CLOSURE="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['evidence_closure'])" "$SUCC_EVID/autonomous_promotion_receipt.json")"
[ -n "$HIST_CLOSURE" ] || fail "the historical closure is empty"
[ -n "$SUCC_CLOSURE" ] || fail "the successor closure is empty"
[ "$HIST_CLOSURE" != "$SUCC_CLOSURE" ] || fail "the successor reuses the historical closure"
pass "distinct closures: historical $HIST_CLOSURE, successor $SUCC_CLOSURE"

echo
echo "ALL CHECKS PASSED"
exit 0
