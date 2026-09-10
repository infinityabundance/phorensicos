#!/bin/bash
# Phorensic OS — Evidence Pack Verification Script
# Fails if any expected file/proof is missing or invalid
# Usage: ./verify_evidence.sh [evidence_dir]

set -e

DIR="${1:-evidence_pack}"
FAIL=0

echo "=== Verifying Evidence Pack: $DIR ==="
echo ""

check_file() {
    local file="$1"
    local desc="$2"
    if [ -f "$DIR/$file" ]; then
        local size=$(stat -c%s "$DIR/$file" 2>/dev/null || stat -f%z "$DIR/$file" 2>/dev/null || echo "?")
        echo "  ✅ $file ($size bytes) — $desc"
    else
        echo "  ❌ MISSING: $file — $desc"
        FAIL=1
    fi
}

check_contains() {
    local file="$1"
    local pattern="$2"
    local desc="$3"
    if grep -q "$pattern" "$DIR/$file" 2>/dev/null; then
        echo "  ✅ $file contains '$pattern' — $desc"
    else
        echo "  ❌ $file missing '$pattern' — $desc"
        FAIL=1
    fi
}

# === Required files ===
echo "--- Required files ---"
check_file "phorensic-kernel.elf" "Linked Multiboot v1 kernel ELF"
check_file "kernel.receipts.json" "Compiler receipts with byte attribution"
check_file "debug.log" "QEMU debug port (0xE9) output"
check_file "serial.log" "QEMU serial port (0x3F8) output"
check_file "test_kernel.ph" "Phorensic kernel source"
check_file "build_kernel.sh" "Repeatable build script"
check_file "boot_qemu.sh" "Repeatable boot script"
check_file "kernel.ld" "Linker script"
check_file "README.md" "Evidence pack documentation"
echo ""

# === ELF header verification ===
echo "--- ELF header ---"
check_contains "readelf-h.txt" "7f 45 4c 46 01" "ELFCLASS32 magic"
check_contains "readelf-h.txt" "02 00 03 00" "EM_386 (Intel 80386)"
echo ""

# === Multiboot header ===
echo "--- Multiboot v1 header ---"
check_contains "readelf-l.txt" "02 b0 ad 1b" "Multiboot magic 0x1BADB002"
check_contains "readelf-l.txt" "03 00 00 00" "Multiboot flags (page-align + memory info)"
echo ""

# === Entry point ===
echo "--- Entry point verification ---"
check_contains "objdump-dr.txt" "89 df" "32-bit entry: mov edi, ebx (save boot info)"
check_contains "objdump-dr.txt" "0f 22 d8" "mov cr3, eax (load page table)"
check_contains "objdump-dr.txt" "0f 30"     "wrmsr (enable long mode)"
check_contains "objdump-dr.txt" "48 c7 c4"  "mov rsp, imm32 (set stack pointer)"
check_contains "objdump-dr.txt" "e6 e9"     "out 0xE9, al (debug port write)"
check_contains "objdump-dr.txt" "ee"        "out dx, al (serial port write)"
check_contains "objdump-dr.txt" "fa"        "cli (disable interrupts)"
check_contains "objdump-dr.txt" "f4"        "hlt (halt CPU)"
echo ""

# === QEMU boot output ===
echo "--- Boot output ---"
check_contains "debug.log" "Ph" "Kernel writes 'Ph' to debug port 0xE9"
check_contains "serial.log" "Ph" "Kernel writes 'Ph' to serial port 0x3F8"
echo ""

# === Receipts ===
echo "--- Receipts ---"
check_contains "kernel.receipts.json" "_start" "Boot entry receipt present"
check_contains "kernel.receipts.json" "kernel_entry" "Kernel function receipt present"
check_contains "kernel.receipts.json" "boot_entry_vma" "Boot entry VMA recorded"
check_contains "kernel.receipts.json" "residuals" "Residual records present"
echo ""

# === Scripts ===
echo "--- Scripts ---"
if grep -q "elf_i386" "$DIR/build_kernel.sh"; then
    echo "  ✅ build_kernel.sh uses elf_i386 for Multiboot compat"
else
    echo "  ❌ build_kernel.sh does not specify elf_i386"
    FAIL=1
fi
echo ""

# === Final verdict ===
echo "=== Verdict ==="
if [ "$FAIL" -eq 0 ]; then
    echo "✅ ALL CHECKS PASSED — Evidence pack is complete and valid"
else
    echo "❌ SOME CHECKS FAILED — Review issues above"
fi
exit $FAIL
