#!/bin/bash
# Phost Kernel — Build Script
#
# Stages:
#   1. cargo build --release   → libphost_kernel.a (staticlib: kernel+phost+deps)
#   2. nasm -f elf64           → multiboot_entry.o (entry stub with AOUT header)
#   3. ld.lld -m elf_x86_64    → phorensic-kernel.elf (non-PIE at 1 MiB)
#   4. objcopy -O binary       → flat image (Multiboot AOUT raw kernel)
#
# QEMU -kernel: the flat image's first 8KB carries the Multiboot v1 AOUT
# header, so QEMU loads it raw at 0x100000 and jumps to the entry (which
# sets Bochs VBE + long mode, then calls the Rust kernel_main).
#
# Usage: ./build_kernel.sh [output.image]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

PHORC_DIR="$(dirname "$SCRIPT_DIR")"
# Absolute host path mounted into the container for the Docker fallback.
# Resolved from SCRIPT_DIR so the script is not tied to any one machine.
HOST_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_TRIPLE="x86_64-unknown-none"
RELEASE_DIR="$PHORC_DIR/target/$TARGET_TRIPLE/release"
OUTPUT="${1:-$SCRIPT_DIR/phorensic-kernel.elf}"
ELF_OUT="$SCRIPT_DIR/phorensic-kernel.elf.elf"
STUB_OBJ="$SCRIPT_DIR/multiboot_entry.o"

HAS_NASM=$(command -v nasm >/dev/null 2>&1 && echo yes || echo no)
HAS_LLD=$(command -v ld.lld >/dev/null 2>&1 && echo yes || echo no)
HAS_OBJCOPY=$(command -v objcopy >/dev/null 2>&1 && echo yes || echo no)
HAS_DOCKER=$(command -v docker >/dev/null 2>&1 && echo yes || echo no)

echo "=== Building Phorensic OS Booted GUI Kernel ==="
echo ""

# --- Stage 1: staticlib (host) ---
echo "--- Stage 1: Build libphost_kernel.a (staticlib) ---"
cargo build --release --target "$TARGET_TRIPLE" 2>&1 | tail -2 || true
RUST_LIB=$(ls -t "$RELEASE_DIR/libphost_kernel.a" 2>/dev/null | head -1)
if [ -z "$RUST_LIB" ] || [ ! -f "$RUST_LIB" ]; then
    echo "ERROR: libphost_kernel.a not found under $RELEASE_DIR"
    ls "$RELEASE_DIR" 2>/dev/null | grep -i phost | head
    exit 1
fi
echo "Staticlib: $RUST_LIB ($(wc -c < "$RUST_LIB") bytes)"
echo ""

# --- Stage 2: nasm stub ---
run_nasm() {
    nasm -f elf64 "$SCRIPT_DIR/src/boot/multiboot_entry.asm" -o "$STUB_OBJ"
}
if [ "$HAS_NASM" = "yes" ]; then
    echo "--- Stage 2: Assemble entry stub (native) ---"
    run_nasm
elif [ "$HAS_DOCKER" = "yes" ]; then
    echo "--- Stage 2: Assemble entry stub (Docker) ---"
    docker run --rm -v "$HOST_ROOT://work" -w //work/phost_kernel \
        debian:trixie-slim bash -c \
        "apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq nasm >/dev/null 2>&1 && \
         nasm -f elf64 src/boot/multiboot_entry.asm -o multiboot_entry.o"
else
    echo "ERROR: need nasm (native or Docker)"; exit 1
fi
echo "Stub: $STUB_OBJ ($(wc -c < "$STUB_OBJ") bytes)"
echo ""

# --- Stage 3: link ---
run_link() {
    # $1 = path to staticlib (container path if in Docker)
    ld.lld -m elf_x86_64 -no-pie -T "$SCRIPT_DIR/linker.ld" -e _start \
        -o "$ELF_OUT" "$STUB_OBJ" "$1"
}
if [ "$HAS_LLD" = "yes" ]; then
    echo "--- Stage 3: Link (native ld.lld) ---"
    run_link "$RUST_LIB"
elif [ "$HAS_DOCKER" = "yes" ]; then
    echo "--- Stage 3: Link (Docker ld.lld) ---"
    CONTAINER_LIB="//work/target/$TARGET_TRIPLE/release/libphost_kernel.a"
    docker run --rm -v "$HOST_ROOT://work" -w //work/phost_kernel \
        debian:trixie-slim bash -c \
        "apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq lld binutils >/dev/null 2>&1 && \
         ld.lld -m elf_x86_64 -no-pie -T linker.ld -e _start \
            -o $(basename "$ELF_OUT") multiboot_entry.o '$CONTAINER_LIB'"
else
    echo "ERROR: need ld.lld (native or Docker)"; exit 1
fi
echo "ELF: $ELF_OUT ($(wc -c < "$ELF_OUT") bytes)"
echo ""

# --- Stage 4: objcopy to flat image ---
if [ "$HAS_OBJCOPY" = "yes" ]; then
    echo "--- Stage 4: objcopy -O binary ---"
    objcopy -O binary "$ELF_OUT" "$OUTPUT"
elif [ "$HAS_DOCKER" = "yes" ]; then
    echo "--- Stage 4: objcopy -O binary (Docker) ---"
    docker run --rm -v "$HOST_ROOT://work" \
        debian:trixie-slim bash -c \
        "apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq binutils >/dev/null 2>&1 && \
         objcopy -O binary //work/phost_kernel/$(basename "$ELF_OUT") //work/phost_kernel/$(basename "$OUTPUT")"
else
    echo "ERROR: need objcopy (native or Docker)"; exit 1
fi
echo "Image: $OUTPUT ($(wc -c < "$OUTPUT") bytes)"
echo ""

# --- Stage 5: verify header ---
echo "--- Stage 5: Image header (expect 02 b0 ad 1b) ---"
xxd -l 32 "$OUTPUT" 2>/dev/null || od -A x -t x1 -N 32 "$OUTPUT"
echo ""

echo "=== Build Complete ==="
echo "Image: $OUTPUT"
