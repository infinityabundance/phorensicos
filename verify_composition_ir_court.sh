#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Generic (IR) Sealed Composition Dispatch Court Verifier
#
#  Phase 2 of the autonomous porting foundry. The seven compositions are no
#  longer bespoke Rust runners: each is a typed `CompositionIR` (data) evaluated
#  by one interpreter on both the foreign and the sealed side. This verifier
#  checks the **v2** evidence schema, which distinguishes four artifact identities:
#
#    composition_ir_hash          — the graph itself (canonical content identity)
#    dependency_binding_hash      — every dependency + the seal it published
#    behavior_hash                — normalized observed behavior over the corpus
#    composition_artifact_hash    — the bound composition artifact
#
#  Two modes, one entry point (mirroring verify_composition_court.sh):
#
#    (default)          determinism court — regenerate the v2 evidence twice and
#                       require the two fresh runs to be byte-identical.
#
#    --check-committed  committed-evidence court — write ONLY to a temp dir and
#                       compare it against the checked-in verdict, without
#                       touching the committed file.
#
#  Usage:
#    ./verify_composition_ir_court.sh [--target <name>] [--check-committed]
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="toupper_memchr"
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

case "$TARGET" in
    toupper_memchr) TARGET_ID="phor:compose:toupper_memchr:c-locale:index:v1"; COUNT=560 ;;
    toupper_strlen_memchr) TARGET_ID="phor:compose:toupper_strlen_memchr:c-locale:index:v1"; COUNT=350 ;;
    toupper_strlen_memchr_pair) TARGET_ID="phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1"; COUNT=474 ;;
    toupper_each) TARGET_ID="phor:compose:toupper_each:c-locale:u8s:v1"; COUNT=267 ;;
    toupper_each_strlen_memchr) TARGET_ID="phor:compose:toupper_each_strlen_memchr:c-locale:index:v1"; COUNT=350 ;;
    toupper_memchr_suffix) TARGET_ID="phor:compose:toupper_memchr_suffix:c-locale:index:v1"; COUNT=688 ;;
    toupper_each_slice_search) TARGET_ID="phor:compose:toupper_each_slice_search:c-locale:index:v1"; COUNT=688 ;;
    *) echo "ERROR: unknown composition target '$TARGET'"; exit 2 ;;
esac

EVID="$ROOT/phost/evidence/composition_ir/$TARGET"
FILE="composition_verdict.json"
PHOST="$ROOT/target/debug/phost"

echo "=== Phorensic OS — Generic (IR) Composition Court Verification ==="
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

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "--- fresh generic composition run (persistent store, temp) ---"
if ! "$PHOST" port compose --target "$TARGET" --ir --store --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port compose --ir --target $TARGET failed"
    "$PHOST" port compose --target "$TARGET" --ir --store --out "$TMP" || true
    exit 2
fi

VALIDATE_DIR="$TMP"
if [ "$MODE" = "check-committed" ]; then
    if [ ! -f "$EVID/$FILE" ]; then
        echo "  [FAIL] committed v2 verdict missing: $EVID/$FILE"
        exit 1
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] fresh generic run matches the checked-in v2 verdict"
    else
        echo "  [FAIL] committed v2 verdict differs from a fresh run"
        exit 1
    fi
else
    echo "--- determinism court (fresh A == fresh B) ---"
    if ! "$PHOST" port compose --target "$TARGET" --ir --store --out "$EVID" >/dev/null 2>&1; then
        echo "ERROR: second generic run failed"
        exit 2
    fi
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        echo "  [PASS] two fresh generic runs are byte-identical"
    else
        echo "  [FAIL] non-deterministic generic verdict"
        exit 1
    fi
    VALIDATE_DIR="$EVID"
fi

echo "--- validating the v2 chain evidence ---"
python3 - "$VALIDATE_DIR" "$ROOT" "$TARGET_ID" "$COUNT" "$TARGET" <<'PY'
import json, os, re, sys

d, root, TARGET_ID, EXPECTED, NAME = (
    sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5],
)
errors = []
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def load(path, name):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}


v = load(os.path.join(d, "composition_verdict.json"), "composition_verdict.json")

if v.get("schema") != "phorensic.porting.composition_verdict.v2":
    errors.append("schema is not composition_verdict.v2")
if v.get("target") != TARGET_ID:
    errors.append("target != %s" % TARGET_ID)
if v.get("cases_run") != EXPECTED:
    errors.append("cases_run != %d" % EXPECTED)
if v.get("cases_passed") != EXPECTED:
    errors.append("cases_passed != %d" % EXPECTED)
if v.get("cases_failed") != 0:
    errors.append("cases_failed != 0")
if v.get("fallback_cases") != 0:
    errors.append("fallback_cases != 0 (foreign fallback in the sealed path)")
if v.get("broken_seal_cases") != 0:
    errors.append("broken_seal_cases != 0 (a sealed entry failed verification)")
if v.get("verdict") != "consistent":
    errors.append("verdict != consistent")
if v.get("mismatches"):
    errors.append("composition reported mismatches")
if not v.get("oracle_hash"):
    errors.append("oracle_hash is missing")

# Four distinct identities, each a canonical 64-hex digest.
identities = {}
for key in (
    "composition_ir_hash",
    "dependency_binding_hash",
    "behavior_hash",
    "composition_artifact_hash",
):
    got = v.get(key, "")
    if not HEX64.match(got):
        errors.append("%s is not a 64-char hex digest" % key)
    identities[key] = got
# The graph identity and the observed behavior are different statements.
if identities["composition_ir_hash"] == identities["behavior_hash"]:
    errors.append("composition_ir_hash must not equal behavior_hash")

# The store's published seals, for a seal-of-a-seal cross-check.
store = load(os.path.join(root, "phost", "evidence", "store", "index.json"), "store index.json")
seals = {}
for e in store.get("entries", []):
    if e.get("kind") == "leaf-object":
        seals[e.get("target")] = e.get("object_hash", "")
    elif e.get("kind") == "composition":
        seals[e.get("target")] = e.get("chain_hash", "")

stages = v.get("stages", [])
if not stages:
    errors.append("no stages recorded")
seen_ports = set()
for s in stages:
    total = (
        s.get("native_cases", 0)
        + s.get("not_reached_cases", 0)
        + s.get("fallback_cases", 0)
        + s.get("broken_cases", 0)
    )
    if total != EXPECTED:
        errors.append(
            "stage %s#%s accounts for %d cases, expected %d"
            % (s.get("port"), s.get("node"), total, EXPECTED)
        )
    if s.get("fallback_cases", 0) != 0 or s.get("broken_cases", 0) != 0:
        errors.append("stage %s#%s has a fallback/broken account" % (s.get("port"), s.get("node")))
    port = s.get("port", "")
    kind = s.get("kind", "")
    if kind in ("leaf", "composition", "map"):
        seen_ports.add(port)
        expected_seal = seals.get(port, "")
        if not expected_seal:
            errors.append("stage %s is not in the committed store" % port)
        elif s.get("seal", "") != expected_seal:
            errors.append(
                "stage %s seal does not match the committed store seal" % port
            )
        if kind in ("leaf", "map") and not s.get("symbol", "").startswith("_phor_"):
            errors.append("leaf stage %s has no _phor_ ELF symbol" % port)
        if kind == "composition" and not s.get("symbol", "").startswith("compose:"):
            errors.append("composition stage %s has no compose: symbol" % port)
if not any(s.get("native_cases", 0) > 0 for s in stages):
    errors.append("no stage was served natively")

print("Target: %s" % v.get("target", "?"))
print("Cases: %s" % v.get("cases_run", 0))
print("composition_ir_hash:       %s" % identities["composition_ir_hash"][:16])
print("dependency_binding_hash:   %s" % identities["dependency_binding_hash"][:16])
print("behavior_hash:             %s" % identities["behavior_hash"][:16])
print("composition_artifact_hash: %s" % identities["composition_artifact_hash"][:16])
for s in stages:
    print(
        "  %-42s %5d native / %5d not reached / %5d cases"
        % (s.get("port", "?") + "#" + str(s.get("node")), s.get("native_cases", 0),
           s.get("not_reached_cases", 0), EXPECTED)
    )
print("Stages in the committed store: %d" % len(seen_ports))
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
