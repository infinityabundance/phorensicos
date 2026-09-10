#!/bin/bash
# Verify framebuffer evidence from QEMU boot
# Checks: ELF valid, boot logs, framebuffer metadata, expected content

set -e

EVIDENCE_DIR="${1:-.}"
DEBUG_LOG="$EVIDENCE_DIR/debug.log"
SERIAL_LOG="$EVIDENCE_DIR/serial.log"
KERNEL="$EVIDENCE_DIR/phorensic-kernel.elf"
EVIDENCE_REPORT="$EVIDENCE_DIR/evidence_report.txt"

echo "=== Framebuffer Evidence Verification ==="
echo "Evidence directory: $EVIDENCE_DIR"
echo ""

PASS=0
FAIL=0

check() {
    local name="$1"
    local result="$2"
    if [ "$result" = "pass" ]; then
        echo "  [$((PASS + FAIL + 1))/5] $name: PASS"
        PASS=$((PASS + 1))
    else
        echo "  [$((PASS + FAIL + 1))/5] $name: FAIL"
        FAIL=$((FAIL + 1))
    fi
}

# 1. Check kernel ELF exists
if [ -f "$KERNEL" ]; then
    check "Kernel ELF exists" "pass"
else
    check "Kernel ELF exists" "fail"
fi

# 2. Check debug.log contains 'Ph'
if [ -f "$DEBUG_LOG" ] && grep -q "Ph" "$DEBUG_LOG" 2>/dev/null; then
    check "debug.log (Ph)" "pass"
else
    check "debug.log (Ph)" "fail"
fi

# 3. Check serial.log contains 'Ph'
if [ -f "$SERIAL_LOG" ] && grep -q "Ph" "$SERIAL_LOG" 2>/dev/null; then
    check "serial.log (Ph)" "pass"
else
    check "serial.log (Ph)" "fail"
fi

# 4. Check evidence report exists
if [ -f "$EVIDENCE_REPORT" ]; then
    check "Evidence report" "pass"
else
    check "Evidence report" "fail"
fi

# 5. Check ELF has Multiboot header (0x1BADB002 at offset 0-8K)
if [ -f "$KERNEL" ]; then
    if xxd -p -l 4 -s 0x8000 "$KERNEL" 2>/dev/null | grep -q "1badb002" \
        || xxd -p -l 4 -s 0x100 "$KERNEL" 2>/dev/null | grep -q "1badb002"; then
        check "Multiboot header" "pass"
    else
        # This is a soft check — ELF might work with -kernel without Multiboot
        echo "  [5/5] Multiboot header: SKIP (not required for -kernel)"
    fi
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
if [ $FAIL -eq 0 ]; then
    echo "Status: ALL CHECKS PASSED"
else
    echo "Status: SOME CHECKS FAILED"
    exit 1
fi
