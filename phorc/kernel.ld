/* Phorensic OS — Kernel Linker Script */
/* Produces a freestanding x86-64 ELF executable for the kernel + boot chain */

ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);   /* PF_R | PF_X */
    data PT_LOAD FLAGS(6);   /* PF_R | PF_W */
    note PT_NOTE;
}

SECTIONS {
    /* Load address: conventional 1MB mark for x86-64 kernels */
    . = 0x100000;

    .text : ALIGN(4K) {
        *(.text)
        *(.text.*)
    } :text

    .rodata : ALIGN(4K) {
        *(.rodata)
        *(.rodata.*)
    } :text

    .data : ALIGN(4K) {
        *(.data)
        *(.data.*)
    } :data

    .bss : ALIGN(4K) {
        *(COMMON)
        *(.bss)
        *(.bss.*)
    } :data

    /* Keep .note with PT_NOTE segment for PVH note (required by QEMU -kernel) */
    .note : ALIGN(4) {
        *(.note)
        *(.note.*)
    } :note

    /* Discard unnecessary sections */
    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.eh_frame_hdr)
    }
}
