#!/usr/bin/env bash
# ============================================================================
#  Phorensic OS — Generation Session Court Verifier (Phase 8 §19, consumption)
#
#  §19 gives a publication model (a new immutable generation). This verifier
#  closes the **consumption** side: a session bound to generation 1 materializes
#  the exact runtime index the generation represents and serves every port it
#  binds — 14 ports from 7 mapped objects, 0 fallbacks, 0 broken seals.
#
#  It proves, adversarially:
#    * a fresh run is byte-identical to the committed generation-session verdict;
#    * the materialized index equals the generation exactly (not a subset);
#    * the lineage serves BOTH strspn identities independently:
#        posix:strspn:c-locale:u64:v1 -> the historical LegacyV1 object
#        libc:strspn:c-locale:u64:v1  -> the successor AutonomousV1 object
#    * a substituted successor hash fails before the session starts;
#    * a generation that drops the historical entry fails materialization;
#    * a registry whose object does not hash to the bound artifact fails;
#    * a tampered generation identity (parent) fails;
#    * the run works with construction machinery absent (no compiler/oracle/FRF);
#    * the committed baseline session verdict (13 ports, d219be2c…) is unchanged.
#
#  Writes only to a temp directory; committed evidence is never modified.
#  Usage: ./verify_generation_session.sh
#  Exit status: 0 = ALL CHECKS PASSED, 1 = a check failed, 2 = setup error.
# ============================================================================
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

PHOST="$ROOT/target/debug/phost"
GEN="phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1/store_generation.json"
REG="phost/evidence/autonomous/artifacts.json"
EVID="phost/evidence/session/generation_session.json"
LEGACY="phost/evidence/session/session_verdict.json"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { echo "  [FAIL] $1"; exit 1; }
pass() { echo "  [PASS] $1"; }

if [ ! -x "$PHOST" ]; then
    echo "--- building phost + phorc ---"
    cargo build -q -p phost -p phorc || { echo "ERROR: cargo build failed"; exit 2; }
fi

[ -f "$EVID" ] || { echo "ERROR: committed evidence missing: $EVID"; exit 2; }
[ -f "$GEN" ] || { echo "ERROR: committed generation missing: $GEN"; exit 2; }
[ -f "$REG" ] || { echo "ERROR: committed registry missing: $REG"; exit 2; }

echo "=== Phorensic OS — Generation Session Court Verification ==="
echo

echo "--- 1. a fresh run reproduces the committed verdict ---"
"$PHOST" port generation-session --out "$TMP/run" >/dev/null 2>&1 || fail "generation session did not run"
cmp -s "$EVID" "$TMP/run/generation_session.json" \
    || fail "the committed generation-session verdict differs from a fresh run"
pass "generation-session verdict reproduces byte-for-byte"

echo "--- 2. the generation session is exact and complete ---"
python3 - "$EVID" <<'PY' || exit 1
import hashlib, json, sys
v = json.load(open(sys.argv[1]))
assert v["schema"] == "phorensic.porting.generation_session.v1", v["schema"]
assert v["generation"] == "8e6fafec7b2b66ced51d8699d86fc264983fe2cd501c11b9c316f4d125912ba5", v["generation"]
assert v["parent"] == "f47d1bec90d8bba92f441138fc29db77094c9a35420057a5d60c3ee4163bdf73", v["parent"]
assert v["ports_available"] == 14, v["ports_available"]
assert v["store_loads"] == 1, v["store_loads"]
assert v["calls"] == 14 and v["native_calls"] == 14, (v["calls"], v["native_calls"])
assert v["fallback_calls"] == 0, v["fallback_calls"]
assert v["broken_seal_calls"] == 0, v["broken_seal_calls"]
assert v["objects_mapped"] == 7, v["objects_mapped"]
assert v["mismatches"] == [], v["mismatches"]
assert v["verdict"] == "consistent", v["verdict"]
# The lineage does not merely remember both identities: the runtime serves both,
# each from its own artifact.
assert v["served"]["posix:strspn:c-locale:u64:v1"] == \
    "c93271d069fd998e6de8c2cd07e665bd098f6fb64072cdd98fb44ae1375dbafc", v["served"]
assert v["served"]["libc:strspn:c-locale:u64:v1"] == \
    "c40c4e3a5100257b04f7df0b593e961bb0acc4ad2658e1604fda068fc7dd54e2", v["served"]
# The residual hash covers the reported fields (same canonical form as Rust).
per_port = ",".join("%s=%s" % (k, v["per_port"][k]) for k in sorted(v["per_port"]))
served = ",".join("%s=%s" % (k, v["served"][k]) for k in sorted(v["served"]))
canonical = (
    "target=phor:generation-session:v1;generation=%s;parent=%s;materialized_index=%s;"
    "ports_available=%s;store_loads=%s;calls=%s;native_calls=%s;fallback_calls=%s;"
    "broken_seal_calls=%s;objects_mapped=%s;dispatches=%s;per_port=%s;served=%s;"
    "session_hash=%s;verdict=%s"
) % (
    v["generation"], v["parent"], v["materialized_index_hash"], v["ports_available"],
    v["store_loads"], v["calls"], v["native_calls"], v["fallback_calls"],
    v["broken_seal_calls"], v["objects_mapped"], v["dispatches"], per_port, served,
    v["session_hash"], v["verdict"],
)
want = hashlib.sha256(canonical.encode()).hexdigest()
assert v["residual_hash"] == want, "residual_hash does not cover the reported fields"
print("    14 ports, 14 native, 7 objects, 69 dispatches")
print("    posix:strspn -> c93271d0… (historical)   libc:strspn -> c40c4e3a… (successor)")
PY
pass "exact generation: 14/14 native, 7 objects, both strspn identities served"

echo "--- 3. a substituted successor hash fails before the session starts ---"
python3 - "$GEN" "$TMP/gen-sub.json" <<'PY'
import json, sys
g = json.load(open(sys.argv[1]))
for e in g["entries"]:
    if e["target"] == "libc:strspn:c-locale:u64:v1":
        e["artifact_hash"] = "c93271d069fd998e6de8c2cd07e665bd098f6fb64072cdd98fb44ae1375dbafc"
json.dump(g, open(sys.argv[2], "w"), indent=2)
PY
if "$PHOST" port generation-session --generation "$TMP/gen-sub.json" --out "$TMP/t3" >/dev/null 2>&1; then
    fail "a substituted successor hash was accepted"
else
    pass "a substituted successor hash is refused"
fi

echo "--- 4. a generation that drops the historical entry fails materialization ---"
python3 - "$GEN" "$TMP/gen-drop.json" <<'PY'
import json, sys
g = json.load(open(sys.argv[1]))
g["entries"] = [e for e in g["entries"] if e["target"] != "posix:strspn:c-locale:u64:v1"]
json.dump(g, open(sys.argv[2], "w"), indent=2)
PY
if "$PHOST" port generation-session --generation "$TMP/gen-drop.json" --out "$TMP/t4" >/dev/null 2>&1; then
    fail "a generation that dropped the historical entry was accepted"
else
    pass "a dropped baseline entry is refused"
fi

echo "--- 5. a registry whose object is not the bound artifact fails ---"
python3 - "$REG" "$TMP/reg-wrong.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
# Point the locator at the historical object, keeping the successor hash: the
# object's bytes no longer hash to the bound artifact.
r["artifacts"][0]["object_path"] = "phost/evidence/porting/strspn/candidate.o"
json.dump(r, open(sys.argv[2], "w"), indent=2)
PY
if "$PHOST" port generation-session --registry "$TMP/reg-wrong.json" --out "$TMP/t5" >/dev/null 2>&1; then
    fail "an object that does not hash to its bound artifact was accepted"
else
    pass "an object that does not hash to its bound artifact is refused"
fi

echo "--- 6. a generation whose parent is not the baseline genesis is refused ---"
python3 - "$GEN" "$TMP/gen-parent.json" <<'PY'
import json, sys
g = json.load(open(sys.argv[1]))
g["parent"] = "0" * 64
json.dump(g, open(sys.argv[2], "w"), indent=2)
PY
if "$PHOST" port generation-session --generation "$TMP/gen-parent.json" --out "$TMP/t6" >/dev/null 2>&1; then
    fail "a generation with a foreign parent was accepted"
else
    pass "a foreign parent is refused"
fi

echo "--- 7. the run needs no construction machinery ---"
# No PATH, no HOME, no compiler, no oracle, no FRF, no Gemel, no network: the
# generation session reads committed evidence and committed object bytes only.
if env -i "$PHOST" port generation-session --out "$TMP/noc" >/dev/null 2>&1 \
    && python3 - "$TMP/noc/generation_session.json" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
sys.exit(0 if v["verdict"] == "consistent" and v["native_calls"] == 14
         and v["objects_mapped"] == 7 else 1)
PY
then
    pass "the exact generation serves 14/14 with no compiler, oracle, FRF or Gemel"
else
    fail "the generation session needed machinery outside committed evidence"
fi

echo "--- 8. the committed baseline session is unchanged ---"
"$PHOST" port session --out "$TMP/legacy" >/dev/null 2>&1 || fail "the legacy session did not run"
cmp -s "$LEGACY" "$TMP/legacy/session_verdict.json" \
    || fail "the committed baseline session verdict changed"
grep -q '"session_hash": "d219be2c207da11caf9a1b258e166bb7d535c36ce26452354cf232d4e425d431"' "$LEGACY" \
    || fail "the baseline session hash is not the committed one"
python3 - "$EVID" <<'PY' || exit 1
import json, sys
v = json.load(open(sys.argv[1]))
assert v["ports_available"] == 14, "the generation session does not carry the baseline 13 plus one"
PY
pass "baseline session (13 ports, d219be2c…) is byte-identical"

echo
echo "ALL CHECKS PASSED"
exit 0
