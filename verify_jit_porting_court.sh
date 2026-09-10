#!/bin/bash
# ============================================================================
#  Phorensic OS — JIT-Porting Court Verifier
#
#  Two courts, one entry point:
#
#    (default)          determinism court  — regenerate the evidence twice and
#                       require the two fresh runs to be byte-identical. This
#                       overwrites the evidence dir with a fresh run.
#
#    --check-committed  committed-evidence court — write ONLY to a temp dir and
#                       compare it against the checked-in evidence, without
#                       touching the committed files. This is the reviewer-grade
#                       check: it proves the committed seal matches a fresh run.
#
#  Checks (both modes):
#    * all six evidence artifacts exist and are valid JSON
#    * case count is 256, all cases passed, no mismatches
#    * the recorded locale contract is C and the target id is qualified
#    * oracle hash matches the stored behavior signature
#    * candidate behavior hash matches the replay verdict
#    * the candidate source hash is bound (non-empty)
#    * promotion level is Sealed and the sealed package targets `toupper`
#
#  Usage:
#    ./verify_jit_porting_court.sh [--check-committed] [evidence_dir]
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

MODE="regenerate"
EVID=""
for arg in "$@"; do
    case "$arg" in
        --check-committed) MODE="check-committed" ;;
        *) EVID="$arg" ;;
    esac
done
[ -n "$EVID" ] || EVID="phost/evidence/porting/toupper"
case "$EVID" in /*) ;; *) EVID="$ROOT/$EVID" ;; esac

BIN="$ROOT/target/debug/phost"
FILES="oracle_traces.json behavior_signature.json candidate_signature.json replay_verdict.json promotion_receipt.json sealed_package.json"

echo "=== Phorensic OS — JIT-Porting Court Verification ==="
echo "Evidence dir: $EVID"
echo "Mode:         $MODE"
echo

# --- 0. Build the runtime if needed ----------------------------------------
if [ ! -x "$BIN" ]; then
    echo "--- building phost ---"
    if ! cargo build -q -p phost; then
        echo "ERROR: cargo build -p phost failed"
        exit 2
    fi
fi

# --- 1. Produce a fresh run in a temp dir (never touches EVID) -------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "--- fresh court run (temp) ---"
if ! "$BIN" port promote toupper --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port promote toupper failed"
    "$BIN" port promote toupper --out "$TMP" || true
    exit 2
fi

if [ "$MODE" = "check-committed" ]; then
    # --- committed-evidence court ------------------------------------------
    echo "--- committed-evidence court (fresh == checked-in; committed untouched) ---"
    if [ ! -d "$EVID" ]; then
        echo "  [FAIL] committed evidence dir missing: $EVID"
        exit 1
    fi
    FAIL=0
    for f in $FILES; do
        if [ ! -f "$EVID/$f" ]; then
            echo "  [FAIL] missing committed artifact: $f"
            FAIL=1
        elif ! cmp -s "$EVID/$f" "$TMP/$f"; then
            echo "  [FAIL] committed artifact differs from a fresh run: $f"
            FAIL=1
        fi
    done
    [ "$FAIL" -eq 0 ] && echo "  [PASS] fresh run matches checked-in evidence (6/6 artifacts)"
    [ "$FAIL" -eq 0 ] || { echo "Status: COMMITTED EVIDENCE STALE"; exit 1; }
else
    # --- determinism court --------------------------------------------------
    echo "--- determinism court (fresh A == fresh B) ---"
    if ! "$BIN" port promote toupper --out "$EVID" >/dev/null 2>&1; then
        echo "ERROR: second court run failed"
        exit 2
    fi
    FAIL=0
    for f in $FILES; do
        if ! cmp -s "$EVID/$f" "$TMP/$f"; then
            echo "  [FAIL] non-deterministic artifact: $f"
            FAIL=1
        fi
    done
    [ "$FAIL" -eq 0 ] && echo "  [PASS] two fresh runs are byte-identical (6/6 artifacts)"
    [ "$FAIL" -eq 0 ] || { echo "Status: NON-DETERMINISTIC"; exit 1; }
fi

# --- 2. Validate evidence content ------------------------------------------
echo "--- validating evidence ---"
python3 - "$EVID" <<'PY'
import json, os, sys

d = sys.argv[1]
TARGET_ID = "libc:toupper:c-locale:u8:v1"
errors = []

def load(name):
    p = os.path.join(d, name)
    try:
        with open(p) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}

traces_doc = load("oracle_traces.json")
sig = load("behavior_signature.json")
cand = load("candidate_signature.json")
verdict = load("replay_verdict.json")
promo = load("promotion_receipt.json")
sealed = load("sealed_package.json")

# Oracle traces: complete, ordered domain, qualified target, C locale.
traces = traces_doc.get("traces", [])
expected_ids = ["0x%02x" % i for i in range(256)]
if traces_doc.get("case_count") != 256:
    errors.append("oracle_traces.case_count != 256")
if len(traces) != 256:
    errors.append("oracle_traces has %d traces, expected 256" % len(traces))
else:
    if [t.get("case_id") for t in traces] != expected_ids:
        errors.append("oracle trace case_ids are not the ordered 0x00..0xff domain")
    if any(t.get("status") != "ok" for t in traces):
        errors.append("some observed cases are not status=ok")
    if any(t.get("target") != TARGET_ID for t in traces):
        errors.append("some traces are not for the qualified target id")
    if any(t.get("locale_contract") != "C" for t in traces):
        errors.append("some traces do not record locale_contract=C")

# Hash cross-checks.
oracle = sig.get("combined_oracle_hash", "")
behavior = cand.get("candidate_behavior_hash", "")
source = cand.get("candidate_source_hash", "")
oracle_match = bool(oracle) and oracle == verdict.get("oracle_hash") and oracle == promo.get("oracle_hash") and oracle == sealed.get("oracle_hash")
behavior_match = bool(behavior) and behavior == verdict.get("candidate_behavior_hash") and behavior == promo.get("candidate_behavior_hash") and behavior == sealed.get("candidate_behavior_hash")
if not oracle_match:
    errors.append("oracle hash does not match across signature/verdict/promotion/sealed")
if not behavior_match:
    errors.append("candidate behavior hash does not match across signature/verdict/promotion/sealed")
if not source:
    errors.append("candidate source hash is missing (seal does not bind candidate source)")
if source and source != promo.get("candidate_source_hash"):
    errors.append("candidate source hash does not match promotion receipt")
if source and source != sealed.get("candidate_source_hash"):
    errors.append("candidate source hash does not match sealed package")

# Signature identity + counts.
if sig.get("target") != TARGET_ID:
    errors.append("behavior_signature.target != %s" % TARGET_ID)
if sig.get("locale_contract") != "C":
    errors.append("behavior_signature.locale_contract != C")
if sig.get("case_count") != 256:
    errors.append("behavior_signature.case_count != 256")
if cand.get("case_count") != 256:
    errors.append("candidate_signature.case_count != 256")

# Replay verdict: everything passed.
if verdict.get("cases_run") != 256:
    errors.append("replay.cases_run != 256")
if verdict.get("cases_passed") != 256:
    errors.append("replay.cases_passed != 256")
if verdict.get("cases_failed") != 0:
    errors.append("replay.cases_failed != 0")
if verdict.get("verdict") != "consistent":
    errors.append("replay.verdict != consistent")
if verdict.get("mismatches"):
    errors.append("replay reported mismatches")

# Promotion sealed.
if promo.get("to") != "sealed":
    errors.append("promotion.to != sealed")
if promo.get("verdict") != "consistent":
    errors.append("promotion.verdict != consistent")
if promo.get("target") != TARGET_ID:
    errors.append("promotion.target != %s" % TARGET_ID)

# Sealed package references the correct target.
if sealed.get("target") != TARGET_ID:
    errors.append("sealed_package.target != %s" % TARGET_ID)
if sealed.get("symbol") != "toupper":
    errors.append("sealed_package.symbol != toupper")
if sealed.get("trust") != "sealed":
    errors.append("sealed_package.trust != sealed")
if sealed.get("dialect") != "libc":
    errors.append("sealed_package.dialect != libc")
if sealed.get("locale_contract") != "C":
    errors.append("sealed_package.locale_contract != C")
if sealed.get("case_count") != 256:
    errors.append("sealed_package.case_count != 256")

# --- report ---------------------------------------------------------------
print("Target: %s" % sealed.get("symbol", "?"))
print("Observed cases: %d" % traces_doc.get("case_count", 0))
print("Replay cases: %d" % verdict.get("cases_run", 0))
print("Passed: %d" % verdict.get("cases_passed", 0))
print("Failed: %d" % verdict.get("cases_failed", 0))
print("Oracle hash: %s" % ("MATCH" if oracle_match else "MISMATCH"))
print("Candidate hash: %s" % ("MATCH" if behavior_match else "MISMATCH"))
print("Promotion: %s" % (str(promo.get("to", "?")).capitalize()))
print("Target id: %s" % TARGET_ID)
print("Source hash bound: %s" % ("yes" if source else "no"))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
exit $?
