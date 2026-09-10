#!/bin/bash
# ============================================================================
#  Phorensic OS — Sealed Native Session Court Verifier
#
#  The one-shot runtime paths (`port native`, `port compose`) load the committed
#  store for a single call. A running system does not. The sealed native
#  **service** owns one verified index for its whole lifetime, so many consumers
#  are served from the same seal and every leaf object is mapped once and reused.
#
#  This verifier checks the committed session residual
#  (phost/evidence/session/session_verdict.json) and proves:
#    * the store is loaded exactly ONCE (`store_loads == 1`) for eleven calls
#    * every planned call was served by a sealed object: 11 native, 0 foreign
#      fallback, 0 broken seals
#    * eleven ports resolve to FIVE mapped objects — one seal reused by many consumers
#    * the fan-in is real: `toupper` serves 30 resolutions, `memchr` 8, `strlen` 4
#      (nested stage resolutions included), and `toupper_each` is consumed 3 times
#      (its own call plus the nested chain's two fold stages)
#    * the session residual hash covers the reported fields
#    * the session names the committed store and its residual hash
#    * a fresh run is byte-identical to the checked-in verdict
#    * the same verdict reproduces from a *copy* of the store at another path
#    * a missing store fails closed; without PORTING the store is never read
#
#  This verifier writes only to a temp directory; committed evidence is never
#  modified.
#
#  Usage:
#    ./verify_session.sh
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

STORE="phost/evidence/store/index.json"
EVID="phost/evidence/session"
FILE="session_verdict.json"
PHOST="$ROOT/target/debug/phost"

echo "=== Phorensic OS — Sealed Native Session Court Verification ==="
echo "Evidence: $EVID/$FILE"
echo

if [ ! -x "$PHOST" ]; then
    echo "--- building phost ---"
    if ! cargo build -q -p phost -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

if [ ! -f "$EVID/$FILE" ]; then
    echo "ERROR: committed session verdict missing: $EVID/$FILE"
    exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAIL=0
pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; FAIL=1; }

# --- 1. Fresh run == committed (never touches committed evidence) ------------
echo "--- fresh session run (temp) ---"
if "$PHOST" port session --out "$TMP" >"$TMP/run.txt" 2>&1; then
    if cmp -s "$EVID/$FILE" "$TMP/$FILE"; then
        pass "fresh session run matches the checked-in verdict"
    else
        fail "committed session verdict differs from a fresh run"
        diff "$EVID/$FILE" "$TMP/$FILE" | head -20 | sed 's/^/  /'
    fi
else
    fail "phost port session failed"
    sed 's/^/  /' "$TMP/run.txt"
fi

# --- 2. The same verdict from a copy of the store at another path ------------
echo
echo "--- the seal loads from any verified copy ---"
cp "$STORE" "$TMP/store-copy.json"
if "$PHOST" port session --out "$TMP/copy" --store "$TMP/store-copy.json" >/dev/null 2>&1; then
    # Only the recorded store *path* differs; the behavior must be identical.
    if python3 - "$EVID/$FILE" "$TMP/copy/$FILE" <<'PY'
import json, sys

a = json.load(open(sys.argv[1]))
b = json.load(open(sys.argv[2]))
keys = [
    "store_loads", "ports_in_store", "calls", "native_calls",
    "fallback_calls", "broken_seal_calls", "objects_mapped", "dispatches",
    "session_hash", "per_port", "verdict",
]
sys.exit(0 if all(a.get(k) == b.get(k) for k in keys) else 1)
PY
    then
        pass "the session reproduces from a copy of the store at another path"
    else
        fail "the session behavior changed when the store was loaded from a copy"
    fi
else
    fail "the session did not run from a store copy"
fi

# --- 3. Validate the session evidence ---------------------------------------
echo
echo "--- validating the session residual ---"
python3 - "$ROOT" "$EVID/$FILE" "$STORE" <<'PY'
import hashlib, json, os, sys

root, evid_rel, store_rel = sys.argv[1], sys.argv[2], sys.argv[3]
errors = []


def load(path, name):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}


def rel(p):
    return p if os.path.isabs(p) else os.path.join(root, p)


v = load(rel(evid_rel), evid_rel)
store = load(rel(store_rel), store_rel)

if v.get("schema") != "phorensic.porting.session_verdict.v1":
    errors.append("session schema != phorensic.porting.session_verdict.v1")
if v.get("target") != "phor:session:sealed-native-service:v1":
    errors.append("session target is not the sealed native service")

# The session must name the committed store and its residual.
if v.get("store") != store_rel:
    errors.append("session does not name the committed store (%s)" % store_rel)
if v.get("store_residual_hash") != store.get("residual_hash"):
    errors.append("session store_residual_hash != the committed store residual")

# One load, many calls: the whole point of the service.
if v.get("store_loads") != 1:
    errors.append("store_loads != 1 (the store was not loaded exactly once)")
if v.get("ports_in_store") != len(store.get("entries", [])):
    errors.append("ports_in_store != the committed store entry count")

if v.get("calls") != 11:
    errors.append("calls != 11")
if v.get("native_calls") != 11:
    errors.append("native_calls != 11 (a port was not served by a sealed object)")
if v.get("fallback_calls") != 0:
    errors.append("fallback_calls != 0 (a foreign fallback entered the sealed path)")
if v.get("broken_seal_calls") != 0:
    errors.append("broken_seal_calls != 0 (a sealed entry failed verification)")
if v.get("mismatches"):
    errors.append("session reported mismatches")
if v.get("verdict") != "consistent":
    errors.append("verdict != consistent")

# Ten ports, five objects: the leaves are mapped once and reused.
if v.get("objects_mapped") != 5:
    errors.append("objects_mapped != 5 (sealed objects were not reused)")

# Every sealed port in the store must have been served, and the fan-in must be
# the one the plan implies (nested stage resolutions included).
EXPECTED_FANIN = {
    "libc:toupper:c-locale:u8:v1": 30,
    "libc:memchr:c-locale:index:v1": 8,
    "libc:strlen:c-locale:u64:v1": 4,
    "libc:memcmp:c-locale:sign:v1": 1,
    "libc:strrchr:c-locale:index:v1": 1,
    "phor:compose:toupper_memchr:c-locale:index:v1": 1,
    "phor:compose:toupper_strlen_memchr:c-locale:index:v1": 1,
    "phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1": 1,
    "phor:compose:toupper_each:c-locale:u8s:v1": 3,
    "phor:compose:toupper_each_strlen_memchr:c-locale:index:v1": 1,
    "phor:compose:toupper_memchr_suffix:c-locale:index:v1": 1,
}
per_port = v.get("per_port", {})
if set(per_port) != set(EXPECTED_FANIN):
    missing = set(EXPECTED_FANIN) - set(per_port)
    extra = set(per_port) - set(EXPECTED_FANIN)
    if missing:
        errors.append("ports not served: %s" % ", ".join(sorted(missing)))
    if extra:
        errors.append("unexpected ports in the session: %s" % ", ".join(sorted(extra)))
for port, expected in EXPECTED_FANIN.items():
    got = per_port.get(port)
    if got != expected:
        errors.append("%s: fan-in %s != %s" % (port, got, expected))

if v.get("dispatches") != sum(EXPECTED_FANIN.values()):
    errors.append("dispatches != the sum of the per-port resolutions")
if len(v.get("session_hash", "")) != 64:
    errors.append("session_hash is missing or not a SHA-256 digest")

# The residual hash must cover the reported fields (same canonical form as Rust:
# the consumer map iterates in lexicographic key order).
per_port_canon = ",".join(
    "%s=%s" % (k, per_port[k]) for k in sorted(per_port)
)
canonical = (
    "target=%s;store=%s;store_residual_hash=%s;store_loads=%s;ports_in_store=%s;"
    "calls=%s;native_calls=%s;fallback_calls=%s;broken_seal_calls=%s;"
    "objects_mapped=%s;dispatches=%s;per_port=%s;session_hash=%s;verdict=%s"
) % (
    v.get("target", ""),
    v.get("store", ""),
    v.get("store_residual_hash", ""),
    v.get("store_loads", ""),
    v.get("ports_in_store", ""),
    v.get("calls", ""),
    v.get("native_calls", ""),
    v.get("fallback_calls", ""),
    v.get("broken_seal_calls", ""),
    v.get("objects_mapped", ""),
    v.get("dispatches", ""),
    per_port_canon,
    v.get("session_hash", ""),
    v.get("verdict", ""),
)
want = hashlib.sha256(canonical.encode()).hexdigest()
if v.get("residual_hash") != want:
    errors.append("residual_hash does not cover the reported session fields")

print("Target:          %s" % v.get("target", "?"))
print("Store loads:     %s" % v.get("store_loads", "?"))
print("Ports in store:  %s" % v.get("ports_in_store", "?"))
print("Calls:           %s (native %s)" % (v.get("calls", "?"), v.get("native_calls", "?")))
print("Foreign fallback: %s" % v.get("fallback_calls", "?"))
print("Broken seal:     %s" % v.get("broken_seal_calls", "?"))
print("Objects mapped:  %s" % v.get("objects_mapped", "?"))
print("Dispatches:      %s" % v.get("dispatches", "?"))
print("Session hash:    %s" % v.get("session_hash", "?"))
print("Residual hash:   %s" % ("MATCH" if v.get("residual_hash") == want else "MISMATCH"))
print("Verdict:         %s" % v.get("verdict", "?"))

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
if [ $? -ne 0 ]; then FAIL=1; fi

# --- 4. Fail closed; no ambient authority -----------------------------------
echo
echo "--- fail closed; no ambient authority ---"

if "$PHOST" port session --out "$TMP/missing" \
    --store phost/evidence/store/does-not-exist.json >/dev/null 2>&1; then
    fail "a missing store did not fail closed"
else
    pass "a missing store fails closed"
fi

if ! "$PHOST" port session --no-capability 2>&1 | grep -q "capability denied"; then
    fail "the service opened without the PORTING capability"
else
    pass "without PORTING the service refuses and the store is never read"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "Status: ALL CHECKS PASSED"
    exit 0
fi
echo "Status: SOME CHECKS FAILED"
exit 1
