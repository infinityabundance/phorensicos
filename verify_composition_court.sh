#!/bin/bash
# ============================================================================
#  Phorensic OS — Sealed Composition Dispatch Court Verifier
#
#  Two modes, one entry point (mirroring verify_jit_porting_court.sh):
#
#    (default)          determinism court  — regenerate the composition evidence
#                       twice and require the two fresh runs to be byte-identical.
#                       This overwrites the evidence dir with a fresh run.
#
#    --check-committed  committed-evidence court — write ONLY to a temp dir and
#                       compare it against the checked-in verdict, without
#                       touching the committed file.
#
#  The composition `phor:compose:toupper_memchr:c-locale:index:v1` is built from
#  two already-sealed leaf ports and executed entirely through the sealed
#  dispatcher. This verifier checks:
#    * the composition verdict exists and is valid JSON
#    * every stage (toupper-haystack, toupper-needle, memchr) was served by the
#      SEALED object for every case: zero foreign fallback and zero broken seals
#    * every case matched the foreign oracle (toupper + memchr)
#    * the objects the chain dispatched to are exactly the committed sealed leaf
#      objects (object hash cross-check against the leaf candidate signatures)
#    * the chain hash is present (it covers the intermediates, not just the index)
#
#  Usage:
#    ./verify_composition_court.sh [--check-committed] [evidence_dir]
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET_ID="phor:compose:toupper_memchr:c-locale:index:v1"
EXPECTED_COUNT=560

MODE="regenerate"
EVID=""
while [ $# -gt 0 ]; do
    case "$1" in
        --check-committed) MODE="check-committed" ;;
        *) EVID="$1" ;;
    esac
    shift
done

[ -n "$EVID" ] || EVID="phost/evidence/composition/toupper_memchr"
case "$EVID" in /*) ;; *) EVID="$ROOT/$EVID" ;; esac

PHOST="$ROOT/target/debug/phost"
FILE="composition_verdict.json"

echo "=== Phorensic OS — Sealed Composition Dispatch Court Verification ==="
echo "Target:       $TARGET_ID"
echo "Evidence dir: $EVID"
echo "Mode:         $MODE"
echo

if [ ! -x "$PHOST" ]; then
    echo "--- building phost ---"
    if ! cargo build -q -p phost -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

# --- 1. Produce a fresh run in a temp dir (never touches EVID) -------------
TMP="$(mktemp -d)"
CMP="$(mktemp -d)"
trap 'rm -rf "$TMP" "$CMP"' EXIT
VALIDATE_DIR="$TMP"

echo "--- fresh composition run (temp) ---"
if ! "$PHOST" port compose --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port compose failed"
    "$PHOST" port compose --out "$TMP" || true
    exit 2
fi

if [ "$MODE" = "check-committed" ]; then
    echo "--- committed-evidence court (fresh == checked-in; committed untouched) ---"
    if [ ! -f "$EVID/$FILE" ]; then
        echo "  [FAIL] committed composition verdict missing: $EVID/$FILE"
        exit 1
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] fresh composition run matches the checked-in verdict"
    else
        echo "  [FAIL] committed composition verdict differs from a fresh run"
        exit 1
    fi
else
    echo "--- determinism court (fresh A == fresh B) ---"
    if ! "$PHOST" port compose --out "$EVID" >/dev/null 2>&1; then
        echo "ERROR: second composition run failed"
        exit 2
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] two fresh composition runs are byte-identical"
    else
        echo "  [FAIL] non-deterministic composition verdict"
        exit 1
    fi
    # Validate the committed copy (never point the EXIT trap at it).
    VALIDATE_DIR="$EVID"
fi

echo "--- validating the chain evidence ---"
python3 - "$VALIDATE_DIR" "$ROOT" "$TARGET_ID" "$EXPECTED_COUNT" <<'PY'
import json, os, sys

d, root, TARGET_ID, EXPECTED = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
errors = []

def load(path, name):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}

v = load(os.path.join(d, "composition_verdict.json"), "composition_verdict.json")

if v.get("target") != TARGET_ID:
    errors.append("composition target != %s" % TARGET_ID)

stages = v.get("stages", [])
for required in ("libc:toupper:c-locale:u8:v1", "libc:memchr:c-locale:index:v1"):
    if required not in stages:
        errors.append("composition stages do not include %s" % required)

if v.get("cases_run") != EXPECTED:
    errors.append("cases_run != %d" % EXPECTED)

# Every stage must be sealed-native for every case, with no fallback and no
# broken seal. A broken seal is not a fallback; both are disqualifying.
for stage in ("toupper_hay_native_cases", "toupper_needle_native_cases", "memchr_native_cases"):
    if v.get(stage) != EXPECTED:
        errors.append("%s != %d (stage was not served by the sealed object)" % (stage, EXPECTED))
if v.get("fallback_cases") != 0:
    errors.append("fallback_cases != 0 (foreign fallback in the sealed path)")
if v.get("broken_seal_cases") != 0:
    errors.append("broken_seal_cases != 0 (a sealed entry failed verification)")
if v.get("cases_passed") != EXPECTED:
    errors.append("cases_passed != %d" % EXPECTED)
if v.get("cases_failed") != 0:
    errors.append("cases_failed != 0")
if v.get("verdict") != "consistent":
    errors.append("verdict != consistent")
if v.get("mismatches"):
    errors.append("composition reported mismatches")
if not v.get("chain_hash"):
    errors.append("chain_hash is missing")
if not v.get("oracle_hash"):
    errors.append("oracle_hash is missing")
if not v.get("dispatches_run"):
    errors.append("dispatches_run is missing")

# The objects the chain dispatched to must be the committed sealed leaf objects.
def leaf_object_hash(symbol):
    p = os.path.join(root, "phost", "evidence", "porting", symbol, "candidate_signature.json")
    doc = load(p, "%s candidate_signature.json" % symbol)
    return doc.get("candidate_object_hash", "")

tupper = leaf_object_hash("toupper")
memchr = leaf_object_hash("memchr")
if v.get("toupper_object_hash") != tupper:
    errors.append("toupper_object_hash does not match the committed sealed toupper object")
if v.get("memchr_object_hash") != memchr:
    errors.append("memchr_object_hash does not match the committed sealed memchr object")
if not v.get("toupper_elf_symbol", "").startswith("_phor_"):
    errors.append("toupper_elf_symbol is missing or unmangled")
if not v.get("memchr_elf_symbol", "").startswith("_phor_"):
    errors.append("memchr_elf_symbol is missing or unmangled")

print("Target: %s" % v.get("target", "?"))
print("Stages: %s" % " -> ".join(stages))
print("Cases: %d" % v.get("cases_run", 0))
print("toupper(haystack) native: %d" % v.get("toupper_hay_native_cases", 0))
print("toupper(needle) native:   %d" % v.get("toupper_needle_native_cases", 0))
print("memchr native:            %d" % v.get("memchr_native_cases", 0))
print("Foreign fallback: %d" % v.get("fallback_cases", 0))
print("Broken seal:      %d" % v.get("broken_seal_cases", 0))
print("Passed: %d" % v.get("cases_passed", 0))
print("Failed: %d" % v.get("cases_failed", 0))
print("Dispatches: %d" % v.get("dispatches_run", 0))
print("toupper object: %s" % ("MATCH" if v.get("toupper_object_hash") == tupper else "MISMATCH"))
print("memchr object:  %s" % ("MATCH" if v.get("memchr_object_hash") == memchr else "MISMATCH"))
print("Chain hash bound: %s" % ("yes" if v.get("chain_hash") else "no"))
print("Verdict: %s" % v.get("verdict", "?"))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
exit $?
