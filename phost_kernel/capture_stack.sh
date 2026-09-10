#!/bin/bash
# Debug capture: boot kernel, trace exceptions (-d int), dump stack region.
set -u
cd "$(dirname "$0")"

IMAGE="${1:-phorensic-kernel.elf}"
OUT="${2:-evidence-dbg}"
SECS="${3:-6}"
MON_PORT=45456
mkdir -p "$OUT"
rm -f "$OUT"/*

qemu-system-x86_64 \
    -kernel "$IMAGE" \
    -machine pc -m 64 -vga std \
    -display none -no-reboot \
    -chardev file,path="$OUT/serial.log",id=ser \
    -serial chardev:ser \
    -chardev file,path="$OUT/debug.log",id=dbg \
    -device isa-debugcon,iobase=0xE9,chardev=dbg \
    -d int,cpu_reset \
    -D "$OUT/qemu-int.log" \
    -monitor tcp:127.0.0.1:$MON_PORT,server,nowait &
QPID=$!

sleep "$SECS"

if exec 3<>/dev/tcp/127.0.0.1/$MON_PORT 2>/dev/null; then
    { cat <&3 > "$OUT/monitor.txt" & } ; CATPID=$!
    printf 'pmemsave 0xF0000 0x20000 %s/stack.bin\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0x150000 0x20000 %s/bss.bin\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0x100000 0x60000 %s/image.bin\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0x1000 0x7000 %s/pagetables.bin\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0x80000 0x12000 %s/stack2.bin\n' "$OUT" >&3; sleep 1
    printf 'pmemsave 0xFD000000 0x400000 %s/lfb.bin\n' "$OUT" >&3; sleep 2
    printf 'screendump %s/screen.ppm\n' "$OUT" >&3; sleep 1
    printf 'quit\n' >&3; sleep 1
    exec 3>&-
    wait "$CATPID" 2>/dev/null
fi
wait "$QPID" 2>/dev/null
echo "=== done (see $OUT) ==="
