#!/bin/bash
# ============================================================================
#  Phorensic OS — Boot Evidence Verifier
#
#  Validates the evidence set produced by phost_kernel/boot_qemu.sh and fails
#  unless the "booted GUI" claim is actually supported by the artifacts:
#
#    serial.log   — kernel proof bytes on COM1 (0x3F8)
#    debug.log    — kernel boot trace on the debug port (0xE9)
#    fb-abi.bin   — compact framebuffer ABI written at 0x300000
#    screen.ppm   — QEMU screendump; must contain the boot-GUI palette
#    lfb.bin      — linear framebuffer dump; must contain rendered pixels
#
#  Usage:
#    ./verify_evidence.sh [evidence_dir]     # default: evidence
#
#  Exit status: 0 if every check passes, 1 if any check fails, 2 on setup error.
# ============================================================================
set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Default to this script's own evidence dir; an explicit argument is resolved
# against the caller's working directory (standard shell behaviour).
DIR="${1:-$SCRIPT_DIR/evidence}"
case "$DIR" in /*) ;; *) DIR="$PWD/$DIR" ;; esac

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); printf '  [PASS] %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  [FAIL] %s\n' "$1"; }

echo "=== Phorensic OS — Boot Evidence Verification ==="
echo "Evidence dir: $DIR"
echo

if [ ! -d "$DIR" ]; then
    echo "ERROR: evidence directory not found: $DIR"
    echo "Generate it with: ./boot_qemu.sh phorensic-kernel.elf $DIR 8"
    exit 2
fi

# --- 1. Required artifacts -------------------------------------------------
echo "--- Required artifacts ---"
for f in serial.log debug.log fb-abi.bin screen.ppm lfb.bin; do
    if [ -s "$DIR/$f" ]; then
        ok "$f present ($(wc -c < "$DIR/$f") bytes)"
    else
        bad "$f present"
    fi
done

# --- 2. Kernel proof bytes -------------------------------------------------
echo "--- Kernel proof bytes ---"
if grep -q 'Ph' "$DIR/serial.log" 2>/dev/null; then
    ok "serial.log contains 'Ph' proof bytes"
else
    bad "serial.log contains 'Ph' proof bytes"
fi
if grep -q 'Ph' "$DIR/debug.log" 2>/dev/null; then
    ok "debug.log contains 'Ph' proof bytes"
else
    bad "debug.log contains 'Ph' proof bytes"
fi
# Boot trace: the stub emits '2' (bss+ABI), kernel_main emits 8 9 A <hex> A B C.
if grep -q 'ABCPh' "$DIR/debug.log" 2>/dev/null; then
    ok "debug.log contains boot trace (A/B/C → Ph)"
else
    bad "debug.log contains boot trace (A/B/C → Ph)"
fi

# --- 3. Binary artifacts (ABI, screendump palette, LFB) --------------------
echo "--- Framebuffer / ABI content ---"
PY_OUT="$(python3 - "$DIR" <<'PY'
import struct, sys, os

d = sys.argv[1]

def emit(ok, name):
    print(("OK " if ok else "BAD ") + name)

# --- compact framebuffer ABI @ 0x300000: <QIIII = addr, w, h, pitch, bpp ---
try:
    abi = open(os.path.join(d, "fb-abi.bin"), "rb").read(64)
    addr, w, h, pitch, bpp = struct.unpack_from("<QIIII", abi, 0)
    emit(
        addr == 0xFD000000 and w == 1024 and h == 768 and pitch == 4096 and bpp == 32,
        "fb-abi.bin = 0x%X %dx%d pitch=%d bpp=%d (want 0xFD000000 1024x768 4096 32)"
        % (addr, w, h, pitch, bpp),
    )
except Exception as e:  # noqa: BLE001
    emit(False, "fb-abi.bin decode failed: %s" % e)

# --- screendump: valid P6 and carries the boot-GUI palette -----------------
try:
    raw = open(os.path.join(d, "screen.ppm"), "rb").read()
    parts = raw.split(b"\n", 3)
    assert parts[0] == b"P6", "not a P6 PPM"
    w, h = (int(x) for x in parts[1].split())
    px = parts[3]
    emit((w, h) == (1024, 768), "screen.ppm dimensions %dx%d (want 1024x768)" % (w, h))
    # Colors the boot surface must paint (see phost_kernel/src/lib.rs).
    want = {
        b"\x00\x88\xff": "top accent bar",
        b"\x15\x15\x1e": "status bar",
        b"\x00\xff\x66": "boot-phase [OK] rows",
    }
    found = set()
    n = w * h
    for i in range(0, n, 4):
        c = px[i * 3:i * 3 + 3]
        if c in want:
            found.add(c)
            if len(found) == len(want):
                break
    missing = [want[c] for c in want if c not in found]
    emit(not missing, "screen.ppm boot-GUI palette (%d/%d markers%s)"
         % (len(found), len(want), "" if not missing else ", missing: " + ", ".join(missing)))
except Exception as e:  # noqa: BLE001
    emit(False, "screen.ppm parse failed: %s" % e)

# --- linear framebuffer dump: right size and actually rendered -------------
try:
    lfb = open(os.path.join(d, "lfb.bin"), "rb").read()
    need = 1024 * 768 * 4
    emit(len(lfb) >= need, "lfb.bin size %d bytes (want >= %d)" % (len(lfb), need))
    colors = set()
    for i in range(0, need - 4, 4 * 64):
        colors.add(lfb[i:i + 3])
        if len(colors) > 2:
            break
    emit(len(colors) > 1, "lfb.bin contains rendered pixels (%d sampled colors)" % len(colors))
except Exception as e:  # noqa: BLE001
    emit(False, "lfb.bin read failed: %s" % e)
PY
)"
while IFS= read -r line; do
    case "$line" in
        OK\ *) ok "${line#OK }" ;;
        BAD\ *) bad "${line#BAD }" ;;
    esac
done <<< "$PY_OUT"

# --- Summary ---------------------------------------------------------------
echo
echo "Results: $PASS passed, $FAIL failed"
if [ "$FAIL" -eq 0 ]; then
    echo "Status: ALL CHECKS PASSED"
    exit 0
fi
echo "Status: SOME CHECKS FAILED"
exit 1
