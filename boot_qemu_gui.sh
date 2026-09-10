#!/bin/bash
# Thin entry point. The real QEMU boot + evidence capture lives next to the
# kernel that it boots: phost_kernel/boot_qemu.sh
#
# That script captures the strong evidence set (COM1/debug proof bytes, the
# framebuffer ABI at 0x300000, a screendump and the linear-framebuffer dump).
# Verify it afterwards with ./verify_framebuffer.sh.
#
# Usage: ./boot_qemu_gui.sh [kernel_image] [evidence_dir] [seconds]
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
KERNEL="${1:-$ROOT/phost_kernel/phorensic-kernel.elf}"
OUT="${2:-$ROOT/phost_kernel/evidence}"
SECS="${3:-8}"

# boot_qemu.sh chdirs to its own directory, so resolve caller-relative paths now.
case "$KERNEL" in /*) ;; *) KERNEL="$PWD/$KERNEL" ;; esac
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac

exec "$ROOT/phost_kernel/boot_qemu.sh" "$KERNEL" "$OUT" "$SECS"
