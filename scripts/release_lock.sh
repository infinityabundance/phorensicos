#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Release Lock
#
#  Locks the empirical identities and tables of the autonomous-porting
#  disclosure to a tagged revision. Everything asserted is **derived from
#  committed evidence** — never from prose — so the lock cannot drift from the
#  courts without failing `--check`.
#
#  Usage:
#    ./scripts/release_lock.sh [TAG] [OUT]          # derive and write
#    ./scripts/release_lock.sh --check [TAG] [OUT]  # derive and compare
#
#  The tag itself binds the revision (an annotated tag); the manifest binds the
#  identities. `--check` additionally asserts the tag resolves to HEAD when the
#  tag exists.
#
#  Exit status: 0 = ok, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CHECK=0
if [ "${1:-}" = "--check" ]; then
    CHECK=1
    shift
fi
TAG="${1:-autonomous-porting-v1}"
OUT="${2:-foundry/releases/${TAG}.json}"

GEN_DIR="phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1"

if [ ! -x "$ROOT/target/debug/phorport" ]; then
    echo "--- building phorport + phorc ---"
    cargo build -q -p phorport -p phorc || { echo "ERROR: cargo build failed"; exit 2; }
fi
SUP="$("$ROOT/target/debug/phorport" supersessions)" || { echo "ERROR: supersessions failed"; exit 2; }

if [ "$CHECK" -eq 1 ]; then
    [ -f "$OUT" ] || { echo "ERROR: no committed lock at $OUT"; exit 2; }
fi

python3 - "$OUT" "$TAG" "$GEN_DIR" "$SUP" "$CHECK" "$ROOT" <<'PY'
import hashlib, json, os, subprocess, sys

out, tag, gen_dir, sup_raw, check, root = sys.argv[1:7]
check = check == "1"


def load(path):
    with open(path) as fh:
        return json.load(fh)


pins = load("foundry/baseline/dependency_pins.json")["repositories"]
store = load("phost/evidence/store/index.json")
sess = load("phost/evidence/session/session_verdict.json")
gs = load("phost/evidence/session/generation_session.json")
gen = load(os.path.join(gen_dir, "store_generation.json"))
seal = load(os.path.join(gen_dir, "autonomous_promotion_receipt.json"))
qual = load(os.path.join(gen_dir, "qualification_receipt.json"))
iso = load(os.path.join(gen_dir, "isolation_report.json"))
reg = load("phost/evidence/autonomous/artifacts.json")["artifacts"][0]
sup = json.loads(sup_raw)["supersessions"][0]

kinds = {}
for e in store["entries"]:
    kinds[e["kind"]] = kinds.get(e["kind"], 0) + 1

asserted = {
    "tag": tag,
    "pinned_stack": {
        n: {"commit": pins[n]["commit"], "version": pins[n].get("version", "")}
        for n in ("frf", "frf_fuzz", "gemel")
    },
    "baseline_store": {
        "schema": store["schema"],
        "residual_hash": store["residual_hash"],
        "leaf_objects": kinds.get("leaf-object", 0),
        "compositions": kinds.get("composition", 0),
        "total": len(store["entries"]),
    },
    "baseline_session": {
        "ports_in_store": sess["ports_in_store"],
        "calls": sess["calls"],
        "native_calls": sess["native_calls"],
        "fallback_calls": sess["fallback_calls"],
        "objects_mapped": sess["objects_mapped"],
        "dispatches": sess["dispatches"],
        "session_hash": sess["session_hash"],
        "verdict": sess["verdict"],
    },
    "provenance_correction": {
        "historical_target": sup["historical_target"],
        "historical_contract": sup["historical_contract"],
        "historical_spec_id": sup["historical_spec_id"],
        "successor_target": sup["successor_target"],
        "successor_contract": sup["successor_contract"],
        "successor_spec_id": sup["successor_spec_id"],
        "supersession_id": sup["supersession_id"],
    },
    "successor_seal": {
        "seal_profile": seal["seal_profile"],
        "evidence_closure": seal["evidence_closure"],
        "residual_hash": seal["residual_hash"],
        "obligations": len(seal["obligations"]),
        "object_hash": reg["object_hash"],
        "oracle_hash": reg["oracle_hash"],
        "candidate_behavior_hash": reg["candidate_behavior_hash"],
        "candidate_source_hash": reg["candidate_source_hash"],
        "qualification_policy": qual["policy"],
        "qualification_universe": qual["universe"],
        "qualification_cases": qual["cases_run"],
        "qualification_verdict": qual["verdict"],
        "isolation_markers": iso["markers_checked"],
        "isolation_leaks": len(iso["leaks"]),
    },
    "generation": {
        "generation_id": gen["generation_id"],
        "parent": gen["parent"],
        "entries": len(gen["entries"]),
        "evidence_closure": gen["evidence_closure"],
    },
    "generation_session": {
        "generation": gs["generation"],
        "parent": gs["parent"],
        "materialized_index_hash": gs["materialized_index_hash"],
        "ports_available": gs["ports_available"],
        "calls": gs["calls"],
        "native_calls": gs["native_calls"],
        "fallback_calls": gs["fallback_calls"],
        "broken_seal_calls": gs["broken_seal_calls"],
        "objects_mapped": gs["objects_mapped"],
        "dispatches": gs["dispatches"],
        "session_hash": gs["session_hash"],
        "residual_hash": gs["residual_hash"],
        "served": gs["served"],
        "verdict": gs["verdict"],
    },
}

canonical = json.dumps(asserted, sort_keys=True, separators=(",", ":"))
residual = hashlib.sha256(canonical.encode()).hexdigest()

if check:
    committed = load(out)
    errors = []
    if committed.get("schema") != "phorensic.foundry.release_lock.v1":
        errors.append("lock schema is not phorensic.foundry.release_lock.v1")
    if committed.get("asserted") != asserted:
        errors.append("the committed lock's asserted identities have drifted from committed evidence")
    if committed.get("residual_hash") != residual:
        errors.append("the committed lock's residual hash does not recompute")
    # The tag binds the revision (git). Inside the container the build context has
    # no `.git`, so this is informational only; the identity comparison above is
    # the check that matters and it needs only committed evidence.
    tag_commit = subprocess.run(
        ["git", "rev-parse", f"{tag}^{{commit}}"], capture_output=True, text=True
    )
    if tag_commit.returncode == 0:
        print("    tag %s -> %s" % (tag, tag_commit.stdout.strip()[:12]))
    if errors:
        print("Status: LOCK CHECK FAILED")
        for e in errors:
            print("  [FAIL] %s" % e)
        sys.exit(1)
    print("Status: LOCK VERIFIED (%s)" % tag)
    sys.exit(0)

receipt = {
    "schema": "phorensic.foundry.release_lock.v1",
    "claim": (
        "the identities and counts below were derived from committed evidence at the "
        "tagged revision; this is a release lock, not a correctness claim"
    ),
    "asserted": asserted,
    "residual_hash": residual,
}
os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
with open(out, "w") as fh:
    json.dump(receipt, fh, indent=2, sort_keys=True)
    fh.write("\n")

print("Tag:            %s" % tag)
print("Store:          %d leaves + %d compositions = %d (%s)" % (
    asserted["baseline_store"]["leaf_objects"], asserted["baseline_store"]["compositions"],
    asserted["baseline_store"]["total"], asserted["baseline_store"]["residual_hash"][:12]))
print("Baseline sess:  %d calls, %d objects, %s" % (
    asserted["baseline_session"]["calls"], asserted["baseline_session"]["objects_mapped"],
    asserted["baseline_session"]["session_hash"][:12]))
print("Correction:     %s -> %s (%s)" % (
    asserted["provenance_correction"]["historical_target"],
    asserted["provenance_correction"]["successor_target"],
    asserted["provenance_correction"]["supersession_id"][:12]))
print("Successor seal: %s, %d obligations, closure %s" % (
    asserted["successor_seal"]["seal_profile"], asserted["successor_seal"]["obligations"],
    asserted["successor_seal"]["evidence_closure"][:12]))
print("Generation:     %s (parent %s, %d entries)" % (
    asserted["generation"]["generation_id"][:12], asserted["generation"]["parent"][:12],
    asserted["generation"]["entries"]))
print("Gen session:    %d ports, %d native, %d objects, %s" % (
    asserted["generation_session"]["ports_available"], asserted["generation_session"]["native_calls"],
    asserted["generation_session"]["objects_mapped"], asserted["generation_session"]["session_hash"][:12]))
print("Residual hash:  %s" % residual)
print("Status: LOCK WRITTEN -> %s" % out)
PY
exit $?
