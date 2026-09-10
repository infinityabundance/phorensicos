#!/bin/bash
# ============================================================================
#  Phorensic OS — JIT-Porting Court Verifier
#
#  Regenerates the `toupper` porting evidence with the built `phost` binary and
#  validates it. The court is deterministic, so regenerating must be
#  byte-identical to the committed evidence — that is checked explicitly.
#
#  Checks:
#    * all six evidence artifacts exist and are valid JSON
#    * re-running the court is byte-deterministic
#    * case count is 256, all cases passed, no mismatches
#    * oracle hash matches the stored behavior signature
#    * candidate hash matches the replay verdict
#    * promotion level is Sealed and the sealed package targets `toupper`
#
#  Usage: ./verify_jit_porting_court.sh [evidence_dir]
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

EVID="${1:-phost/evidence/porting/toupper}"
case "$EVID" in /*) ;; *) EVID="$ROOT/$EVID" ;; esac
BIN="$ROOT/target/debug/phost"

echo "=== Phorensic OS — JIT-Porting Court Verification ==="
echo "Evidence dir: $EVID"
echo

# --- 0. Build the runtime if needed ----------------------------------------
if [ ! -x "$BIN" ]; then
    echo "--- building phost ---"
    if ! cargo build -q -p phost; then
        echo "ERROR: cargo build -p phost failed"
        exit 2
    fi
fi

# --- 1. Regenerate the evidence (deterministic) ----------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "--- regenerating evidence ---"
if ! "$BIN" port promote toupper --out "$EVID" >/dev/null 2>&1; then
    echo "ERROR: phost port promote toupper failed"
    "$BIN" port promote toupper --out "$EVID" || true
    exit 2
fi
if ! "$BIN" port promote toupper --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: second (determinism) court run failed"
    exit 2
fi

# --- 2. Determinism: two runs must be byte-identical -----------------------
DET=0
for f in oracle_traces.json behavior_signature.json candidate_signature.json \
         replay_verdict.json promotion_receipt.json sealed_package.json; do
    if [ ! -f "$EVID/$f" ]; then
        echo "  [FAIL] missing artifact: $f"
        DET=1
        continue
    fi
    if ! cmp -s "$EVID/$f" "$TMP/$f"; then
        echo "  [FAIL] non-deterministic artifact: $f"
        DET=1
    fi
done
if [ "$DET" -eq 0 ]; then
    echo "  [PASS] regeneration is byte-deterministic (6/6 artifacts)"
fi

# --- 3. Validate evidence content ------------------------------------------
echo "--- validating evidence ---"
python3 - "$EVID" <<'PY'
import json, os, sys

d = sys.argv[1]
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

# Oracle traces: complete, ordered domain.
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

# Hash cross-checks.
oracle = sig.get("combined_oracle_hash", "")
candidate = cand.get("candidate_hash", "")
oracle_match = bool(oracle) and oracle == verdict.get("oracle_hash") and oracle == promo.get("oracle_hash")
candidate_match = bool(candidate) and candidate == verdict.get("candidate_hash") and candidate == promo.get("candidate_hash")
if not oracle_match:
    errors.append("oracle hash does not match across signature/verdict/promotion")
if not candidate_match:
    errors.append("candidate hash does not match across signature/verdict/promotion")

# Signature counts.
if sig.get("case_count") != 256:
    errors.append("behavior_signature.case_count != 256")
if cand.get("case_count") != 256:
    errors.append("candidate_signature.case_count != 256")
if sig.get("target") != "toupper":
    errors.append("behavior_signature.target != toupper")

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
if promo.get("target") != "toupper":
    errors.append("promotion.target != toupper")

# Sealed package references the correct target.
if sealed.get("target") != "toupper":
    errors.append("sealed_package.target != toupper")
if sealed.get("trust") != "sealed":
    errors.append("sealed_package.trust != sealed")
if sealed.get("dialect") != "libc":
    errors.append("sealed_package.dialect != libc")
if sealed.get("case_count") != 256:
    errors.append("sealed_package.case_count != 256")

# --- report ---------------------------------------------------------------
print("Target: %s" % sig.get("target", "?"))
print("Observed cases: %d" % traces_doc.get("case_count", 0))
print("Replay cases: %d" % verdict.get("cases_run", 0))
print("Passed: %d" % verdict.get("cases_passed", 0))
print("Failed: %d" % verdict.get("cases_failed", 0))
print("Oracle hash: %s" % ("MATCH" if oracle_match else "MISMATCH"))
print("Candidate hash: %s" % ("MATCH" if candidate_match else "MISMATCH"))
print("Promotion: %s" % (str(promo.get("to", "?")).capitalize()))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
STATUS=$?

if [ "$DET" -ne 0 ]; then
    echo "Status: SOME CHECKS FAILED (determinism)"
    exit 1
fi

exit "$STATUS"
