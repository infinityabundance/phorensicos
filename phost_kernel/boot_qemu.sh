#!/bin/bash
# ============================================================================
#  Phorensic OS — Booted GUI Runtime Court: QEMU evidence capture
#
#  Boots the flat Multiboot kernel image in QEMU and captures the full
#  evidence set for the "Booted GUI Runtime Court" milestone:
#
#    serial.log       — bytes written to COM1 (0x3F8) by the kernel
#    debug.log        — bytes written to the debug port (0xE9)
#    seabios.log      — SeaBIOS POST/debug log (0x402)
#    qemu.log         — QEMU -d guest_errors,cpu_reset (or $QEMU_D_LOG)
#    fw_cfg.txt       — QEMU monitor: info fw_cfg (kernel blob registered?)
#    monitor.txt      — full monitor transcript
#    mem-1m.bin       — guest memory at 0x100000 (kernel loaded?)
#    fb-abi.bin       — guest memory at 0x300000 (framebuffer ABI written?)
#    lfb.bin          — linear framebuffer dump (0xFD000000, 4 MiB)
#    screen.ppm       — QEMU screendump (what a real display would show)
#
#  Usage:
#    ./boot_qemu.sh [image] [outdir] [seconds]
#
#  Environment:
#    QEMU_BIN     override the qemu binary
#    QEMU_D_LOG   override -d log flags (default: guest_errors,cpu_reset)
#    QEMU_NO_REBOOT  set to 0 to allow reboot-on-fault (keeps the guest
#                    alive so post-mortem dumps can be taken)
# ============================================================================
set -u

cd "$(dirname "$0")"

IMAGE="${1:-phorensic-kernel.elf}"
OUT="${2:-evidence}"
SECS="${3:-8}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"
D_LOG="${QEMU_D_LOG:-guest_errors,cpu_reset}"
NO_REBOOT="${QEMU_NO_REBOOT:-1}"
MON_PORT=45454

[ -f "$IMAGE" ] || { echo "ERROR: image not found: $IMAGE"; exit 2; }
mkdir -p "$OUT"
rm -f "$OUT"/*.log "$OUT"/*.bin "$OUT"/*.ppm "$OUT"/fw_cfg.txt "$OUT"/monitor.txt

NO_REBOOT_ARG=""
[ "$NO_REBOOT" = "1" ] && NO_REBOOT_ARG="-no-reboot"

echo "=== Phorensic OS — Boot Evidence Capture ==="
echo "Image:     $IMAGE ($(wc -c < "$IMAGE") bytes)"
echo "QEMU:      $QEMU_BIN"
echo "Output:    $OUT"
echo "Runtime:   ${SECS}s"

"$QEMU_BIN" \
    -kernel "$IMAGE" \
    -machine pc \
    -m 64 \
    -vga std \
    -display none \
    $NO_REBOOT_ARG \
    -chardev file,path="$OUT/serial.log",id=ser \
    -serial chardev:ser \
    -chardev file,path="$OUT/debug.log",id=dbg \
    -device isa-debugcon,iobase=0xE9,chardev=dbg \
    -chardev file,path="$OUT/seabios.log",id=dbg2 \
    -device isa-debugcon,iobase=0x402,chardev=dbg2 \
    -d "$D_LOG" \
    -D "$OUT/qemu.log" \
    -monitor tcp:127.0.0.1:$MON_PORT,server,nowait &
QPID=$!

sleep "$SECS"

# Drive the monitor via /dev/tcp (container-local connection).
if exec 3<>/dev/tcp/127.0.0.1/$MON_PORT 2>/dev/null; then
    { cat <&3 > "$OUT/monitor.txt" & } ; CATPID=$!
    printf 'info fw_cfg\n' >&3; sleep 1
    # NOTE: filenames must be quoted. QEMU's HMP splits on whitespace and a
    # bare token starting with '/' is parsed as an expression (division),
    # which silently drops the dump for absolute output paths.
    printf 'pmemsave 0x100000 4096 "%s/mem-1m.bin"\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0x300000 64 "%s/fb-abi.bin"\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0xFD000000 4194304 "%s/lfb.bin"\n' "$OUT" >&3; sleep 2
    printf 'screendump "%s/screen.ppm"\n' "$OUT" >&3; sleep 2
    printf 'quit\n' >&3; sleep 1
    exec 3>&-
    wait "$CATPID" 2>/dev/null
else
    echo "WARN: monitor socket not reachable — QEMU likely exited early (see qemu.log)"
fi

wait "$QPID" 2>/dev/null
echo "=== done (see $OUT) ==="
