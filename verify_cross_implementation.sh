#!/bin/bash
# ============================================================================
#  Phorensic OS — Cross-Implementation Court Verifier
#
#  Every seal binds an observation of *one* implementation: the host C library.
#  So "the POSIX contract for `strspn`" is really "the POSIX contract as this host
#  implements it". This verifier checks the residual that closes that gap: the same
#  sealed corpus observed through a **second, independent implementation** (musl),
#  with agreement required on every case.
#
#  Checks:
#    * the second implementation really is independent: the probe is compiled by
#      `musl-gcc`, reports `libc=musl` from its own `#ifdef __GLIBC__`, and its ELF
#      has NO program interpreter (statically linked), so it cannot be the host's
#      library in disguise
#    * for every sealed leaf, a fresh run matches the committed verdict byte for byte
#    * the committed verdict's `primary_oracle_hash` equals the leaf's committed
#      sealed oracle hash — the claim is bound to the seal it refers to
#    * `secondary_oracle_hash == primary_oracle_hash`: the two implementations
#      produce the *same* sealed trace set, not merely the same answers
#    * agreements == cases, disagreements == 0, no mismatches
#    * the probe's source hash matches the committed probe source, and the probe
#      binary hash is present but explicitly NOT asserted reproducible (it is
#      toolchain-bound, like the kernel image)
#    * the residual hash covers the reported fields
#    * without PORTING the court refuses
#
#  This verifier writes only to a temp directory; committed evidence is never
#  modified.
#
#  Usage:
#    ./verify_cross_implementation.sh
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error
#  (including a missing musl toolchain: a cross-implementation claim that cannot
#  run the second implementation must fail loudly, never pass quietly).
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

PROBE_SRC="phost/foreign/musl_probe.c"
PHOST="$ROOT/target/debug/phost"

# target symbol -> expected case count
SYMBOLS="toupper memcmp memchr strlen strrchr strspn"
expected_count() {
    case "$1" in
        toupper) echo 256 ;;
        memcmp) echo 312 ;;
        memchr) echo 482 ;;
        strlen) echo 308 ;;
        strrchr) echo 336 ;;
        strspn) echo 578 ;;
        *) echo 0 ;;
    esac
}

echo "=== Phorensic OS — Cross-Implementation Court Verification ==="
echo "Probe source: $PROBE_SRC"
echo

if [ ! -x "$PHOST" ]; then
    echo "--- building phost ---"
    if ! cargo build -q -p phost -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

if ! command -v musl-gcc >/dev/null 2>&1; then
    echo "ERROR: musl-gcc is required: the cross-implementation court must observe a"
    echo "       second implementation, and there is no way to fake that."
    echo "       Debian/Ubuntu: apt-get install musl-tools"
    exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAIL=0
pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; FAIL=1; }

# --- 1. The second implementation is genuinely independent -------------------
echo "--- the second implementation ---"
PROBE="$TMP/musl_probe"
if ! musl-gcc -static -O1 -o "$PROBE" "$PROBE_SRC" 2>"$TMP/cc.log"; then
    echo "ERROR: musl-gcc failed to build the probe"
    sed 's/^/  /' "$TMP/cc.log"
    exit 2
fi
pass "musl-gcc built the probe"

WHICH="$("$PROBE" --which 2>&1)"
echo "$WHICH" | sed 's/^/  /'
if echo "$WHICH" | grep -qx "libc=musl"; then
    pass "the probe reports libc=musl (from its own __GLIBC__ check)"
else
    fail "the probe does not report libc=musl"
fi

# Parsed in python: no PT_INTERP means the ELF cannot be loading the host's libc.
python3 - "$PROBE" <<'PY'
import struct, sys
path = sys.argv[1]
data = open(path, "rb").read()
errors = []
if data[:4] != b"\x7fELF":
    errors.append("the probe is not an ELF file")
    elf_class = None
else:
    elf_class = data[4]
    if elf_class != 2:
        errors.append("the probe is not a 64-bit ELF")
    if data[5] != 1:
        errors.append("the probe is not little-endian")
    e_type = struct.unpack_from("<H", data, 16)[0]
    if e_type != 2:
        errors.append("the probe is not an executable (e_type=%d)" % e_type)
    e_phoff = struct.unpack_from("<Q", data, 32)[0]
    e_phentsize = struct.unpack_from("<H", data, 54)[0]
    e_phnum = struct.unpack_from("<H", data, 56)[0]
    types = []
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        p_type = struct.unpack_from("<I", data, off)[0]
        types.append(p_type)
    # PT_INTERP = 3. A statically linked binary has none, so it cannot be
    # dynamically linking the host C library.
    if 3 in types:
        errors.append("the probe has a PT_INTERP: it is dynamically linked")
    else:
        print("  [PASS] the probe has no PT_INTERP: statically linked")
if errors:
    for e in errors:
        print("  [FAIL] %s" % e)
    sys.exit(1)
PY
if [ $? -ne 0 ]; then FAIL=1; fi

PROBE_SHA="$(sha256sum "$PROBE" | cut -d' ' -f1)"
echo "  probe sha256: $PROBE_SHA"

# --- 2. Every sealed leaf: fresh run == committed, then validate -------------
for SYM in $SYMBOLS; do
    COUNT="$(expected_count "$SYM")"
    EVID="phost/evidence/cross/$SYM"
    FILE="cross_implementation_verdict.json"

    echo
    echo "--- $SYM ($COUNT cases) ---"

    if [ ! -f "$EVID/$FILE" ]; then
        fail "committed cross verdict missing: $EVID/$FILE"
        continue
    fi

    if ! "$PHOST" port cross "$SYM" --out "$TMP/$SYM" --probe "$PROBE" >"$TMP/$SYM.log" 2>&1; then
        fail "phost port cross $SYM failed"
        sed 's/^/  /' "$TMP/$SYM.log"
        continue
    fi

    # The *asserted* part must be reproducible; the observed part (the host library
    # version and the probe binary hash) legitimately varies by toolchain, so a whole
    # file comparison would be a lie. The python block below compares every asserted
    # field against this fresh run instead.
    python3 - "$EVID/$FILE" "$TMP/$SYM/$FILE" <<'PY'
import json, sys

try:
    committed = json.load(open(sys.argv[1]))
    fresh = json.load(open(sys.argv[2]))
except Exception as e:  # noqa: BLE001
    print("  [FAIL] cannot read a verdict: %s" % e)
    sys.exit(1)

if committed.get("asserted") == fresh.get("asserted"):
    print("  [PASS] fresh run matches the checked-in asserted claim")
    sys.exit(0)
print("  [FAIL] committed asserted claim differs from a fresh run")
for key in sorted(set(list(committed.get("asserted", {})) + list(fresh.get("asserted", {})))):
    if committed.get("asserted", {}).get(key) != fresh.get("asserted", {}).get(key):
        print("    %s: committed=%r fresh=%r" % (
            key, committed.get("asserted", {}).get(key), fresh.get("asserted", {}).get(key)))
sys.exit(1)
PY
    if [ $? -ne 0 ]; then FAIL=1; fi

    python3 - "$ROOT" "$EVID/$FILE" "$TMP/$SYM/$FILE" "$SYM" "$COUNT" "$PROBE_SRC" "$PROBE_SHA" <<'PY'
import hashlib, json, os, sys

(root, evid_rel, fresh_rel, symbol, expected, probe_src, probe_sha) = (
    sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]),
    sys.argv[6], sys.argv[7],
)
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
fresh = load(rel(fresh_rel), fresh_rel)

a = v.get("asserted", {})
obs = v.get("observed_not_asserted", {})

if v.get("schema") != "phorensic.porting.cross_implementation_verdict.v1":
    errors.append("cross schema != phorensic.porting.cross_implementation_verdict.v1")

# The asserted claim must be reproducible, so a fresh run must reproduce it exactly.
for key in sorted(set(list(a.keys()) + list(fresh.get("asserted", {}).keys()))):
    if a.get(key) != fresh.get("asserted", {}).get(key):
        errors.append("asserted field %s is not reproducible" % key)

# The environment-bound facts are recorded, not asserted, and must be marked so.
if "primary_version" not in obs or "probe_binary_hash" not in obs:
    errors.append("observed_not_asserted is missing an environment-bound fact")
for bound in ("primary_version", "probe_binary_hash"):
    if bound in a:
        errors.append("%s must not be an asserted field" % bound)

# The fresh run must have used the probe this verifier built.
if fresh.get("observed_not_asserted", {}).get("probe_binary_hash") != probe_sha:
    errors.append("the fresh run did not record the probe this verifier built")

target = a.get("target", "")
# The id's namespace is the dialect: a POSIX contract cannot seal as libc.
if a.get("dialect") != target.split(":", 1)[0]:
    errors.append("dialect does not match the target id's namespace")
if a.get("locale_contract") != "C":
    errors.append("locale_contract != C")

# Two named implementations, reached differently.
if a.get("primary") != "host-libc":
    errors.append("primary implementation is not host-libc")
if a.get("secondary") != "musl":
    errors.append("secondary implementation is not musl")
if "FFI" not in a.get("primary_mechanism", ""):
    errors.append("primary mechanism is not described as in-process FFI")
if "probe" not in a.get("secondary_mechanism", ""):
    errors.append("secondary mechanism is not described as the out-of-process probe")
if "libc=musl" not in a.get("secondary_identity", ""):
    errors.append("secondary_identity does not show the probe reporting musl")

# Agreement on the whole corpus.
if a.get("cases_run") != expected:
    errors.append("cases_run != %d" % expected)
if a.get("agreements") != expected:
    errors.append("agreements != %d" % expected)
if a.get("disagreements") != 0:
    errors.append("disagreements != 0 (the implementations did not agree)")
if v.get("mismatches"):
    errors.append("the verdict carries mismatches")
if a.get("verdict") != "consistent":
    errors.append("verdict != consistent")

# Bound to the seal: the primary hash must be the leaf's committed oracle hash.
sig = load(
    os.path.join(root, "phost", "evidence", "porting", symbol, "behavior_signature.json"),
    "%s behavior_signature.json" % symbol,
)
sealed_oracle = sig.get("combined_oracle_hash", "")
if not sealed_oracle:
    errors.append("the committed behavior signature has no oracle hash")
if a.get("primary_oracle_hash") != sealed_oracle:
    errors.append("primary_oracle_hash != the committed sealed oracle hash")

# Stronger than per-case agreement: the two implementations produced the *same*
# sealed trace set over the corpus.
if a.get("secondary_oracle_hash") != a.get("primary_oracle_hash"):
    errors.append("secondary_oracle_hash != primary_oracle_hash")

# The probe provenance: source hash reproducible, binary hash observed-only.
probe_src_sha = hashlib.sha256(open(rel(probe_src), "rb").read()).hexdigest()
if a.get("probe_source") != probe_src:
    errors.append("probe_source != %s" % probe_src)
if a.get("probe_source_hash") != probe_src_sha:
    errors.append("probe_source_hash does not match the committed probe source")
if len(obs.get("probe_binary_hash", "")) != 64:
    errors.append("probe_binary_hash is missing or not a SHA-256 digest")

# The residual covers exactly the asserted claim.
canonical = (
    "target=%s;dialect=%s;locale=%s;primary=%s;primary_mechanism=%s;secondary=%s;"
    "secondary_mechanism=%s;secondary_identity=%s;probe_source=%s;probe_source_hash=%s;"
    "cases_run=%s;agreements=%s;disagreements=%s;primary_oracle_hash=%s;"
    "secondary_oracle_hash=%s;verdict=%s"
) % (
    a.get("target", ""), a.get("dialect", ""), a.get("locale_contract", ""),
    a.get("primary", ""), a.get("primary_mechanism", ""), a.get("secondary", ""),
    a.get("secondary_mechanism", ""), a.get("secondary_identity", ""),
    a.get("probe_source", ""), a.get("probe_source_hash", ""), a.get("cases_run", ""),
    a.get("agreements", ""), a.get("disagreements", ""), a.get("primary_oracle_hash", ""),
    a.get("secondary_oracle_hash", ""), a.get("verdict", ""),
)
want = hashlib.sha256(canonical.encode()).hexdigest()
if v.get("residual_hash") != want:
    errors.append("residual_hash does not cover the asserted claim")

print("  Target:    %s" % target)
print("  Primary:   %s — %s" % (a.get("primary", ""), a.get("primary_mechanism", "")))
print("  Secondary: %s — %s" % (a.get("secondary", ""), a.get("secondary_identity", "")))
print("  Cases:     %d   agreements: %s   disagreements: %s" % (
    a.get("cases_run", 0), a.get("agreements", "?"), a.get("disagreements", "?")))
print("  Seal binding (primary oracle hash): %s" % (
    "MATCH" if a.get("primary_oracle_hash") == sealed_oracle else "MISMATCH"))
print("  Trace sets identical: %s" % (
    "yes" if a.get("secondary_oracle_hash") == a.get("primary_oracle_hash") else "no"))
print("  Observed (not asserted): %s" % obs.get("primary_version", "?"))

if errors:
    for e in errors:
        print("  [FAIL] %s" % e)
    sys.exit(1)
PY
    if [ $? -ne 0 ]; then FAIL=1; fi
done

# --- 3. No ambient authority -------------------------------------------------
echo
echo "--- no ambient authority ---"
if ! "$PHOST" port cross strspn --out "$TMP/nocap" --probe "$PROBE" --no-capability 2>&1 \
    | grep -q "capability denied"; then
    fail "the court did not refuse without the PORTING capability"
else
    pass "without PORTING the cross-implementation court refuses"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "Status: ALL CHECKS PASSED"
    exit 0
fi
echo "Status: SOME CHECKS FAILED"
exit 1
