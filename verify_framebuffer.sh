#!/bin/bash
# Thin entry point. The real boot-evidence verifier lives next to the kernel
# that produces the evidence: phost_kernel/verify_evidence.sh
#
# It validates the artifacts captured by phost_kernel/boot_qemu.sh
# (serial/debug proof bytes, the framebuffer ABI at 0x300000, the screendump
# palette, and the linear-framebuffer dump).
#
# Usage: ./verify_framebuffer.sh [evidence_dir]
set -u

ROOT="$(cd "$(dirname "$0")" && pwd)"
exec "$ROOT/phost_kernel/verify_evidence.sh" "$@"
