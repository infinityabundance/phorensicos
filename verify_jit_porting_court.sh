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
#  Checks (both modes, for the selected target):
#    * all eight evidence artifacts exist and are valid JSON
#    * the corpus is structurally well-formed (see the target-specific checks)
#    * every replay case passed, with no mismatches
#    * the recorded locale contract is C and the target id is qualified
#    * oracle / candidate-behavior hashes match across artifacts
#    * an INDEPENDENT recompilation of the .phor candidate reproduces the
#      recorded candidate object and receipt hashes
#    * the sealed object EXECUTION court passed: the object that was loaded and
#      called is the sealed object, every case matched, and the execution hash is
#      bound into the promotion receipt and the sealed package
#    * the sealed native DISPATCH court passed: the runtime dispatcher served
#      every case from the sealed object (zero foreign fallbacks) and the
#      dispatch hash is bound into the promotion receipt and the sealed package
#    * promotion level is Sealed and the sealed package targets the right symbol
#
#  Usage:
#    ./verify_jit_porting_court.sh [--target toupper|memcmp|memchr|strlen] [--check-committed] [evidence_dir]
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="toupper"
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
    toupper)
        TARGET_ID="libc:toupper:c-locale:u8:v1"
        EXPECTED_COUNT=256
        SOURCE="examples/jit_port_toupper.phor"
        ;;
    memcmp)
        TARGET_ID="libc:memcmp:c-locale:sign:v1"
        EXPECTED_COUNT=312
        SOURCE="examples/jit_port_memcmp.phor"
        ;;
    memchr)
        TARGET_ID="libc:memchr:c-locale:index:v1"
        EXPECTED_COUNT=482
        SOURCE="examples/jit_port_memchr.phor"
        ;;
    strlen)
        TARGET_ID="libc:strlen:c-locale:u64:v1"
        EXPECTED_COUNT=308
        SOURCE="examples/jit_port_strlen.phor"
        ;;
    *)
        echo "ERROR: unknown target '$TARGET' (expected toupper|memcmp|memchr|strlen)"
        exit 2
        ;;
esac

[ -n "$EVID" ] || EVID="phost/evidence/porting/$TARGET"
case "$EVID" in /*) ;; *) EVID="$ROOT/$EVID" ;; esac

PHOST="$ROOT/target/debug/phost"
PHORC="$ROOT/target/debug/phorc"
FILES="oracle_traces.json behavior_signature.json candidate_signature.json replay_verdict.json execution_verdict.json dispatch_verdict.json promotion_receipt.json sealed_package.json"

echo "=== Phorensic OS — JIT-Porting Court Verification ==="
echo "Target:       $TARGET ($TARGET_ID)"
echo "Evidence dir: $EVID"
echo "Mode:         $MODE"
echo

# --- 0. Build the runtime + compiler if needed -----------------------------
if [ ! -x "$PHOST" ] || [ ! -x "$PHORC" ]; then
    echo "--- building phost + phorc ---"
    if ! cargo build -q -p phost -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

# --- 1. Produce a fresh run in a temp dir (never touches EVID) -------------
TMP="$(mktemp -d)"
CMP="$(mktemp -d)"
trap 'rm -rf "$TMP" "$CMP"' EXIT

echo "--- fresh court run (temp) ---"
if ! "$PHOST" port promote "$TARGET" --out "$TMP" >/dev/null 2>&1; then
    echo "ERROR: phost port promote $TARGET failed"
    "$PHOST" port promote "$TARGET" --out "$TMP" || true
    exit 2
fi

if [ "$MODE" = "check-committed" ]; then
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
    [ "$FAIL" -eq 0 ] && echo "  [PASS] fresh run matches checked-in evidence (8/8 artifacts)"
    [ "$FAIL" -eq 0 ] || { echo "Status: COMMITTED EVIDENCE STALE"; exit 1; }
else
    echo "--- determinism court (fresh A == fresh B) ---"
    if ! "$PHOST" port promote "$TARGET" --out "$EVID" >/dev/null 2>&1; then
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
    [ "$FAIL" -eq 0 ] && echo "  [PASS] two fresh runs are byte-identical (8/8 artifacts)"
    [ "$FAIL" -eq 0 ] || { echo "Status: NON-DETERMINISTIC"; exit 1; }
fi

# --- 2. Independent recompilation of the .phor candidate -------------------
# Compile from the workspace root with the repo-relative source, exactly as the
# court does, and hash the artifacts. These must match the committed seal.
echo "--- independent recompilation ---"
if ! (cd "$ROOT" && "$PHORC" "$SOURCE" "$CMP/candidate.o" --emit-receipts) >/dev/null 2>&1; then
    echo "  [FAIL] independent phorc compilation failed"
    exit 1
fi
OBJ_HASH="$(sha256sum "$CMP/candidate.o" | cut -d' ' -f1)"
RCP_HASH="$(sha256sum "$CMP/candidate.receipts.json" | cut -d' ' -f1)"

# --- 3. Validate evidence content ------------------------------------------
echo "--- validating evidence ---"
python3 - "$EVID" "$TARGET_ID" "$EXPECTED_COUNT" "$TARGET" "$OBJ_HASH" "$RCP_HASH" <<'PY'
import json, os, sys

d, TARGET_ID, EXPECTED, SYMBOL = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
OBJ_HASH, RCP_HASH = sys.argv[5], sys.argv[6]
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
exec_v = load("execution_verdict.json")
disp_v = load("dispatch_verdict.json")
promo = load("promotion_receipt.json")
sealed = load("sealed_package.json")

traces = traces_doc.get("traces", [])

# ---- oracle traces: completeness, identity, locale ------------------------
if traces_doc.get("case_count") != EXPECTED:
    errors.append("oracle_traces.case_count != %d" % EXPECTED)
if len(traces) != EXPECTED:
    errors.append("oracle_traces has %d traces, expected %d" % (len(traces), EXPECTED))
if traces:
    ids = [t.get("case_id") for t in traces]
    if len(set(ids)) != len(ids):
        errors.append("oracle trace case_ids are not unique")
    if any((t.get("target") != TARGET_ID) for t in traces):
        errors.append("some traces are not for the qualified target id")
    if any(t.get("locale_contract") != "C" for t in traces):
        errors.append("some traces do not record locale_contract=C")
    if any(t.get("status") != "ok" for t in traces):
        errors.append("some observed cases are not status=ok")

# ---- target-specific corpus structure -------------------------------------
def args_of(t):
    return t.get("input_hex", "").split(":")

if SYMBOL == "toupper":
    expected_ids = ["0x%02x" % i for i in range(256)]
    if [t.get("case_id") for t in traces] != expected_ids:
        errors.append("toupper case_ids are not the ordered 0x00..0xff domain")
    for t in traces:
        a = args_of(t)
        if len(a) != 1 or len(a[0]) != 2:
            errors.append("toupper case %s is not a single 1-byte argument" % t.get("case_id"))
            break

if SYMBOL == "memcmp":
    for t in traces:
        a = args_of(t)
        if len(a) != 3:
            errors.append("memcmp case %s does not have exactly 3 arguments" % t.get("case_id"))
            break
        if len(a[2]) != 16:
            errors.append("memcmp case %s has a non-8-byte length" % t.get("case_id"))
            break
        try:
            buflen_a = len(bytes.fromhex(a[0]))
            buflen_b = len(bytes.fromhex(a[1]))
            n = int.from_bytes(bytes.fromhex(a[2]), "little")
        except ValueError:
            errors.append("memcmp case %s has non-hex arguments" % t.get("case_id"))
            break
        if n > min(buflen_a, buflen_b):
            errors.append("memcmp case %s has n > min(len(a), len(b))" % t.get("case_id"))
            break
    id_set = set(t.get("case_id") for t in traces)
    for required in ("F.7f.80", "F.80.7f"):
        if required not in id_set:
            errors.append("memcmp corpus is missing case %s" % required)
    for group in ("A.", "B.", "C.", "D.", "E.", "F.", "G."):
        if not any(i.startswith(group) for i in id_set):
            errors.append("memcmp corpus is missing group %s" % group)

if SYMBOL == "memchr":
    for t in traces:
        a = args_of(t)
        if len(a) != 3:
            errors.append("memchr case %s does not have exactly 3 arguments" % t.get("case_id"))
            break
        if len(a[1]) != 2:
            errors.append("memchr case %s does not have a 1-byte needle" % t.get("case_id"))
            break
        if len(a[2]) != 16:
            errors.append("memchr case %s has a non-8-byte length" % t.get("case_id"))
            break
        try:
            hay = bytes.fromhex(a[0])
            n = int.from_bytes(bytes.fromhex(a[2]), "little")
        except ValueError:
            errors.append("memchr case %s has non-hex arguments" % t.get("case_id"))
            break
        if n > len(hay):
            errors.append("memchr case %s has n > len(haystack)" % t.get("case_id"))
            break
        if n > 8 or len(hay) > 8:
            errors.append("memchr case %s exceeds the 8-byte packed-word contract" % t.get("case_id"))
            break
        if len(t.get("output_hex", "")) != 8:
            errors.append("memchr case %s output is not a 4-byte index" % t.get("case_id"))
            break
    id_set = set(t.get("case_id") for t in traces)
    for required in ("A.000", "B.zero.0", "C2.ones.3", "D.4.2.n2", "E.80", "F.7f", "F.80"):
        if required not in id_set:
            errors.append("memchr corpus is missing case %s" % required)
    for group in ("A.", "B.", "C.", "C2.", "D.", "E.", "F."):
        if not any(i.startswith(group) for i in id_set):
            errors.append("memchr corpus is missing group %s" % group)

if SYMBOL == "strlen":
    for t in traces:
        a = args_of(t)
        if len(a) != 2:
            errors.append("strlen case %s does not have exactly 2 arguments" % t.get("case_id"))
            break
        if len(a[1]) != 16:
            errors.append("strlen case %s has a non-8-byte scan bound" % t.get("case_id"))
            break
        try:
            buf = bytes.fromhex(a[0])
            n = int.from_bytes(bytes.fromhex(a[1]), "little")
        except ValueError:
            errors.append("strlen case %s has non-hex arguments" % t.get("case_id"))
            break
        if n > len(buf):
            errors.append("strlen case %s has n > len(buf)" % t.get("case_id"))
            break
        if n > 8 or len(buf) > 8:
            errors.append("strlen case %s exceeds the 8-byte packed-word contract" % t.get("case_id"))
            break
        # The ABI precondition: a NUL terminator inside the scan bound.
        if 0 not in buf[:n]:
            errors.append("strlen case %s has no terminator inside its bound" % t.get("case_id"))
            break
        if len(t.get("output_hex", "")) != 16:
            errors.append("strlen case %s output is not an 8-byte length" % t.get("case_id"))
            break
    id_set = set(t.get("case_id") for t in traces)
    for required in ("A.0.1", "A.7.8", "B2.empty.high", "C.0", "D.00", "D.7f", "D.80", "D.ff"):
        if required not in id_set:
            errors.append("strlen corpus is missing case %s" % required)
    for group in ("A.", "B.", "B2.", "C.", "D."):
        if not any(i.startswith(group) for i in id_set):
            errors.append("strlen corpus is missing group %s" % group)
    if len([i for i in id_set if i.startswith("A.")]) != 36:
        errors.append("strlen corpus does not have the complete 36-case (k, n) grid")
    if len([i for i in id_set if i.startswith("D.")]) != 256:
        errors.append("strlen corpus does not have the exhaustive 256-value sweep")

# ---- hash cross-checks ----------------------------------------------------
oracle = sig.get("combined_oracle_hash", "")
behavior = cand.get("candidate_behavior_hash", "")
source = cand.get("candidate_source_hash", "")
obj = cand.get("candidate_object_hash", "")
rcp = cand.get("candidate_receipt_hash", "")
compiler = cand.get("compiler_version", "")

oracle_match = bool(oracle) and oracle == verdict.get("oracle_hash") and oracle == promo.get("oracle_hash") and oracle == sealed.get("oracle_hash")
behavior_match = bool(behavior) and behavior == verdict.get("candidate_behavior_hash") and behavior == promo.get("candidate_behavior_hash") and behavior == sealed.get("candidate_behavior_hash")
if not oracle_match:
    errors.append("oracle hash does not match across signature/verdict/promotion/sealed")
if not behavior_match:
    errors.append("candidate behavior hash does not match across signature/verdict/promotion/sealed")

# ---- compiled candidate authority -----------------------------------------
if not source:
    errors.append("candidate source hash is missing")
if not obj:
    errors.append("candidate object hash is missing")
if not rcp:
    errors.append("candidate receipt hash is missing")
if not compiler:
    errors.append("compiler version is missing")
for name, doc in (("promotion_receipt", promo), ("sealed_package", sealed)):
    if doc.get("candidate_source_hash") != source:
        errors.append("%s.candidate_source_hash != candidate_signature" % name)
    if doc.get("candidate_object_hash") != obj:
        errors.append("%s.candidate_object_hash != candidate_signature" % name)
    if doc.get("candidate_receipt_hash") != rcp:
        errors.append("%s.candidate_receipt_hash != candidate_signature" % name)
compiled_match = (obj == OBJ_HASH and rcp == RCP_HASH)
if not compiled_match:
    errors.append("independent recompilation does not reproduce the sealed object/receipt hashes")

# ---- identity + counts ----------------------------------------------------
if sig.get("target") != TARGET_ID:
    errors.append("behavior_signature.target != %s" % TARGET_ID)
if sig.get("locale_contract") != "C":
    errors.append("behavior_signature.locale_contract != C")
if sig.get("case_count") != EXPECTED:
    errors.append("behavior_signature.case_count != %d" % EXPECTED)
if cand.get("case_count") != EXPECTED:
    errors.append("candidate_signature.case_count != %d" % EXPECTED)

# ---- replay verdict -------------------------------------------------------
if verdict.get("cases_run") != EXPECTED:
    errors.append("replay.cases_run != %d" % EXPECTED)
if verdict.get("cases_passed") != EXPECTED:
    errors.append("replay.cases_passed != %d" % EXPECTED)
if verdict.get("cases_failed") != 0:
    errors.append("replay.cases_failed != 0")
if verdict.get("verdict") != "consistent":
    errors.append("replay.verdict != consistent")
if verdict.get("mismatches"):
    errors.append("replay reported mismatches")

# ---- execution court (the sealed object was loaded and executed) ----------
exec_hash = exec_v.get("execution_hash", "")
if exec_v.get("target") != TARGET_ID:
    errors.append("execution_verdict.target != %s" % TARGET_ID)
if exec_v.get("cases_run") != EXPECTED:
    errors.append("execution.cases_run != %d" % EXPECTED)
if exec_v.get("cases_passed") != EXPECTED:
    errors.append("execution.cases_passed != %d" % EXPECTED)
if exec_v.get("cases_failed") != 0:
    errors.append("execution.cases_failed != 0")
if exec_v.get("verdict") != "consistent":
    errors.append("execution.verdict != consistent")
if exec_v.get("mismatches"):
    errors.append("execution reported mismatches")
if not exec_hash:
    errors.append("execution_hash is missing")
if not exec_v.get("abi_symbol", ""):
    errors.append("execution.abi_symbol is missing")
if not exec_v.get("elf_symbol", ""):
    errors.append("execution.elf_symbol is missing")
# The object that was executed must be exactly the object bound into the seal.
if exec_v.get("object_hash") != obj:
    errors.append("execution.object_hash != candidate_signature.candidate_object_hash")
if exec_v.get("object_hash") != sealed.get("candidate_object_hash"):
    errors.append("execution.object_hash != sealed_package.candidate_object_hash")
if exec_v.get("oracle_hash") != oracle:
    errors.append("execution.oracle_hash != behavior_signature.combined_oracle_hash")
# The promotion/seal must carry the execution hash and symbol.
sealed_exec_symbol = "_phor_" + str(sealed.get("candidate_abi_symbol", ""))
if promo.get("execution_hash") != exec_hash:
    errors.append("promotion_receipt.execution_hash != execution_verdict.execution_hash")
if sealed.get("candidate_execution_hash") != exec_hash:
    errors.append("sealed_package.candidate_execution_hash != execution_verdict.execution_hash")
if sealed.get("candidate_abi_symbol") != exec_v.get("abi_symbol"):
    errors.append("sealed_package.candidate_abi_symbol != execution.abi_symbol")
if sealed.get("executed_elf_symbol") != exec_v.get("elf_symbol"):
    errors.append("sealed_package.executed_elf_symbol != execution.elf_symbol")
if sealed_exec_symbol != exec_v.get("elf_symbol"):
    errors.append("sealed_package candidate_abi_symbol does not mangle to the executed ELF symbol")
if sealed.get("execution_verdict") != "consistent":
    errors.append("sealed_package.execution_verdict != consistent")

# ---- dispatch court (the runtime preferred the sealed object) -------------
dispatch_hash = disp_v.get("dispatch_hash", "")
if disp_v.get("target") != TARGET_ID:
    errors.append("dispatch_verdict.target != %s" % TARGET_ID)
if disp_v.get("cases_run") != EXPECTED:
    errors.append("dispatch.cases_run != %d" % EXPECTED)
if disp_v.get("native_cases") != EXPECTED:
    errors.append("dispatch.native_cases != %d (runtime did not prefer native)" % EXPECTED)
if disp_v.get("fallback_cases") != 0:
    errors.append("dispatch.fallback_cases != 0")
if disp_v.get("broken_seal_cases") != 0:
    errors.append("dispatch.broken_seal_cases != 0")
if disp_v.get("cases_passed") != EXPECTED:
    errors.append("dispatch.cases_passed != %d" % EXPECTED)
if disp_v.get("cases_failed") != 0:
    errors.append("dispatch.cases_failed != 0")
if disp_v.get("verdict") != "consistent":
    errors.append("dispatch.verdict != consistent")
if disp_v.get("mismatches"):
    errors.append("dispatch reported mismatches")
if not dispatch_hash:
    errors.append("dispatch_hash is missing")
if disp_v.get("object_hash") != obj:
    errors.append("dispatch.object_hash != candidate_signature.candidate_object_hash")
if disp_v.get("oracle_hash") != oracle:
    errors.append("dispatch.oracle_hash != behavior_signature.combined_oracle_hash")
if not disp_v.get("elf_symbol", ""):
    errors.append("dispatch.elf_symbol is missing")
if promo.get("dispatch_hash") != dispatch_hash:
    errors.append("promotion_receipt.dispatch_hash != dispatch_verdict.dispatch_hash")
if sealed.get("dispatch_hash") != dispatch_hash:
    errors.append("sealed_package.dispatch_hash != dispatch_verdict.dispatch_hash")
if sealed.get("dispatch_verdict") != "consistent":
    errors.append("sealed_package.dispatch_verdict != consistent")
if sealed.get("dispatch_native_cases") != EXPECTED:
    errors.append("sealed_package.dispatch_native_cases != %d" % EXPECTED)
if sealed.get("dispatch_fallback_cases") != 0:
    errors.append("sealed_package.dispatch_fallback_cases != 0")
if sealed.get("dispatch_broken_seal_cases") != 0:
    errors.append("sealed_package.dispatch_broken_seal_cases != 0")

# ---- promotion + sealed package -------------------------------------------
if promo.get("to") != "sealed":
    errors.append("promotion.to != sealed")
if promo.get("verdict") != "consistent":
    errors.append("promotion.verdict != consistent")
if promo.get("target") != TARGET_ID:
    errors.append("promotion.target != %s" % TARGET_ID)
if sealed.get("target") != TARGET_ID:
    errors.append("sealed_package.target != %s" % TARGET_ID)
if sealed.get("symbol") != SYMBOL:
    errors.append("sealed_package.symbol != %s" % SYMBOL)
if sealed.get("trust") != "sealed":
    errors.append("sealed_package.trust != sealed")
if sealed.get("dialect") != "libc":
    errors.append("sealed_package.dialect != libc")
if sealed.get("locale_contract") != "C":
    errors.append("sealed_package.locale_contract != C")
if sealed.get("case_count") != EXPECTED:
    errors.append("sealed_package.case_count != %d" % EXPECTED)

# --- report ---------------------------------------------------------------
print("Target: %s" % sealed.get("symbol", SYMBOL))
print("Observed cases: %d" % traces_doc.get("case_count", 0))
print("Replay cases: %d" % verdict.get("cases_run", 0))
print("Passed: %d" % verdict.get("cases_passed", 0))
print("Failed: %d" % verdict.get("cases_failed", 0))
print("Oracle hash: %s" % ("MATCH" if oracle_match else "MISMATCH"))
print("Candidate hash: %s" % ("MATCH" if behavior_match else "MISMATCH"))
print("Promotion: %s" % (str(promo.get("to", "?")).capitalize()))
print("Target id: %s" % TARGET_ID)
print("Source hash bound: %s" % ("yes" if source else "no"))
print("Compiled object: %s" % ("MATCH" if compiled_match else "MISMATCH"))
print("Compiler: %s" % compiler)
print("Execution cases: %d" % exec_v.get("cases_run", 0))
print("Execution passed: %d" % exec_v.get("cases_passed", 0))
print("Execution failed: %d" % exec_v.get("cases_failed", 0))
print("Executed symbol: %s" % exec_v.get("elf_symbol", "?"))
print("Execution object: %s" % ("MATCH" if exec_v.get("object_hash") == obj else "MISMATCH"))
print("Execution hash bound: %s" % ("yes" if exec_hash else "no"))
print("Dispatch cases: %d" % disp_v.get("cases_run", 0))
print("Dispatch native: %d" % disp_v.get("native_cases", 0))
print("Dispatch fallback: %d" % disp_v.get("fallback_cases", 0))
print("Dispatch broken seal: %d" % disp_v.get("broken_seal_cases", 0))
print("Dispatch object: %s" % ("MATCH" if disp_v.get("object_hash") == obj else "MISMATCH"))
print("Dispatch hash bound: %s" % ("yes" if dispatch_hash else "no"))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
exit $?
