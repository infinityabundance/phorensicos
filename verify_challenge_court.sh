#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Court-Sensitivity (Challenge) Court Verifier
#
#  Phase 3: a passing candidate is weak evidence if the measuring instrument is
#  blind. This verifier regenerates the challenge evidence and checks that the
#  court detects every declared, non-equivalent defect family over the same corpus
#  and oracle the real court uses.
#
#    (default)          determinism court  — regenerate twice, require byte-equal.
#    --check-committed  committed-evidence court — compare against the checked-in
#                       verdict without touching it.
#
#  Usage:
#    ./verify_challenge_court.sh [--target <symbol|composition>] [--check-committed]
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="toupper"
MODE="regenerate"
while [ $# -gt 0 ]; do
    case "$1" in
        --check-committed) MODE="check-committed" ;;
        --target) TARGET="$2"; shift ;;
        --target=*) TARGET="${1#--target=}" ;;
        *) ;;
    esac
    shift
done

EVID="$ROOT/phost/evidence/challenge/$TARGET"
FILE="challenge_verdict.json"
PHOST="$ROOT/target/debug/phost"

echo "=== Phorensic OS — Court-Sensitivity (Challenge) Court Verification ==="
echo "Target:       $TARGET"
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

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if ! "$PHOST" port challenge "$TARGET" --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port challenge $TARGET failed"
    "$PHOST" port challenge "$TARGET" --out "$TMP" || true
    exit 2
fi

VALIDATE_DIR="$TMP"
if [ "$MODE" = "check-committed" ]; then
    if [ ! -f "$EVID/$FILE" ]; then
        echo "  [FAIL] committed challenge verdict missing: $EVID/$FILE"
        exit 1
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] fresh challenge run matches the checked-in verdict"
    else
        echo "  [FAIL] committed challenge verdict differs from a fresh run"
        exit 1
    fi
else
    if ! "$PHOST" port challenge "$TARGET" --out "$EVID" >/dev/null 2>&1; then
        echo "ERROR: second challenge run failed"
        exit 2
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] two fresh challenge runs are byte-identical"
    else
        echo "  [FAIL] non-deterministic challenge verdict"
        exit 1
    fi
    VALIDATE_DIR="$EVID"
fi

echo "--- validating the challenge evidence ---"
python3 - "$VALIDATE_DIR" <<'PY'
import json, os, re, sys

d = sys.argv[1]
errors = []
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def load(path):
    with open(path) as fh:
        return json.load(fh)


try:
    v = load(os.path.join(d, "challenge_verdict.json"))
except Exception as e:  # noqa: BLE001
    print("cannot read/parse challenge_verdict.json: %s" % e)
    sys.exit(1)

if v.get("schema") != "phorensic.porting.challenge_verdict.v1":
    errors.append("schema is not challenge_verdict.v1")
if v.get("court") not in ("leaf", "composition"):
    errors.append("court is not leaf|composition")
if v.get("families_total", 0) < 1:
    errors.append("profile has no mutants")
if v.get("families_undetected") != 0:
    errors.append("families_undetected != 0 (a declared family is a blind spot)")
if v.get("families_invalid") != 0:
    errors.append("families_invalid != 0 (an undetermined mutant blocks the claim)")
if v.get("court_sensitive") is not True:
    errors.append("court_sensitive is not true")
if v.get("verdict") != "court-sensitive":
    errors.append("verdict != court-sensitive")
if not HEX64.match(v.get("residual_hash", "")):
    errors.append("residual_hash is missing or malformed")
if not HEX64.match(v.get("corpus_hash", "")):
    errors.append("corpus_hash is missing or malformed")

mutants = v.get("mutants", [])
if len(mutants) != v.get("families_total"):
    errors.append("mutant count does not match families_total")

detected = 0
equivalent = 0
for m in mutants:
    mid = m.get("id", "?")
    total = m.get("total_cases", 0)
    cases = m.get("detected_cases", 0)
    if cases > total:
        errors.append("%s: detected_cases > total_cases" % mid)
    if m.get("equivalent"):
        equivalent += 1
        if not m.get("equivalence_note"):
            errors.append("%s: equivalent mutant has no equivalence note" % mid)
        continue
    if not m.get("valid"):
        errors.append("%s: invalid mutant in a sensitive profile" % mid)
        continue
    if not m.get("detected"):
        errors.append("%s: declared family is not detected" % mid)
        continue
    detected += 1
    if m.get("specific") and not (0 < cases < total):
        errors.append("%s: specific but cases are not localized" % mid)

if detected != v.get("families_detected"):
    errors.append("detected count does not match families_detected")
if equivalent != v.get("families_equivalent"):
    errors.append("equivalent count does not match families_equivalent")

print("Target: %s" % v.get("target"))
print("Court:  %s" % v.get("court"))
print("Families: %d detected, %d equivalent, %d invalid (of %d)"
      % (detected, equivalent, v.get("families_invalid"), v.get("families_total")))
for m in mutants:
    mark = "equiv" if m.get("equivalent") else ("detected" if m.get("detected") else "MISSED")
    print("  %-30s %-9s %5s/%-5s %s"
          % (m.get("id"), mark, m.get("detected_cases"), m.get("total_cases"),
             "specific" if m.get("specific") else "coarse"))
print("Verdict: %s" % v.get("verdict"))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
exit $?
