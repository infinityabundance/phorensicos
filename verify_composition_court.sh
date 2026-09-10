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
#  Six compositions are checked, selected with --target:
#
#    toupper_memchr         (default)  phor:compose:toupper_memchr:c-locale:index:v1
#    toupper_strlen_memchr            phor:compose:toupper_strlen_memchr:c-locale:index:v1
#    toupper_strlen_memchr_pair       phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1
#    toupper_each                     phor:compose:toupper_each:c-locale:u8s:v1
#    toupper_each_strlen_memchr       phor:compose:toupper_each_strlen_memchr:c-locale:index:v1
#    toupper_memchr_suffix            phor:compose:toupper_memchr_suffix:c-locale:index:v1
#
#  Each is built from already-sealed ports and executed entirely through the sealed
#  dispatcher. The nested one resolves the sealed composition `toupper_each` from the
#  store. The last is the first chain where a derived value selects a **buffer**: the
#  folded haystack is sliced at the origin the first `memchr` derived, and its second
#  search is data-dependent (no origin = no slice = the search is never dispatched),
#  which the verifier checks as a group rather than as a per-case native count.
#  This verifier checks:
#    * the composition verdict exists and is valid JSON
#    * every stage was served by the SEALED object for every case: zero foreign
#      fallback and zero broken seals (a broken seal is never a fallback, and both
#      are disqualifying)
#    * every case matched the foreign oracle
#    * the objects the chain dispatched to are exactly the committed sealed leaf
#      objects (object hash cross-check against the leaf candidate signatures)
#    * for the nested chain, the recorded nested seal (the fold composition's chain
#      hash) is exactly the committed `toupper_each` chain hash — a seal of a seal
#    * the chain hash is present (it covers the intermediates — including, for the
#      chains with a length stage, the bound the strlen stage derived, and for the
#      pair chain both indexes — not just the answer)
#
#  Usage:
#    ./verify_composition_court.sh [--target <name>] [--check-committed] [evidence_dir]
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="toupper_memchr"
MODE="regenerate"
EVID=""
while [ $# -gt 0 ]; do
    case "$1" in
        --check-committed) MODE="check-committed" ;;
        --target) TARGET="$2"; shift ;;
        --target=*) TARGET="${1#--target=}" ;;
        *) EVID="$1" ;;
    esac
    shift
done

# Per-target expectations.
case "$TARGET" in
    toupper_memchr)
        TARGET_ID="phor:compose:toupper_memchr:c-locale:index:v1"
        EXPECTED_COUNT=560
        ;;
    toupper_strlen_memchr)
        TARGET_ID="phor:compose:toupper_strlen_memchr:c-locale:index:v1"
        EXPECTED_COUNT=350
        ;;
    toupper_strlen_memchr_pair)
        TARGET_ID="phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1"
        EXPECTED_COUNT=474
        ;;
    toupper_each)
        TARGET_ID="phor:compose:toupper_each:c-locale:u8s:v1"
        EXPECTED_COUNT=267
        ;;
    toupper_each_strlen_memchr)
        TARGET_ID="phor:compose:toupper_each_strlen_memchr:c-locale:index:v1"
        EXPECTED_COUNT=350
        ;;
    toupper_memchr_suffix)
        TARGET_ID="phor:compose:toupper_memchr_suffix:c-locale:index:v1"
        EXPECTED_COUNT=688
        ;;
    *)
        echo "ERROR: unknown composition target '$TARGET'"
        exit 2
        ;;
esac

[ -n "$EVID" ] || EVID="phost/evidence/composition/$TARGET"
case "$EVID" in /*) ;; *) EVID="$ROOT/$EVID" ;; esac

PHOST="$ROOT/target/debug/phost"
FILE="composition_verdict.json"

echo "=== Phorensic OS — Sealed Composition Dispatch Court Verification ==="
echo "Target:       $TARGET ($TARGET_ID)"
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
if ! "$PHOST" port compose --target "$TARGET" --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port compose --target $TARGET failed"
    "$PHOST" port compose --target "$TARGET" --out "$TMP" || true
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
    if ! "$PHOST" port compose --target "$TARGET" --out "$EVID" >/dev/null 2>&1; then
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
python3 - "$VALIDATE_DIR" "$ROOT" "$TARGET_ID" "$EXPECTED_COUNT" "$TARGET" <<'PY'
import json, os, sys

d, root, TARGET_ID, EXPECTED, SYMBOL = (
    sys.argv[1],
    sys.argv[2],
    sys.argv[3],
    int(sys.argv[4]),
    sys.argv[5],
)
errors = []

def load(path, name):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}

v = load(os.path.join(d, "composition_verdict.json"), "composition_verdict.json")

# Per-composition expectation tables. Each stage is (label, json field prefix,
# committed leaf symbol directory).
SPECS = {
    "toupper_memchr": {
        "stages": ["libc:toupper:c-locale:u8:v1", "libc:memchr:c-locale:index:v1"],
        "native_keys": [
            "toupper_hay_native_cases",
            "toupper_needle_native_cases",
            "memchr_native_cases",
        ],
        "objects": [
            ("toupper(haystack)", "toupper", "toupper"),
            ("toupper(needle)", "toupper", "toupper"),
            ("memchr", "memchr", "memchr"),
        ],
    },
    "toupper_strlen_memchr": {
        "stages": [
            "libc:toupper:c-locale:u8:v1",
            "libc:strlen:c-locale:u64:v1",
            "libc:memchr:c-locale:index:v1",
        ],
        "native_keys": [
            "toupper_hay_native_cases",
            "strlen_native_cases",
            "toupper_needle_native_cases",
            "memchr_native_cases",
        ],
        "objects": [
            ("toupper(haystack)", "toupper", "toupper"),
            ("strlen(derived bound)", "strlen", "strlen"),
            ("memchr", "memchr", "memchr"),
        ],
    },
    "toupper_strlen_memchr_pair": {
        "stages": [
            "libc:toupper:c-locale:u8:v1",
            "libc:strlen:c-locale:u64:v1",
            "libc:memchr:c-locale:index:v1",
        ],
        "native_keys": [
            "toupper_hay_native_cases",
            "strlen_native_cases",
            "toupper_needle_a_native_cases",
            "memchr_a_native_cases",
            "toupper_needle_b_native_cases",
            "memchr_b_native_cases",
        ],
        "objects": [
            ("toupper(haystack)", "toupper", "toupper"),
            ("strlen(derived bound)", "strlen", "strlen"),
            ("memchr(needle A)", "memchr", "memchr"),
        ],
    },
    "toupper_each": {
        "stages": ["libc:toupper:c-locale:u8:v1"],
        "native_keys": ["toupper_native_cases"],
        "objects": [("toupper(each byte)", "toupper", "toupper")],
    },
    "toupper_each_strlen_memchr": {
        "stages": [
            "phor:compose:toupper_each:c-locale:u8s:v1",
            "libc:strlen:c-locale:u64:v1",
            "libc:memchr:c-locale:index:v1",
        ],
        "native_keys": [
            "fold_hay_native_cases",
            "strlen_native_cases",
            "fold_needle_native_cases",
            "memchr_native_cases",
        ],
        # The fold stages dispatch the *composition*, so the only leaf objects to
        # cross-check are strlen and memchr; the nested seal is checked separately.
        "objects": [
            ("strlen(derived bound)", "strlen", "strlen"),
            ("memchr", "memchr", "memchr"),
        ],
        "nested_seal": {
            "field": "fold_composition_chain_hash",
            "composition": "toupper_each",
        },
    },
    "toupper_memchr_suffix": {
        "stages": [
            "libc:toupper:c-locale:u8:v1",
            "libc:memchr:c-locale:index:v1",
        ],
        # The fold stages and the origin search run on every case; the suffix search
        # is data-dependent, so its two accounts (ran natively / never reached
        # because needleA was absent) are checked as a group below.
        "native_keys": [
            "toupper_hay_native_cases",
            "toupper_needle_a_native_cases",
            "toupper_needle_b_native_cases",
            "memchr_a_native_cases",
        ],
        "native_key_groups": [
            (["memchr_b_native_cases", "memchr_b_not_reached_cases"], 688),
        ],
        "objects": [
            ("toupper(haystack)", "toupper", "toupper"),
            ("memchr(origin)", "memchr", "memchr"),
        ],
    },
}

spec = SPECS.get(SYMBOL)
if spec is None:
    errors.append("no verifier specification for composition %s" % SYMBOL)
    spec = {"stages": [], "native_keys": [], "objects": []}

if v.get("target") != TARGET_ID:
    errors.append("composition target != %s" % TARGET_ID)

stages = v.get("stages", [])
for required in spec["stages"]:
    if required not in stages:
        errors.append("composition stages do not include %s" % required)

if v.get("cases_run") != EXPECTED:
    errors.append("cases_run != %d" % EXPECTED)

# Every stage must be sealed-native, with no fallback and no broken seal. A broken
# seal is not a fallback; both are disqualifying. Stages that are data-dependently
# skipped are checked as a group (the group must sum to the case count).
for key in spec["native_keys"]:
    if v.get(key) != EXPECTED:
        errors.append("%s != %d (stage was not served by the sealed object)" % (key, EXPECTED))
for keys, total in spec.get("native_key_groups", []):
    got = sum(v.get(k, 0) for k in keys)
    if got != total:
        errors.append(
            "%s != %d (a data-dependent stage is unaccounted for)" % ("+".join(keys), total)
        )
if v.get("memchr_b_not_reached_cases") is not None:
    ran = v.get("memchr_b_native_cases")
    skipped = v.get("memchr_b_not_reached_cases")
    if not (ran > 0 and skipped > 0):
        errors.append(
            "the suffix search was not exercised both ways (ran %s, not reached %s)"
            % (ran, skipped)
        )
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

results = []
for label, prefix, symbol in spec["objects"]:
    leaf = leaf_object_hash(symbol)
    got = v.get("%s_object_hash" % prefix)
    if got != leaf:
        errors.append(
            "%s_object_hash does not match the committed sealed %s object" % (prefix, symbol)
        )
    if not v.get("%s_elf_symbol" % prefix, "").startswith("_phor_"):
        errors.append("%s_elf_symbol is missing or unmangled" % prefix)
    results.append((label, got, leaf))

# A nested composition stage is sealed with the *chain hash* of the composition it
# dispatches, so verify the recorded value against that composition's committed verdict
# — a seal of a seal.
nested = spec.get("nested_seal")
nested_result = None
if nested:
    inner = load(
        os.path.join(
            root,
            "phost",
            "evidence",
            "composition",
            nested["composition"],
            "composition_verdict.json",
        ),
        "%s composition_verdict.json" % nested["composition"],
    )
    expected_chain = inner.get("chain_hash", "")
    got_chain = v.get(nested["field"], "")
    if not expected_chain:
        errors.append("committed %s verdict has no chain hash" % nested["composition"])
    if got_chain != expected_chain:
        errors.append(
            "%s (%s) does not match the committed %s chain hash"
            % (nested["field"], got_chain, nested["composition"])
        )
    if v.get("fold_composition_id") != "phor:compose:%s:c-locale:u8s:v1" % nested["composition"]:
        errors.append("fold_composition_id is not the nested composition id")
    nested_result = (nested["composition"], got_chain, expected_chain)

# Unique stages, in the order they first appear.
print("Target: %s" % v.get("target", "?"))
print("Stages: %s" % " -> ".join(stages))
print("Cases: %d" % v.get("cases_run", 0))
for key in spec["native_keys"]:
    print("%s: %d" % (key, v.get(key, 0)))
for keys, total in spec.get("native_key_groups", []):
    print("%s: %d" % ("+".join(keys), sum(v.get(k, 0) for k in keys)))
print("Foreign fallback: %d" % v.get("fallback_cases", 0))
print("Broken seal:      %d" % v.get("broken_seal_cases", 0))
print("Passed: %d" % v.get("cases_passed", 0))
print("Failed: %d" % v.get("cases_failed", 0))
print("Dispatches: %d" % v.get("dispatches_run", 0))
seen = set()
for label, got, leaf in results:
    if leaf in seen:
        continue
    seen.add(leaf)
    print("%s object: %s" % (label, "MATCH" if got == leaf else "MISMATCH"))
if nested_result is not None:
    name, got, expected = nested_result
    print(
        "nested %s chain hash: %s"
        % (name, "MATCH" if got == expected else "MISMATCH")
    )
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
