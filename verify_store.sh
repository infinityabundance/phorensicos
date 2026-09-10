#!/bin/bash
# ============================================================================
#  Phorensic OS — Persistent Sealed Port Store Verifier
#
#  The courts *derive* a seal (observe → compile → replay → publish). The
#  committed store index (phost/evidence/store/index.json) records the result so
#  the runtime can *load* it instead: no compiler invocation and no oracle replay
#  at a call site.
#
#  This verifier proves the committed index is that artifact:
#    * the index exists, is valid JSON, and matches the store schema
#    * `phost port store` loads and verifies every entry (leaf objects hashed
#      against their seal, composition stages resolved) — it fails closed
#    * every leaf object file is committed, its SHA-256 equals the recorded
#      `object_hash`, and that equals the committed `candidate_signature.json`
#      object hash
#    * every committed leaf object is byte-identical to a fresh `phorc`
#      compilation of its `.phor` source (the object *is* the promoted artifact)
#    * every composition entry's `chain_hash` equals the committed
#      `composition_verdict.json` chain hash, and its `leaves` are all in the store
#    * the composition graph is acyclic (resolving a chain recurses through the
#      index, so a cycle is rejected as data)
#    * the index regenerates byte-identically from committed evidence (written to
#      a temp path; the committed file is never touched)
#    * the runtime path needs no compiler: the composition court reproduces its
#      committed verdict from the store with an impossible `--phorc` path, and
#      `phost port native` dispatches a sealed object
#    * a missing store fails closed (never a silent fallback)
#    * without PORTING the store reveals nothing (foreign fallback)
#
#  Usage:
#    ./verify_store.sh [store_index_path]
#
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
#
#  This verifier writes only to a temp directory; committed evidence is never
#  modified.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

STORE="phost/evidence/store/index.json"
if [ $# -gt 0 ]; then
    STORE="$1"
fi

PHOST="$ROOT/target/debug/phost"
PHORC="$ROOT/target/debug/phorc"

echo "=== Phorensic OS — Persistent Sealed Port Store Verification ==="
echo "Store:        $STORE"
echo

if [ ! -x "$PHOST" ] || [ ! -x "$PHORC" ]; then
    echo "--- building phost + phorc ---"
    if ! cargo build -q -p phost -p phorc; then
        echo "ERROR: cargo build failed"
        exit 2
    fi
fi

if [ ! -f "$STORE" ]; then
    echo "ERROR: committed store index missing: $STORE"
    exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAIL=0
pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; FAIL=1; }

# --- 1. The store loads and verifies -----------------------------------------
echo "--- store loads and verifies ---"
if "$PHOST" port store "$STORE" >"$TMP/store.txt" 2>&1; then
    pass "phost port store loaded and verified the index"
    grep -E "^(Entries|Residual|Status):" "$TMP/store.txt" | sed 's/^/  /'
else
    fail "phost port store rejected the index"
    sed 's/^/  /' "$TMP/store.txt"
fi

# --- 2. Regeneration is deterministic (temp only) ----------------------------
echo
echo "--- store regenerates deterministically from committed evidence ---"
if "$PHOST" port store --write "$TMP/index.json" >/dev/null 2>&1; then
    if cmp -s "$STORE" "$TMP/index.json"; then
        pass "a fresh index from committed evidence is byte-identical"
    else
        fail "regenerated index differs from the committed one"
        diff "$STORE" "$TMP/index.json" | head -20 | sed 's/^/  /'
    fi
else
    fail "phost port store --write failed"
fi

# --- 3. Every entry is verified against committed evidence -------------------
echo
echo "--- validating every entry against committed evidence ---"
python3 - "$ROOT" "$STORE" "$PHORC" "$TMP" <<'PY'
import hashlib, json, os, subprocess, sys

root, store_rel, phorc, tmp = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
errors = []


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def load(path, name):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read/parse %s: %s" % (name, e))
        return {}


def rel(p):
    return p if os.path.isabs(p) else os.path.join(root, p)


store_path = rel(store_rel)
doc = load(store_path, store_rel)

if doc.get("schema") != "phorensic.porting.store.v1":
    errors.append("store schema != phorensic.porting.store.v1")

entries = doc.get("entries", [])
if not entries:
    errors.append("store has no entries")

if doc.get("entry_count") != len(entries):
    errors.append("entry_count != number of entries")

if len(doc.get("residual_hash", "")) != 64:
    errors.append("store residual_hash is missing or not a SHA-256 digest")

targets = [e.get("target", "") for e in entries]
if len(set(targets)) != len(targets):
    errors.append("store has duplicate targets")

# The store must cover exactly the known port set.
EXPECTED_LEAVES = {
    "libc:toupper:c-locale:u8:v1": "toupper",
    "libc:memcmp:c-locale:sign:v1": "memcmp",
    "libc:memchr:c-locale:index:v1": "memchr",
    "libc:strlen:c-locale:u64:v1": "strlen",
    "libc:strrchr:c-locale:index:v1": "strrchr",
    "posix:strspn:c-locale:u64:v1": "strspn",
}
EXPECTED_COMPOSITIONS = {
    "toupper_memchr": "phor:compose:toupper_memchr:c-locale:index:v1",
    "toupper_strlen_memchr": "phor:compose:toupper_strlen_memchr:c-locale:index:v1",
    "toupper_strlen_memchr_pair": "phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1",
    "toupper_each": "phor:compose:toupper_each:c-locale:u8s:v1",
    "toupper_each_strlen_memchr": "phor:compose:toupper_each_strlen_memchr:c-locale:index:v1",
    "toupper_memchr_suffix": "phor:compose:toupper_memchr_suffix:c-locale:index:v1",
    "toupper_each_slice_search": "phor:compose:toupper_each_slice_search:c-locale:index:v1",
}

expected_targets = set(EXPECTED_LEAVES) | set(EXPECTED_COMPOSITIONS.values())
missing = expected_targets - set(targets)
extra = set(targets) - expected_targets
if missing:
    errors.append("store is missing ports: %s" % ", ".join(sorted(missing)))
if extra:
    errors.append("store has unexpected ports: %s" % ", ".join(sorted(extra)))

leaves = [e for e in entries if e.get("kind") == "leaf-object"]
compositions = [e for e in entries if e.get("kind") == "composition"]

# ---- leaves ----------------------------------------------------------------
recompiled = []
for entry in leaves:
    target = entry.get("target", "?")
    if entry.get("trust") != "sealed":
        errors.append("%s: store entry is not sealed" % target)

    symbol = EXPECTED_LEAVES.get(target)
    if symbol is None:
        errors.append("%s: not a known leaf target" % target)
        continue

    obj_rel = entry.get("object_path", "")
    obj_abs = rel(obj_rel)
    if not os.path.isfile(obj_abs):
        errors.append("%s: committed object missing (%s)" % (target, obj_rel))
        continue

    obj_sha = sha256(obj_abs)
    if obj_sha != entry.get("object_hash"):
        errors.append("%s: object bytes do not hash to the sealed object_hash" % target)

    # Cross-check against the committed candidate signature.
    sig = load(
        os.path.join(root, "phost", "evidence", "porting", symbol, "candidate_signature.json"),
        "%s candidate_signature.json" % symbol,
    )
    if sig.get("target") != target:
        errors.append("%s: committed candidate_signature target mismatch" % target)
    if sig.get("candidate_object_hash") != entry.get("object_hash"):
        errors.append("%s: store object_hash != committed candidate_object_hash" % target)
    if sig.get("candidate_behavior_hash") != entry.get("candidate_behavior_hash"):
        errors.append("%s: store candidate_behavior_hash != committed" % target)
    if sig.get("candidate_source_hash") != entry.get("candidate_source_hash"):
        errors.append("%s: store candidate_source_hash != committed" % target)

    pkg = load(
        os.path.join(root, "phost", "evidence", "porting", symbol, "sealed_package.json"),
        "%s sealed_package.json" % symbol,
    )
    if pkg.get("oracle_hash") != entry.get("oracle_hash"):
        errors.append("%s: store oracle_hash != committed sealed_package oracle_hash" % target)
    if pkg.get("court_verdict") != "consistent":
        errors.append("%s: committed sealed_package verdict is not consistent" % target)

    sealed_pkg_abs = rel(entry.get("sealed_package", ""))
    if not os.path.isfile(sealed_pkg_abs):
        errors.append("%s: sealed_package file missing" % target)

    # The committed object must be exactly what the compiler emits today.
    src_rel = "examples/jit_port_%s.phor" % symbol
    src_abs = os.path.join(root, src_rel)
    if not os.path.isfile(src_abs):
        errors.append("%s: candidate source missing (%s)" % (target, src_rel))
        continue
    if sha256(src_abs) != entry.get("candidate_source_hash"):
        errors.append("%s: committed source hash does not match the .phor source" % target)

    out_obj = os.path.join(tmp, "%s.o" % symbol)
    proc = subprocess.run(
        [phorc, src_rel, out_obj],
        cwd=root,
        capture_output=True,
    )
    if proc.returncode != 0:
        errors.append("%s: fresh phorc compilation failed" % target)
        continue
    fresh = sha256(out_obj)
    if fresh != entry.get("object_hash"):
        errors.append("%s: fresh compilation is not byte-identical to the sealed object" % target)
    else:
        recompiled.append((symbol, fresh))

# ---- compositions ----------------------------------------------------------
for entry in compositions:
    target = entry.get("target", "?")
    if entry.get("trust") != "sealed":
        errors.append("%s: store entry is not sealed" % target)
    if entry.get("composition_id") != target:
        errors.append("%s: composition_id != target" % target)
    if len(entry.get("chain_hash", "")) != 64:
        errors.append("%s: chain_hash is not a SHA-256 digest" % target)

    for leaf in entry.get("leaves", []):
        if leaf not in targets:
            errors.append("%s: stage %s is not in the store" % (target, leaf))

    name = None
    for candidate_name, candidate_id in EXPECTED_COMPOSITIONS.items():
        if candidate_id == target:
            name = candidate_name
    if name is None:
        errors.append("%s: not a known composition target" % target)
        continue

    verdict = load(
        os.path.join(root, "phost", "evidence", "composition", name, "composition_verdict.json"),
        "%s composition_verdict.json" % name,
    )
    if verdict.get("target") != target:
        errors.append("%s: committed verdict target mismatch" % target)
    if verdict.get("chain_hash") != entry.get("chain_hash"):
        errors.append("%s: store chain_hash != committed composition verdict" % target)
    if verdict.get("verdict") != "consistent":
        errors.append("%s: committed composition verdict is not consistent" % target)
    if entry.get("leaves") != verdict.get("stages"):
        errors.append("%s: store leaves != committed stages" % target)

# ---- report ----------------------------------------------------------------
print("Targets:        %d (%d leaf objects, %d compositions)" % (len(entries), len(leaves), len(compositions)))
print("Store residual: %s" % doc.get("residual_hash", "?"))
for symbol, digest in recompiled:
    print("%-9s object: fresh compile MATCH" % symbol)
for entry in compositions:
    print(
        "%-34s chain hash: %s"
        % (entry.get("target", "?"), "present" if len(entry.get("chain_hash", "")) == 64 else "MISSING")
    )

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: ALL CHECKS PASSED")
PY
if [ $? -ne 0 ]; then FAIL=1; fi

# --- 4. The runtime path loads the store and needs no compiler ---------------
echo
echo "--- runtime path: loaded seal, no compiler ---"

if "$PHOST" port native toupper 61 >"$TMP/native.txt" 2>&1 \
    && grep -q "Source:        sealed-object" "$TMP/native.txt" \
    && grep -q "Output:        41" "$TMP/native.txt"; then
    pass "port native toupper 61 dispatched the sealed object from the store"
else
    fail "port native toupper did not dispatch the sealed object"
    sed 's/^/  /' "$TMP/native.txt"
fi

if "$PHOST" port compose --target toupper_memchr --store --phorc /nonexistent/phorc \
    --out "$TMP/court" >/dev/null 2>&1 \
    && cmp -s "$TMP/court/composition_verdict.json" \
        phost/evidence/composition/toupper_memchr/composition_verdict.json; then
    pass "composition court reproduced its committed verdict from the store with no compiler"
else
    fail "store-backed composition court did not reproduce the committed verdict"
fi

if "$PHOST" port compose --target toupper_each_strlen_memchr --store --phorc /nonexistent/phorc \
    --out "$TMP/court_nested" >/dev/null 2>&1 \
    && cmp -s "$TMP/court_nested/composition_verdict.json" \
        phost/evidence/composition/toupper_each_strlen_memchr/composition_verdict.json; then
    pass "nested composition court reproduced its verdict from the store (inner chain included)"
else
    fail "store-backed nested composition court did not reproduce the committed verdict"
fi

# --- 5. Fail-closed and capability-gated behaviour ---------------------------
echo
echo "--- fail closed; no ambient authority ---"

if "$PHOST" port native toupper 61 --store phost/evidence/store/does-not-exist.json \
    >/dev/null 2>&1; then
    fail "a missing store did not fail closed"
else
    pass "a missing store fails closed (never a silent fallback)"
fi

if "$PHOST" port native toupper 61 --no-capability 2>&1 | grep -q "foreign-fallback"; then
    pass "without PORTING the store reveals nothing (foreign fallback)"
else
    fail "the store was readable without the PORTING capability"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "Status: ALL CHECKS PASSED"
    exit 0
fi
echo "Status: SOME CHECKS FAILED"
exit 1
