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
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

HIST_ID="posix:strspn:c-locale:u64:v1"
SUCC_ID="libc:strspn:c-locale:u64:v1"
SUCC_SLUG="libc-strspn-c-locale-u64-v1"

# The golden content identities. The historical id must never drift (existing
# seals bind it); the successor id is new and distinct.
HIST_SPEC_ID="bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23"
SUCC_SPEC_ID="a4ee309a8959b40a1f8d32bf3944156a70fbc15b0a4a8e85a10de5344bd1e697"

# The successor's sealed artifact, its evidence closure and its promotion receipt,
# and the immutable generation it is published into (Phase 8 §19).
SUCC_ARTIFACT="c40c4e3a5100257b04f7df0b593e961bb0acc4ad2658e1604fda068fc7dd54e2"
SUCC_CLOSURE="b7ec423a12e27a409fc47944ace471ada1fae9b44d0dd18d89aedef16fd2fc0b"
SUCC_EVIDENCE="3b93f1d76df3fbdc2ee8236b0448486576535cc3506a3c2175f4d9aa93a8a6a0"
SUCC_GENERATION_ID="8e6fafec7b2b66ced51d8699d86fc264983fe2cd501c11b9c316f4d125912ba5"
HIST_ARTIFACT="c93271d069fd998e6de8c2cd07e665bd098f6fb64072cdd98fb44ae1375dbafc"

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

echo "--- 7. the successor is published as an immutable store generation ---"
GEN="$SUCC_EVID/store_generation.json"
[ -f "$GEN" ] || fail "no committed store generation at $GEN"
GEN_TMP="$TMP/generation.json"
"$PHORPORT" store generations \
    --publish-target "$SUCC_ID" \
    --publish-artifact "$SUCC_ARTIFACT" \
    --closure "$SUCC_CLOSURE" \
    --publish-evidence "$SUCC_EVIDENCE" \
    --out "$GEN_TMP" >/dev/null 2>&1 \
    || fail "the store generation did not regenerate"
diff -q "$GEN" "$GEN_TMP" >/dev/null \
    || fail "the committed store generation did not reproduce"
python3 - "$GEN" "$SUCC_ID" "$SUCC_ARTIFACT" "$SUCC_GENERATION_ID" "$HIST_ARTIFACT" <<'PY' || exit 1
import json, sys
g = json.load(open(sys.argv[1]))
succ_id, succ_art, succ_gen, hist_art = sys.argv[2:6]
assert g["schema"] == "phorensic.porting.store_generation.v1", g["schema"]
assert g["generation_id"] == succ_gen, (g["generation_id"], succ_gen)
assert g["parent"], "the published generation must have a parent"
assert len(g["entries"]) == 14, len(g["entries"])
e = {x["target"]: x for x in g["entries"]}
# The successor is admitted under its own autonomous profile and artifact.
assert succ_id in e, "the successor is not published in the generation"
assert e[succ_id]["seal_profile"] == "AutonomousV1", e[succ_id]
assert e[succ_id]["artifact_hash"] == succ_art, e[succ_id]
# The historical leaf is carried forward unchanged: the v1 baseline is not mutated.
hist = e["posix:strspn:c-locale:u64:v1"]
assert hist["seal_profile"] == "LegacyV1", hist
assert hist["artifact_hash"] == hist_art, hist
print(f"    generation {g['generation_id'][:16]}...: 14 entries, parent bound, successor AutonomousV1")
PY
pass "successor published as an immutable child generation; the v1 index is untouched"

echo
echo "ALL CHECKS PASSED"
exit 0
