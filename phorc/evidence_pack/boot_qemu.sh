#!/bin/bash
# Phorensic OS — QEMU Boot Script
# Boots kernel with debug port, serial, and VGA output capture
# Usage: ./boot_qemu.sh [kernel.elf] [output_dir]
# Run from the evidence_pack/ directory for best results.

set -e

QEMU=${QEMU:-qemu-system-x86_64}
KERNEL="${1:-phorensic-kernel.elf}"
OUT_DIR="${2:-.}"

# Use relative paths when possible (works cross-platform).
# On MSYS2/Windows, MSYS converts Unix paths for native Windows tools,
# but QEMU's -serial file: flag expects Windows-native paths.
# Using relative paths avoids the conversion issue entirely.
KERNEL_ARG="$KERNEL"

if [ "$OUT_DIR" != "." ]; then
    DEBUG_LOG="$OUT_DIR/debug.log"
    SERIAL_LOG="$OUT_DIR/serial.log"
else
    DEBUG_LOG="debug.log"
    SERIAL_LOG="serial.log"
fi

echo "=== Booting Phorensic Kernel in QEMU ==="
echo "Kernel: $KERNEL_ARGS"
echo "Cwd:    $(pwd)"
echo ""

# Boot with debug console (port 0xE9), serial (COM1 0x3F8), 5-second timeout
timeout 5 $QEMU \
    -kernel "$KERNEL" \
    -debugcon file:"$DEBUG_LOG" \
    -serial file:"$SERIAL_LOG" \
    -display none \
    -no-reboot 2>&1 || true

echo "=== QEMU exited ==="
echo "Debug output:"
cat "$DEBUG_LOG" 2>/dev/null || echo "(empty)"
echo ""
echo "Serial output:"
cat "$SERIAL_LOG" 2>/dev/null || echo "(empty)"
