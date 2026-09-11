#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Integration Baseline Receipt
#
#  Phase 0 of the autonomous JIT-porting foundry. Produces a machine-readable
#  baseline receipt for the four-repository epistemic stack, and *derives*
#  the counts from executable courts rather than trusting prose:
#
#    * phorensicos / frf / frf-fuzz / gemel commit identities (pins)
#    * rustc / cargo / host triple
#    * the phorc compiler identity
#    * the phorc and phost test counts (the suites are actually run)
#    * the sealed leaf-object and composition counts + the store residual hash
#    * the sealed-native session identity
#    * the committed court-evidence roots
#
#  The receipt follows the repository's asserted/observed split: environment-
#  bound build bytes (the phorc binary hash, the rustc commit hash) are
#  recorded as *observed*, not asserted, and are excluded from the residual
#  hash, exactly as the boot manifest treats the kernel image.
#
#  No wall-clock value participates in the residual hash. `generated_at`
#  exists for humans only and is outside the hash.
#
#  Usage:
#    ./scripts/integration_baseline.sh [OUT_PATH]
#  Default OUT_PATH: foundry/baseline/integration_baseline_receipt.json
#
#  Optional environment: FRF_ROOT, FRF_FUZZ_ROOT, GEMEL_ROOT — when set, the
#  recorded pin commits are verified against those working copies, and a
#  mismatch fails closed.
#
#  Exit status: 0 = receipt written, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${1:-foundry/baseline/integration_baseline_receipt.json}"
PINS="foundry/baseline/dependency_pins.json"
PHOST="$ROOT/target/debug/phost"

echo "=== Phorensic OS — Integration Baseline Receipt ==="
echo "Pins: $PINS"
echo "Out:  $OUT"
echo

if [ ! -f "$PINS" ]; then
    echo "ERROR: missing pin record $PINS"
    exit 2
fi

echo "--- building phorc + phost ---"
if ! cargo build -q -p phorc -p phost; then
    echo "ERROR: cargo build failed"
    exit 2
fi

echo "--- running the phorc suite (counts derive from execution) ---"
PHORC_TEST="$(cargo test -q -p phorc 2>&1 | grep -E '^test result' | awk '{p+=$4; f+=$6; i+=$8} END {printf "%d %d %d", p, f, i}')"
echo "--- running the phost suite ---"
PHOST_TEST="$(cargo test -q -p phost --lib 2>&1 | grep -E '^test result' | awk '{p+=$4; f+=$6; i+=$8} END {printf "%d %d %d", p, f, i}')"

PHORC_VERSION="$(./target/debug/phorc --version 2>/dev/null | head -1)"
PHORC_SHA="$(sha256sum target/debug/phorc | cut -d' ' -f1)"
RUSTC_REL="$(rustc -Vv | sed -n 's/^release: //p')"
RUSTC_COMMIT="$(rustc -Vv | sed -n 's/^commit-hash: //p')"
CARGO_V="$(cargo -V | sed 's/^cargo //')"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
PHORENSICOS_COMMIT="$(git rev-parse HEAD 2>/dev/null || echo unknown)"

python3 - "$OUT" "$PINS" \
    "$PHORC_TEST" "$PHOST_TEST" "$PHORC_VERSION" "$PHORC_SHA" \
    "$RUSTC_REL" "$RUSTC_COMMIT" "$CARGO_V" "$HOST" "$PHORENSICOS_COMMIT" \
    "${FRF_ROOT:-}" "${FRF_FUZZ_ROOT:-}" "${GEMEL_ROOT:-}" <<'PY'
import hashlib, json, os, subprocess, sys

(out, pins_path, phorc_test, phost_test, phorc_version, phorc_sha,
 rustc_rel, rustc_commit, cargo_v, host, phorensicos_commit,
 frf_root, frf_fuzz_root, gemel_root) = sys.argv[1:15]

root = os.getcwd()
errors = []


def split_counts(s):
    p, f, i = (int(x) for x in s.split())
    return {"passed": p, "failed": f, "ignored": i}


def load(path):
    with open(path) as fh:
        return json.load(fh)


pins = load(pins_path)
repos = pins["repositories"]

# ---- optional: verify the external pins against working copies -------------
def verify_pin(name, path):
    if not path:
        return None
    try:
        got = subprocess.run(["git", "-C", path, "rev-parse", "HEAD"],
                             capture_output=True, text=True, check=True).stdout.strip()
    except Exception as e:  # noqa: BLE001
        errors.append("cannot read %s HEAD: %s" % (name, e))
        return None
    want = repos[name]["commit"]
    if got != want:
        errors.append("%s commit drift: pinned %s, working copy %s" % (name, want, got))
    return got


verified = {}
for name, path in (("frf", frf_root), ("frf_fuzz", frf_fuzz_root), ("gemel", gemel_root)):
    v = verify_pin(name, path)
    if v is not None:
        verified[name] = v

# ---- phorensicos-side measurements -----------------------------------------
store = load("phost/evidence/store/index.json")
session = load("phost/evidence/session/session_verdict.json")

kinds = {}
for e in store["entries"]:
    kinds[e.get("kind", "?")] = kinds.get(e.get("kind", "?"), 0) + 1

evidence_roots = []
for dirpath, _dirnames, _filenames in os.walk("phost/evidence"):
    rel = os.path.relpath(dirpath, ".")
    if rel.count(os.sep) >= 1 and rel != "phost/evidence":
        evidence_roots.append(rel)
evidence_roots = sorted(evidence_roots)

asserted = {
    "repositories": {
        name: {"commit": repos[name]["commit"], "version": repos[name].get("version", "")}
        for name in ("frf", "frf_fuzz", "gemel")
    },
    "toolchain": {"rustc_release": rustc_rel, "cargo": cargo_v, "host_triple": host},
    "phorc_version": phorc_version,
    "tests": {"phorc": split_counts(phorc_test), "phost": split_counts(phost_test)},
    "sealed_ports": {
        "store_schema": store.get("schema", ""),
        "store_residual_hash": store.get("residual_hash", ""),
        "leaf_objects": kinds.get("leaf-object", 0),
        "compositions": kinds.get("composition", 0),
        "total": len(store["entries"]),
    },
    "session": {
        "ports_in_store": session.get("ports_in_store"),
        "calls": session.get("calls"),
        "native_calls": session.get("native_calls"),
        "objects_mapped": session.get("objects_mapped"),
        "dispatches": session.get("dispatches"),
        "session_hash": session.get("session_hash", ""),
        "verdict": session.get("verdict", ""),
    },
    "evidence_roots": evidence_roots,
}

observed = {
    "phorensicos_commit": phorensicos_commit,
    "rustc_commit_hash": rustc_commit,
    "phorc_binary_sha256": phorc_sha,
}
if verified:
    observed["verified_working_copies"] = verified

# ---- structural assertions (fail closed) -----------------------------------
if asserted["tests"]["phorc"]["failed"] or asserted["tests"]["phost"]["failed"]:
    errors.append("a test suite reported failures")
if asserted["sealed_ports"]["leaf_objects"] < 1 or asserted["sealed_ports"]["compositions"] < 1:
    errors.append("the store is missing leaves or compositions")
if asserted["session"]["native_calls"] != asserted["session"]["calls"]:
    errors.append("a session call was not served by a sealed object")
if len(asserted["sealed_ports"]["store_residual_hash"]) != 64:
    errors.append("the store residual hash is not a SHA-256 digest")

canonical = json.dumps(asserted, sort_keys=True, separators=(",", ":"))
residual = hashlib.sha256(canonical.encode()).hexdigest()

receipt = {
    "schema": "phorensic.foundry.integration_baseline_receipt.v1",
    "claim": (
        "the four-repository epistemic stack is pinned to the recorded commits, and the "
        "recorded phorensicos leaf/composition/store/session counts were derived from the "
        "executable courts at the recorded toolchain; this is a baseline, not a correctness claim"
    ),
    "asserted": asserted,
    "observed_not_asserted": observed,
    "residual_hash": residual,
}

os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
with open(out, "w") as fh:
    json.dump(receipt, fh, indent=2, sort_keys=True)
    fh.write("\n")

print("Repositories:   " + ", ".join(
    "%s@%s" % (n, asserted["repositories"][n]["commit"][:12]) for n in asserted["repositories"]))
print("Toolchain:      rustc %s / cargo %s / %s" % (rustc_rel, cargo_v, host))
print("phorc:          %s (%s)" % (phorc_version, phorc_sha[:12]))
print("Tests:          phorc %s, phost %s" % (asserted["tests"]["phorc"], asserted["tests"]["phost"]))
print("Sealed ports:   %d leaves + %d compositions = %d" % (
    asserted["sealed_ports"]["leaf_objects"],
    asserted["sealed_ports"]["compositions"],
    asserted["sealed_ports"]["total"]))
print("Store residual: %s" % asserted["sealed_ports"]["store_residual_hash"])
print("Session:        %d calls, %d dispatches, %d objects, %s" % (
    asserted["session"]["calls"], asserted["session"]["dispatches"],
    asserted["session"]["objects_mapped"], asserted["session"]["session_hash"][:12]))
print("Evidence roots: %d" % len(evidence_roots))
print("Residual hash:  %s" % residual)

if errors:
    print("")
    for e in errors:
        print("  [FAIL] %s" % e)
    print("Status: SOME CHECKS FAILED")
    sys.exit(1)
print("Status: RECEIPT WRITTEN -> %s" % out)
PY
exit $?
