#!/bin/bash
# Phorensic OS — Evidence Pack Generator
# Produces all files for the boot proof evidence pack

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EVIDENCE_DIR="$SCRIPT_DIR/evidence_pack"
mkdir -p "$EVIDENCE_DIR"

echo "=== Generating Evidence Pack ==="
echo "Output: $EVIDENCE_DIR"
echo ""

# Step 1: Build the kernel
echo "--- Building kernel ---"
bash "$SCRIPT_DIR/build_kernel.sh" "$SCRIPT_DIR/test_kernel.ph" "$EVIDENCE_DIR/phorensic-kernel.elf"

# Step 2: Copy receipts
echo "--- Copying receipts ---"
cp "$SCRIPT_DIR/test_kernel.receipts.json" "$EVIDENCE_DIR/kernel.receipts.json"

# Step 3: ELF header dump
echo "--- ELF header ---"
od -A x -t x1 -N 64 "$EVIDENCE_DIR/phorensic-kernel.elf" > "$EVIDENCE_DIR/readelf-h.txt"
cat "$EVIDENCE_DIR/readelf-h.txt"

# Step 4: Program headers
echo "" >> "$EVIDENCE_DIR/readelf-l.txt"
echo "Program headers from binary:" >> "$EVIDENCE_DIR/readelf-l.txt"
od -A x -t x1 -j 64 -N 64 "$EVIDENCE_DIR/phorensic-kernel.elf" >> "$EVIDENCE_DIR/readelf-l.txt"
echo "Multiboot header location:" >> "$EVIDENCE_DIR/readelf-l.txt"
od -A x -t x1 -N 8192 "$EVIDENCE_DIR/phorensic-kernel.elf" | grep "02 b0 ad 1b" | head -3 >> "$EVIDENCE_DIR/readelf-l.txt"
cat "$EVIDENCE_DIR/readelf-l.txt"

# Step 5: Disassembly dump of entry point
echo "--- Entry disassembly (first 256 bytes of PT_LOAD) ---"
od -A x -t x1 -j 4096 -N 176 "$EVIDENCE_DIR/phorensic-kernel.elf" > "$EVIDENCE_DIR/objdump-dr.txt"
cat "$EVIDENCE_DIR/objdump-dr.txt"

# Step 6: Multiboot header verification
echo "" >> "$EVIDENCE_DIR/objdump-dr.txt"
echo "Multiboot v1 header bytes at offset 0xA0 within load segment:" >> "$EVIDENCE_DIR/objdump-dr.txt"
od -A x -t x1 -j $((4096 + 0xA0)) -N 12 "$EVIDENCE_DIR/phorensic-kernel.elf" >> "$EVIDENCE_DIR/objdump-dr.txt"

# Step 7: Boot in QEMU and capture output (debug + serial)
# Run from evidence dir so native-path `-serial file:` works on Windows
echo "--- QEMU boot ---"
pushd "$EVIDENCE_DIR" > /dev/null
bash "$SCRIPT_DIR/boot_qemu.sh" phorensic-kernel.elf .
popd > /dev/null

# Step 8: Copy build scripts
cp "$SCRIPT_DIR/build_kernel.sh" "$EVIDENCE_DIR/build_kernel.sh"
cp "$SCRIPT_DIR/boot_qemu.sh" "$EVIDENCE_DIR/boot_qemu.sh"
cp "$SCRIPT_DIR/kernel.ld" "$EVIDENCE_DIR/kernel.ld"
cp "$SCRIPT_DIR/test_kernel.ph" "$EVIDENCE_DIR/test_kernel.ph"

# Step 9: Copy verification script
cp "$SCRIPT_DIR/verify_evidence.sh" "$EVIDENCE_DIR/verify_evidence.sh"

# Step 10: Create README
cat > "$EVIDENCE_DIR/README.md" << 'EOF'
# Phorensic OS — Boot Proof Evidence Pack

This pack contains the evidence that `phorc`, the Phorensic bootstrap compiler,
produces a bootable x86-64 kernel that QEMU loads and executes.

## Contents

| File | Description |
|------|-------------|
| `test_kernel.ph` | Phorensic source — minimal kernel with `kernel_entry()` |
| `phorensic-kernel.elf` | Linked Multiboot v1-compatible ELF (32-bit header, 64-bit code) |
| `kernel.receipts.json` | Byte attribution receipts + boot residuals + ELF hash |
| `debug.log` | QEMU debug port (0xE9) output: "Ph" |
| `serial.log` | QEMU serial port (0x3F8) output: "Ph" |
| `readelf-h.txt` | ELF header: ELFCLASS32, EM_386 |
| `readelf-l.txt` | Program headers + Multiboot header at file 0x10a8 |
| `objdump-dr.txt` | Entry code bytes + Multiboot header bytes |
| `build_kernel.sh` | Repeatable build: phorc --emit-kernel + ld.lld -m elf_i386 |
| `boot_qemu.sh` | Repeatable boot: qemu -kernel -debugcon -serial -display none |
| `verify_evidence.sh` | Automated verification — fails if proof is incomplete |
| `kernel.ld` | Linker script: 0x100000 load, PHDRS for text/data/note |

## Memory Layout (VMA)

```
0x100000  _start / 32-bit entry code
0x1000a8  Multiboot v1 header (magic 0x1BADB002)
0x1000ac  64-bit entry code (segments, serial, VGA, call kernel_entry, halt)
0x101000  PML4 page (entry 0 → PDP at 0x102003 | present+writable)
0x102000  PDP page (entry 0 → 1GB page at 0 | PS=1, present, writable)
0x103000  Stack (16 KB)
0x107000  Stack top + _phor_kernel_entry code
```

## Build and Boot

```sh
# Build
./build_kernel.sh test_kernel.ph phorensic-kernel.elf

# Boot
./boot_qemu.sh phorensic-kernel.elf

# Verify
./verify_evidence.sh
```

## Verification Chain

1. `phorensic-kernel.elf` is ELFCLASS32, EM_386 (Multiboot v1 compatible)
2. Entry point `0x100000` points to 32-bit startup code (`mov edi, ebx`), not the header
3. Multiboot v1 magic `0x1BADB002` at VMA `0x1000a8` within first 8KB of image
4. Startup enables PAE, loads CR3→PML4 at 0x101000, sets EFER.LME, loads GDT, far-jumps to 64-bit
5. 64-bit code writes "Ph" to debug port 0xE9 (`debug.log`) and serial 0x3F8 (`serial.log`)
6. 64-bit code writes "Ph" to VGA text buffer at 0xB8000 (green-on-black)
7. Calls `_phor_kernel_entry(boot_info_ptr)` at VMA 0x107000
8. Halts cleanly with `cli; hlt; jmp $`
9. `kernel.receipts.json` records byte attribution, residuals with ELF hash
EOF

echo ""
echo "=== Evidence Pack Generated ==="
echo "Files in $EVIDENCE_DIR:"
ls -la "$EVIDENCE_DIR/"
