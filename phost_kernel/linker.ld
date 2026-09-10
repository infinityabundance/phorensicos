/* Phost Kernel — Linker Script
 * Loaded at 1 MiB. The Multiboot v1 header must be in the first 8192 bytes.
 * The 32-bit stub (text32) transitions to long mode and calls the 64-bit
 * Rust kernel (text). Data/BSS follow. Link with: ld -m elf_i386
 */

ENTRY(_start)

SECTIONS {
    . = 1M;

    .multiboot : ALIGN(16) {
        KEEP(*(.multiboot))
    }

    .text32 : ALIGN(4096) {
        *(.text32)
    }

    .text : ALIGN(4096) {
        *(.text .text.*)
    }

    .text64 : ALIGN(4096) {
        *(.text64)
    }

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4096) {
        *(.data .data.*)
    }

    .bss : ALIGN(4096) {
        __bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note.gnu.build-id)
    }
}
