#!/bin/bash
# Phorensic OS — Kernel Build Script
# Produces a Multiboot v1-compatible x86-64 kernel ELF (32-bit ELFCLASS32 header)
# Usage: ./build_kernel.sh [source.phor] [output.elf]

set -e

PHORC=${PHORC:-cargo run --}
LD=${LD:-ld.lld}
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

INPUT="${1:-test_kernel.ph}"
OUTPUT="${2:-phorensic-kernel.elf}"

INPUT_BASENAME="$(basename "$INPUT" .phor)"
INPUT_BASENAME="$(basename "$INPUT_BASENAME" .ph)"
OBJ_FILE="${INPUT_BASENAME}.o"
RECEIPTS="${INPUT_BASENAME}.receipts.json"

echo "=== Phorensic OS Kernel Build ==="
echo "Source:  $INPUT"
echo "Output:  $OUTPUT"
echo ""

# Step 1: Compile with Phorc
echo "--- Compile ---"
$PHORC "$INPUT" "$OBJ_FILE" --emit-kernel --emit-receipts
echo ""

# Step 2: Link with LLD (force 32-bit ELF for Multiboot v1 compatibility)
echo "--- Link ---"
$LD -m elf_i386 -T "$SCRIPT_DIR/kernel.ld" -o "$OUTPUT" "$OBJ_FILE"
echo "Linked: $(ls -la "$OUTPUT" | awk '{print $5}') bytes"
echo ""

# Step 3: Verify ELF header
echo "--- Verify ---"
od -A x -t x1 -N 64 "$OUTPUT" | head -4
echo ""

echo "=== Build Complete ==="
echo "Outputs: $OUTPUT, $OBJ_FILE, $RECEIPTS"
