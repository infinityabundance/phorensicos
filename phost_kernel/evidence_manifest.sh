#!/bin/bash
# ============================================================================
#  Phorensic OS — Boot Evidence Manifest
#
#  Emits a small, committable JSON manifest recording *what* was booted and
#  *what it proved*: hashes and sizes of the kernel image and each captured
#  artifact, the framebuffer ABI location, the exact commands used, and the
#  toolchain versions. The large raw dumps stay gitignored; the manifest is
#  the durable claim and can be regenerated from any evidence dir.
#
#  Usage:
#    ./evidence_manifest.sh [evidence_dir] [output_json] [--update-screenshot]
#
#    --update-screenshot   re-render ../assets/screen.png from the fresh
#                          evidence/screen.ppm before hashing it
#
#  Prerequisites: run ./build_kernel.sh and ./boot_qemu.sh first
#  (or generate a fresh evidence dir + manifest in one shot with:
#     ./boot_qemu.sh phorensic-kernel.elf evidence 8 && ./evidence_manifest.sh )
# ============================================================================
set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DIR=""
OUT="$SCRIPT_DIR/evidence_manifest.json"
UPDATE_SCREENSHOT=0

for arg in "$@"; do
    case "$arg" in
        --update-screenshot) UPDATE_SCREENSHOT=1 ;;
        *.json) OUT="$arg" ;;
        *) DIR="$arg" ;;
    esac
done
# Default to this script's own evidence dir; an explicit argument is resolved
# against the caller's working directory (standard shell behaviour).
[ -n "$DIR" ] || DIR="$SCRIPT_DIR/evidence"
case "$DIR" in /*) ;; *) DIR="$PWD/$DIR" ;; esac

[ -d "$DIR" ] || {
    echo "ERROR: evidence directory not found: $DIR"
    echo "Run: ./boot_qemu.sh phorensic-kernel.elf evidence 8"
    exit 2
}

sha() {
    if [ -f "$1" ]; then
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$1" | cut -d' ' -f1
        else
            shasum -a 256 "$1" | cut -d' ' -f1
        fi
    else
        echo ""
    fi
}
bytes() { [ -f "$1" ] && wc -c < "$1" | tr -d ' ' || echo 0; }
tool() { command -v "$1" >/dev/null 2>&1 && ("$@" 2>&1 | head -1) || echo "unavailable"; }

KERNEL="$SCRIPT_DIR/phorensic-kernel.elf"
SHOT="$ROOT/assets/screen.png"
PPM="$DIR/screen.ppm"

# Optionally refresh the committed screenshot so it matches this boot.
if [ "$UPDATE_SCREENSHOT" = "1" ] && [ -s "$PPM" ]; then
    if command -v magick >/dev/null 2>&1; then
        magick "$PPM" -strip PNG24:"$SHOT"
    elif command -v convert >/dev/null 2>&1; then
        convert "$PPM" -strip "$SHOT"
    else
        echo "WARN: no ImageMagick; leaving existing $SHOT" >&2
    fi
fi

DIMS="unknown"
if [ -s "$PPM" ]; then
    DIMS="$(awk 'NR==2{print $1"x"$2; exit}' "$PPM")"
fi

jq -n \
    --arg generated "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg kernel_hash "$(sha "$KERNEL")" --argjson kernel_size "$(bytes "$KERNEL")" \
    --arg serial_hash "$(sha "$DIR/serial.log")" --argjson serial_size "$(bytes "$DIR/serial.log")" \
    --arg debug_hash "$(sha "$DIR/debug.log")" --argjson debug_size "$(bytes "$DIR/debug.log")" \
    --arg ppm_hash "$(sha "$PPM")" --argjson ppm_size "$(bytes "$PPM")" \
    --arg abi_hash "$(sha "$DIR/fb-abi.bin")" --argjson abi_size "$(bytes "$DIR/fb-abi.bin")" \
    --arg lfb_hash "$(sha "$DIR/lfb.bin")" --argjson lfb_size "$(bytes "$DIR/lfb.bin")" \
    --arg shot_hash "$(sha "$SHOT")" --argjson shot_size "$(bytes "$SHOT")" \
    --arg dims "$DIMS" \
    --arg qemu "$(tool qemu-system-x86_64 --version)" \
    --arg nasm "$(tool nasm -v)" \
    --arg lld "$(tool ld.lld --version)" \
    --arg rustc "$(tool rustc --version)" \
    --arg cargo "$(tool cargo --version)" \
    '{
      generated_utc: $generated,
      kernel: {
        image: "phorensic-kernel.elf",
        sha256: $kernel_hash,
        size_bytes: $kernel_size,
        image_kind: "Multiboot v1 AOUT flat image (32-bit header, 64-bit entry)",
        load_addr: "0x100000",
        multiboot_magic: "0x1BADB002"
      },
      boot_abi: {
        address: "0x300000",
        fields: ["fb_addr:u64", "width:u32", "height:u32", "pitch:u32", "bpp:u32"],
        expected: { fb_addr: "0xFD000000", width: 1024, height: 768, pitch: 4096, bpp: 32 }
      },
      evidence: {
        dir: "evidence",
        serial_log:  { sha256: $serial_hash, size_bytes: $serial_size },
        debug_log:   { sha256: $debug_hash,  size_bytes: $debug_size },
        screen_ppm:  { sha256: $ppm_hash,    size_bytes: $ppm_size, dimensions: $dims },
        fb_abi_bin:  { sha256: $abi_hash,    size_bytes: $abi_size },
        lfb_bin:     { sha256: $lfb_hash,    size_bytes: $lfb_size }
      },
      screenshot: {
        path: "../assets/screen.png",
        sha256: $shot_hash,
        size_bytes: $shot_size,
        dimensions: $dims,
        source: "QEMU screendump (boot_qemu.sh screendump → PNG)"
      },
      commands: {
        build:  "./build_kernel.sh",
        boot:   "./boot_qemu.sh phorensic-kernel.elf evidence 8",
        verify: "./verify_evidence.sh evidence"
      },
      toolchain: {
        qemu: $qemu, nasm: $nasm, "ld.lld": $lld, rustc: $rustc, cargo: $cargo
      }
    }' > "$OUT"

echo "Wrote evidence manifest: $OUT"
echo "  kernel sha256:     $(sha "$KERNEL")"
echo "  screenshot sha256: $(sha "$SHOT")"
echo "  dimensions:        $DIMS"
