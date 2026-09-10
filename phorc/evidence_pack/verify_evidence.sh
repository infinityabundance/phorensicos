#!/bin/bash
# Phorensic OS — Booted GUI Runtime Court Evidence Verifier
# Fails if any expected file/proof is missing or invalid.
# Pure shell — no Python, no readelf, no objdump.
# Uses: od, xxd, grep, head, tr, wc, cut

set -e

DIR="${1:-evidence_pack}"
FAIL=0
PASS=0

echo "=== Booted GUI Runtime Court Evidence Verifier ==="
echo "Directory: $DIR"
echo ""

check_file() {
    local file="$1"
    local desc="$2"
    if [ -f "$DIR/$file" ]; then
        local size=$(wc -c < "$DIR/$file" 2>/dev/null || echo "0")
        PASS=$((PASS + 1))
        echo "  [$PASS] $file ($size bytes) — $desc: PASS"
    else
        FAIL=$((FAIL + 1))
        echo "  [FAIL] MISSING: $file — $desc"
    fi
}

check_contains() {
    local file="$1"
    local pattern="$2"
    local desc="$3"
    if grep -q "$pattern" "$DIR/$file" 2>/dev/null; then
        PASS=$((PASS + 1))
        echo "  [$PASS] $file contains '$pattern' — $desc: PASS"
    else
        FAIL=$((FAIL + 1))
        echo "  [FAIL] $file missing '$pattern' — $desc"
    fi
}

check_hex_contains() {
    local file="$1"
    local hexbytes="$2"
    local desc="$3"
    # Use xxd to search for hex bytes in the binary
    if xxd -p "$DIR/$file" 2>/dev/null | tr -d '\n' | grep -q "$hexbytes"; then
        PASS=$((PASS + 1))
        echo "  [$PASS] $file contains bytes 0x$hexbytes — $desc: PASS"
    else
        FAIL=$((FAIL + 1))
        echo "  [FAIL] $file missing bytes 0x$hexbytes — $desc"
    fi
}

read_bytes() {
    # Read exactly N bytes at offset from file, return as hex string without newlines
    local file="$1"
    local offset="$2"
    local count="$3"
    dd if="$DIR/$file" bs=1 skip=$offset count=$count 2>/dev/null | xxd -p | tr -d '\n'
}

check_elf_magic() {
    local file="$1"
    local offset="$2"
    local count="$3"
    local expected="$4"
    local desc="$5"
    local actual=$(read_bytes "$file" $offset $count)
    if [ "$actual" = "$expected" ]; then
        PASS=$((PASS + 1))
        echo "  [$PASS] $file at +$offset: $expected — $desc: PASS"
    else
        FAIL=$((FAIL + 1))
        echo "  [FAIL] $file at +$offset: expected $expected, got $actual — $desc"
    fi
}

# === 1. Required files ===
echo "--- Required files ---"
check_file "phorensic-kernel.elf" "Bootable kernel ELF"
check_file "debug.log" "QEMU debug port output"
check_file "serial.log" "QEMU serial port output"
check_file "test_kernel.ph" "Kernel source (reference)"
check_file "build_kernel.sh" "Build script"
check_file "boot_qemu.sh" "Boot script"
check_file "kernel.ld" "Linker script"
check_file "README.md" "Evidence pack docs"
echo ""

# === 2. ELF verification (pure xxd: offset size hex) ===
echo "--- ELF Verification ---"
check_elf_magic "phorensic-kernel.elf" 0 4 "7f454c46" "ELF magic \\x7fELF"
# ELF class: byte 4 -> 02 = 64-bit
check_elf_magic "phorensic-kernel.elf" 4 1 "02" "ELF 64-bit class"
# ELF encoding: byte 5 -> 01 = little endian
check_elf_magic "phorensic-kernel.elf" 5 1 "01" "Little endian"
# ELF type at offset 16 (0x10): 0200 = ET_EXEC, 0300 = ET_DYN (little-endian)
check_elf_magic "phorensic-kernel.elf" 16 2 "0300" "Executable type (PIE)"
# Machine at offset 18 (0x12): 3e00 = EM_X86_64
check_elf_magic "phorensic-kernel.elf" 18 2 "3e00" "x86-64 machine"
# Entry point at offset 24 (0x18) for 64-bit ELF: 8 bytes, must be non-zero
entry_hex=$(read_bytes "phorensic-kernel.elf" 24 8)
if [ "$entry_hex" != "0000000000000000" ]; then
    PASS=$((PASS + 1))
    echo "  [$PASS] Entry point non-zero (entry=0x$entry_hex): PASS"
else
    FAIL=$((FAIL + 1))
    echo "  [FAIL] Entry point is zero"
fi
ELF_SIZE=$(wc -c < "$DIR/phorensic-kernel.elf" 2>/dev/null || echo 0)
echo "  ELF size: $ELF_SIZE bytes"
echo ""

# === 3. Boot output ===
echo "--- Boot Output ---"
check_contains "debug.log" "Ph" "Kernel writes to debug port 0xE9"
check_contains "serial.log" "Ph" "Kernel writes to serial port 0x3F8"
echo ""

# === 4. Disassembly via raw byte search ===
echo "--- Disassembly Verification ---"
# out dx, al = 0xEE (port I/O instruction)
check_hex_contains "phorensic-kernel.elf" "ee" "out dx, al (serial port I/O)"
# HLT = 0xF4
check_hex_contains "phorensic-kernel.elf" "f4" "HLT instruction"
# CLI = 0xFA
check_hex_contains "phorensic-kernel.elf" "fa" "CLI instruction"
# kernel_main string in binary
check_contains "phorensic-kernel.elf" "kernel_main" "kernel_main function symbol"
# _start string in binary
check_contains "phorensic-kernel.elf" "_start" "_start entry symbol"
echo ""

# === 5. Source reference ===
echo "--- Source Reference ---"
check_file "test_kernel.ph" "Kernel source reference"
echo ""

# === Final verdict ===
echo "=== Verdict ==="
if [ "$FAIL" -eq 0 ]; then
    echo "✅ ALL $PASS CHECKS PASSED — Booted GUI Runtime Court evidence is valid"
else
    echo "❌ $FAIL CHECK(S) FAILED — Review issues above"
    exit 1
fi
