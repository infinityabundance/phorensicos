#!/usr/bin/env bash
# Kernel CI — build the no_std Multiboot kernel, boot it in QEMU, verify the
# captured boot evidence, and check the boot evidence is byte-reproducible
# against the committed evidence manifest.
#
# Runs inside the `kernel` Docker service (docker/Dockerfile.kernel): the QEMU
# boot happens in the same minimal container.
#
# Note on reproducibility scope: the *boot evidence* (proof bytes, framebuffer
# ABI, screendump, LFB dump) is byte-identical across hosts and containers. The
# kernel *image bytes* additionally depend on the exact nasm/ld.lld/binutils
# versions, so they are recorded but not asserted equal across toolchains.
set -euo pipefail

# Make cargo available whether this runs in the rust image (non-login shell) or
# on a developer host.
export PATH="$HOME/.cargo/bin:/usr/local/cargo/bin:$PATH"

cd "$(dirname "$0")/.."

# Bare-metal target for the kernel staticlib (already added in the image).
rustup target add x86_64-unknown-none >/dev/null 2>&1 || true

cd phost_kernel

echo "=== build kernel ==="
./build_kernel.sh

echo
echo "=== boot in QEMU + capture evidence ==="
SECS="${QEMU_SECONDS:-8}"
./boot_qemu.sh phorensic-kernel.elf evidence "$SECS"

echo
echo "=== verify boot evidence ==="
./verify_evidence.sh evidence

echo
echo "=== boot-evidence reproducibility (vs committed manifest) ==="
FAIL=0
check_ev() {
    # $1 = manifest key, $2 = evidence file
    committed="$(jq -r ".evidence.$1.sha256" evidence_manifest.json)"
    actual="$(sha256sum "evidence/$2" | cut -d' ' -f1)"
    if [ "$committed" = "$actual" ]; then
        echo "  [PASS] $2 matches committed manifest"
    else
        echo "  [FAIL] $2 mismatch"
        echo "         committed: $committed"
        echo "         actual:    $actual"
        FAIL=1
    fi
}
check_ev serial_log serial.log
check_ev debug_log debug.log
check_ev screen_ppm screen.ppm
check_ev fb_abi_bin fb-abi.bin
check_ev lfb_bin lfb.bin
[ "$FAIL" -eq 0 ] || { echo "Status: EVIDENCE NOT REPRODUCIBLE"; exit 1; }
echo "  boot evidence is byte-reproducible: 5/5 artifacts"

echo
echo "=== kernel image (informational) ==="
echo "kernel sha256: $(sha256sum phorensic-kernel.elf | cut -d' ' -f1)"
echo "(image bytes depend on the linker toolchain; the evidence above is deterministic)"

# Refresh the manifest for this run (kept in-container; the committed manifest
# in the repo is authoritative and its image hash was recorded on the host).
./evidence_manifest.sh evidence >/dev/null
echo "evidence manifest refreshed: phost_kernel/evidence_manifest.json"
