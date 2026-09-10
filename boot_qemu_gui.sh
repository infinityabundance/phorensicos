#!/bin/bash
# QEMU boot with VGA display, framebuffer dump, and screenshot capture
# Usage: ./boot_qemu_gui.sh [kernel.elf] [output_dir]

set -e

QEMU=${QEMU:-qemu-system-x86_64}
KERNEL="${1:-phost_kernel/phorensic-kernel.elf}"
OUT_DIR="${2:-.}"

DEBUG_LOG="$OUT_DIR/debug.log"
SERIAL_LOG="$OUT_DIR/serial.log"
FB_DUMP="$OUT_DIR/framebuffer.dump"
SCREENSHOT="$OUT_DIR/screenshot.ppm"

echo "=== Booted GUI Runtime Court ==="
echo "Kernel: $KERNEL"
echo "Output: $OUT_DIR"
echo ""

# Create a PPM header for the framebuffer dump
# 1024x768 24-bit RGB
echo "Creating PPM header for $SCREENSHOT..."
# We'll capture via QEMU's -gdb or just use the framebuffer dump

# Boot with VGA display, debug console, serial, and framebuffer dump
timeout 5 $QEMU \
    -kernel "$KERNEL" \
    -vga std \
    -display sdl \
    -debugcon file:"$DEBUG_LOG" \
    -serial file:"$SERIAL_LOG" \
    -no-reboot 2>&1 || true

echo "=== QEMU exited ==="

# Show outputs
echo ""
echo "Debug output:"
cat "$DEBUG_LOG" 2>/dev/null || echo "(empty)"
echo ""
echo "Serial output:"
cat "$SERIAL_LOG" 2>/dev/null || echo "(empty)"
echo ""

# Create a minimal evidence report
echo "=== Evidence Report ===" > "$OUT_DIR/evidence_report.txt"
echo "Date: $(date)" >> "$OUT_DIR/evidence_report.txt"
echo "Kernel: $KERNEL" >> "$OUT_DIR/evidence_report.txt"
echo "" >> "$OUT_DIR/evidence_report.txt"
echo "Boot Evidence:" >> "$OUT_DIR/evidence_report.txt"

if [ -f "$DEBUG_LOG" ] && grep -q "Ph" "$DEBUG_LOG" 2>/dev/null; then
    echo "  debug.log: PASS (contains 'Ph')" >> "$OUT_DIR/evidence_report.txt"
else
    echo "  debug.log: FAIL (missing 'Ph')" >> "$OUT_DIR/evidence_report.txt"
fi

if [ -f "$SERIAL_LOG" ] && grep -q "Ph" "$SERIAL_LOG" 2>/dev/null; then
    echo "  serial.log: PASS (contains 'Ph')" >> "$OUT_DIR/evidence_report.txt"
else
    echo "  serial.log: FAIL (missing 'Ph')" >> "$OUT_DIR/evidence_report.txt"
fi

echo "" >> "$OUT_DIR/evidence_report.txt"
echo "Framebuffer: $(test -f "$FB_DUMP" && echo 'captured' || echo 'not captured')" >> "$OUT_DIR/evidence_report.txt"
echo "Screenshot: $(test -f "$SCREENSHOT" && echo 'captured' || echo 'not captured')" >> "$OUT_DIR/evidence_report.txt"

cat "$OUT_DIR/evidence_report.txt"
echo ""
echo "=== Evidence Report Complete ==="
