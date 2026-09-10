; phost_kernel — 32-bit Entry Stub (Multiboot AOUT + Bochs VBE + long mode)
;
; The final kernel is a FLAT image (objcopy -O binary). QEMU's Multiboot
; loader (AOUT kludge) scans the first 8KB of the file, sees the header at
; offset 0, loads the raw image at 0x100000 and jumps to the entry address
; in 32-bit protected mode (paging off).
;
; This stub:
;   1. Carries the Multiboot v1 AOUT header (bytes 0x100000-0x100020)
;   2. Sets a 1024x768x32 linear framebuffer via Bochs VBE ports
;      (0x1CE/0x1CF — QEMU std-VGA mode path; QEMU does NOT support
;      multiboot video mode itself)
;   3. Writes the compact 5-field framebuffer ABI at 0x108000
;   4. Builds 4GB identity page tables
;   5. Transitions to long mode
;   6. Zeroes .bss (the flat image carries no .bss content)
;   7. Installs a full 256-entry IDT with per-vector exception stubs, a TSS
;      with an IST (interrupt stack table) so that even stack-corruption
;      faults are catchable, and a diagnostic exception handler that prints
;      the vector/RIP/CR2/registers to the serial + debug ports, then halts.
;   8. Calls the 64-bit Rust kernel_main
;
; Assembled with: nasm -f elf64 multiboot_entry.asm -o multiboot_entry.o

BITS 32

section .text32
global _start
extern kernel_main
extern __bss_start
extern __bss_end

; ------------------------------------------------------------------
; Multiboot v1 header — at the very start of the flat image (0x100000).
; ------------------------------------------------------------------
align 4
mb_header:
    dd 0x1BADB002                    ; magic
    dd 0x00010002                    ; flags: AOUT kludge (16) | meminfo (2)
    dd -(0x1BADB002 + 0x00010002)    ; checksum
    dd 0x100000                      ; header_addr
    dd 0x100000                      ; load_addr
    dd 0                             ; load_end_addr (QEMU uses file size)
    dd 0                             ; bss_end_addr
    dd _start                        ; entry_addr (link-time resolved)

; Bochs VBE registers (QEMU std VGA)
%define VBE_INDEX   0x1CE
%define VBE_DATA    0x1CF
%define VBE_DISPI_INDEX_XRES      0x1
%define VBE_DISPI_INDEX_YRES      0x2
%define VBE_DISPI_INDEX_BPP       0x3
%define VBE_DISPI_INDEX_ENABLE    0x4

; Framebuffer ABI (see boot.rs: BootFramebufferInfo).
; NOTE: must NOT overlap the kernel image (0x100000-0x153600) or the
; kernel heap (.bss). 0x300000 is free RAM above both. It is written
; AFTER the .bss zeroing (see long_mode), because the .bss range
; (0x154000-0x9571A8) covers 0x300000.
%define BOOT_FB_INFO    0x300000
%define FB_ADDR         0xFD000000   ; QEMU std VGA LFB

_start:
    cli

    ; ----------------------------------------------------------
    ; 1. Set VBE mode: 1024x768x32 with LFB enabled
    ; ----------------------------------------------------------
    ; Disable display first
    mov dx, VBE_INDEX
    mov ax, VBE_DISPI_INDEX_ENABLE
    out dx, ax
    mov dx, VBE_DATA
    mov ax, 0x0000
    out dx, ax

    ; X resolution = 1024
    mov dx, VBE_INDEX
    mov ax, VBE_DISPI_INDEX_XRES
    out dx, ax
    mov dx, VBE_DATA
    mov ax, 1024
    out dx, ax

    ; Y resolution = 768
    mov dx, VBE_INDEX
    mov ax, VBE_DISPI_INDEX_YRES
    out dx, ax
    mov dx, VBE_DATA
    mov ax, 768
    out dx, ax

    ; BPP = 32
    mov dx, VBE_INDEX
    mov ax, VBE_DISPI_INDEX_BPP
    out dx, ax
    mov dx, VBE_DATA
    mov ax, 32
    out dx, ax

    ; Enable + LFB: VBE_DISPI_ENABLED(1) | LFB(0x40) = 0x41
    mov dx, VBE_INDEX
    mov ax, VBE_DISPI_INDEX_ENABLE
    out dx, ax
    mov dx, VBE_DATA
    mov ax, 0x0041
    out dx, ax

    ; ----------------------------------------------------------
    ; (The compact framebuffer ABI is written in long_mode, AFTER the
    ; .bss zeroing — writing it here would be clobbered by the rep
    ; stosb below, since .bss covers BOOT_FB_INFO.)
    ; ----------------------------------------------------------

    ; ----------------------------------------------------------
    ; 3. Page tables: 4GB identity map with 2MB pages
    ;    PML4 @ 0x1000, PDPT @ 0x2000, PD @ 0x3000
    ; ----------------------------------------------------------
    mov edi, 0x1000
    xor eax, eax
    ; Clear the whole page-table area (PML4+PDPT at 0x1000-0x3000 plus
    ; the four PDs at 0x3000-0x7000). The PD loop below writes only the
    ; low 32 bits of each entry; without zeroing the high dwords first,
    ; they inherit stale BIOS RAM — including reserved-bit garbage that
    ; turns every walk of those pages into a #PF.
    mov ecx, 0x6000 / 4             ; 24KB: 0x1000..0x7000
    rep stosd

    ; PML4[0] -> PDPT at 0x2000
    mov dword [0x1000], 0x2003

    ; PDPT[0..3] -> four PDs (0x3000/0x4000/0x5000/0x6000). The PD
    ; table below fills 2048 entries (4 x 512), covering 0-4GB, but
    ; only the first 1GB is reachable unless ALL FOUR PDPT entries
    ; are present — the framebuffer (0xFD000000) lives at ~3.9GB, so
    ; a missing PDPT[3] would fault the first canvas write.
    mov dword [0x2000], 0x3003
    mov dword [0x2008], 0x4003
    mov dword [0x2010], 0x5003
    mov dword [0x2018], 0x6003

    ; PD: 2048 x 2MB pages = 4GB identity map
    mov edi, 0x3000
    mov eax, 0x00000083              ; Present | Writable | Huge
    xor ecx, ecx
.pd_loop:
    mov [edi + ecx * 8], eax
    add eax, 0x200000
    inc ecx
    cmp ecx, 2048
    jl .pd_loop

    ; ----------------------------------------------------------
    ; 4. Long mode transition
    ; ----------------------------------------------------------
    lgdt [gdt32_desc]

    ; Enable PAE (CR4 bit 5)
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    ; Load PML4
    mov eax, 0x1000
    mov cr3, eax

    ; EFER.LME = 1 (MSR 0xC0000080, bit 8)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr

    ; Switch to the 64-bit GDT (code descriptor with L bit) BEFORE
    ; enabling paging, so the far jump lands in real long mode
    ; rather than 32-bit compatibility mode.
    lgdt [gdt64_desc]

    ; CR0: PG | PE | WP = 0x80010001
    mov eax, cr0
    or eax, 0x80010001
    mov cr0, eax

    ; Far jump into 64-bit code (selector 0x08)
    jmp 0x08:long_mode

; ------------------------------------------------------------------
align 16
gdt32:
    dq 0x0000000000000000            ; null
    dq 0x00CF9A000000FFFF            ; code (32-bit flat)
    dq 0x00CF92000000FFFF            ; data (32-bit flat)
gdt32_end:

gdt32_desc:
    dw gdt32_end - gdt32 - 1
    dd gdt32

align 16
gdt64:
    dq 0x0000000000000000            ; null (0x00)
    dq 0x00209A0000000000            ; 64-bit code, L bit set (0x08)
    dq 0x0000920000000000            ; 64-bit data (0x10)
gdt64_end:

gdt64_desc:
    dw gdt64_end - gdt64 - 1
    dd gdt64

; ------------------------------------------------------------------
; Exception handling scratch, TSS, and IST stack (all in .bss — the
; stub zeroes .bss on entry to long mode, so these start clean).
; ------------------------------------------------------------------
section .bss
align 16
exc_vector:   resq 1                 ; exception vector (written by the stub slot)
exc_cr2:      resq 1                 ; faulting address (#PF)
exc_rip:      resq 1                 ; faulting instruction pointer
exc_cs:       resq 1
exc_rflags:   resq 1
exc_rsp:      resq 1                 ; faulting RSP at handler entry
exc_tss1:     resq 1
exc_fr0:      resq 1                 ; raw frame qwords at [exc_rsp+0..24]
exc_fr1:      resq 1
exc_fr2:      resq 1
exc_fr3:      resq 1
exc_rax:      resq 1
exc_rbx:      resq 1
exc_rcx:      resq 1
exc_rdx:      resq 1
exc_rdi:      resq 1
exc_rsi:      resq 1
exc_rbp:      resq 1
exc_stack:    resb 8192              ; exception handler stack
exc_stack_top:
idt_storage:  resb 4096              ; 256 x 16-byte IDT entries (built at boot)
; ------------------------------------------------------------------
; 64-bit mode
; ------------------------------------------------------------------
BITS 64
section .text64

; ------------------------------------------------------------------
; Debug marker: emit one byte to the debug port (0xE9). Used to trace
; boot progress without serial init.
; ------------------------------------------------------------------
%macro dbg 1
    push rax
    push rdx
    mov dx, 0xE9
    mov al, %1
    out dx, al
    pop rdx
    pop rax
%endmacro

long_mode:
    ; 64-bit segment registers
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Stack: 64KB region in conventional memory. NOTE: the region
    ; directly below 1MiB (0xF0000-0xFFFFF) is the system BIOS ROM
    ; shadow — reads return ROM content and writes are silently
    ; dropped — so the stack must live BELOW 0xC0000. 0x90000 gives
    ; 0x80000-0x90000 (64KB), clear of the EBDA (0x9FC00) and the
    ; option-ROM window (0xC0000+).
    mov rsp, 0x90000

    dbg '1'   ; entered long mode

    ; ----------------------------------------------------------
    ; Zero .bss (heap + statics). The flat image does not carry
    ; .bss content, so without this the allocator heap and Rust
    ; statics start as arbitrary RAM.
    ; ----------------------------------------------------------
    mov rdi, __bss_start
    mov rcx, __bss_end
    sub rcx, rdi
    xor eax, eax
    rep stosb

    ; ----------------------------------------------------------
    ; Write the compact framebuffer ABI at BOOT_FB_INFO. Done here
    ; (AFTER the .bss zeroing) because .bss covers 0x300000.
    ; ----------------------------------------------------------
    mov dword [BOOT_FB_INFO],      FB_ADDR
    mov dword [BOOT_FB_INFO + 4],  0x00000000
    mov dword [BOOT_FB_INFO + 8],  1024
    mov dword [BOOT_FB_INFO + 12], 768
    mov dword [BOOT_FB_INFO + 16], 4096       ; 1024 * 4
    mov dword [BOOT_FB_INFO + 20], 32

    dbg '2'   ; .bss zeroed, ABI written

    ; ----------------------------------------------------------
    ; Build the 256-entry IDT in idt_storage. Each entry points at
    ; its own 16-byte stub slot in exc_table. Built at runtime
    ; because NASM cannot fold &/>> relocations for ELF64 output.
    ; ----------------------------------------------------------
    mov rdi, idt_storage
    mov rsi, exc_table
    xor ecx, ecx                     ; vector
.idt_loop:
    mov rax, rsi
    mov rbx, rcx
    shl rbx, 4
    add rax, rbx                     ; rax = exc_table + vector*16
    mov [rdi], ax                    ; offset 15:0
    mov word [rdi + 2], 0x0008       ; selector: 64-bit code
    mov byte [rdi + 4], 0x00         ; IST = 0 (frame is pushed on the faulting stack)
    mov byte [rdi + 5], 0x8E         ; present, DPL0, 64-bit interrupt gate
    shr rax, 16
    mov [rdi + 6], ax                ; offset 31:16
    shr rax, 16
    mov [rdi + 8], eax               ; offset 63:32
    mov dword [rdi + 12], 0          ; reserved
    add rdi, 16
    inc ecx
    cmp ecx, 256
    jne .idt_loop

    ; Load the IDT
    lidt [idt_desc]

    dbg '4'   ; IDT loaded

    ; Jump to the Rust kernel
    mov rax, kernel_main
    call rax

.hang:
    hlt
    jmp .hang

; ------------------------------------------------------------------
; Per-vector stubs: record the vector in fixed scratch (no stack
; access), then enter the common handler. Defined before the IDT so
; the IDT's expressions resolve on the assembler's first pass.
; ------------------------------------------------------------------
align 16
exc_table:
%assign v 0
%rep 256
    mov byte [exc_vector], v
    jmp exc_common
    times 16 - ($ - exc_table - v*16) db 0x90
%assign v v+1
%endrep

; ------------------------------------------------------------------
; Common exception handler.
;
; On entry the CPU has switched RSP to exc_stack_top (IST1) and pushed
; the exception frame onto it:
;   error-code vectors (8,10,11,12,13,14,17,21):
;       [rsp+0]=ERR [rsp+8]=RIP [rsp+16]=CS [rsp+24]=RFLAGS
;   other vectors:
;       [rsp+0]=RIP [rsp+8]=CS  [rsp+16]=RFLAGS
; ------------------------------------------------------------------
align 16
exc_common:
    ; The frame is on the FAULTING stack at [rsp]:
    ;   error-code vectors (8,10,11,12,13,14,17,21):
    ;       [rsp+0]=ERR [rsp+8]=RIP [rsp+16]=CS [rsp+24]=RFLAGS
    ;   other vectors:
    ;       [rsp+0]=RIP [rsp+8]=CS  [rsp+16]=RFLAGS
    ; Save the faulting RSP and the general registers first.
    mov [exc_rsp], rsp
    mov [exc_rax], rax
    mov [exc_rbx], rbx
    mov [exc_rcx], rcx
    mov [exc_rdx], rdx
    mov [exc_rdi], rdi
    mov [exc_rsi], rsi
    mov [exc_rbp], rbp
    mov rax, cr2
    mov [exc_cr2], rax

    ; Save the raw frame qwords.
    mov rax, [rsp + 0]
    mov [exc_fr0], rax
    mov rax, [rsp + 8]
    mov [exc_fr1], rax
    mov rax, [rsp + 16]
    mov [exc_fr2], rax
    mov rax, [rsp + 24]
    mov [exc_fr3], rax

    ; RIP offset depends on whether the vector pushes an error code.
    mov rbx, [exc_vector]
    xor rcx, rcx
    cmp rbx, 8
    je .has_err
    cmp rbx, 10
    jb .no_err
    cmp rbx, 14
    jbe .has_err
    cmp rbx, 17
    je .has_err
    cmp rbx, 21
    je .has_err
    jmp .no_err
.has_err:
    mov rcx, 8
.no_err:
    mov rax, [rsp + rcx + 0]
    mov [exc_rip], rax
    mov rax, [rsp + rcx + 8]
    mov [exc_cs], rax
    mov rax, [rsp + rcx + 16]
    mov [exc_rflags], rax

    ; Frame extracted — switch to the dedicated handler stack.
    mov rsp, exc_stack_top

    ; Diagnostics to serial (0x3F8) + debug (0xE9) ports.
    mov rsi, msg_exc
    call exc_print_str
    mov rax, [exc_vector]
    call exc_print_hex
    mov rsi, msg_rip
    call exc_print_str
    mov rax, [exc_rip]
    call exc_print_hex
    mov rsi, msg_cs
    call exc_print_str
    mov rax, [exc_cs]
    call exc_print_hex
    mov rsi, msg_flags
    call exc_print_str
    mov rax, [exc_rflags]
    call exc_print_hex
    mov rsi, msg_cr2
    call exc_print_str
    mov rax, [exc_cr2]
    call exc_print_hex
    mov rsi, msg_rsp
    call exc_print_str
    mov rax, [exc_rsp]
    call exc_print_hex
    mov rsi, msg_rax
    call exc_print_str
    mov rax, [exc_rax]
    call exc_print_hex
    ; Raw frame qwords captured at entry
    mov rsi, msg_fr0
    call exc_print_str
    mov rax, [exc_fr0]
    call exc_print_hex
    mov rsi, msg_fr1
    call exc_print_str
    mov rax, [exc_fr1]
    call exc_print_hex
    mov rsi, msg_fr2
    call exc_print_str
    mov rax, [exc_fr2]
    call exc_print_hex
    mov rsi, msg_fr3
    call exc_print_str
    mov rax, [exc_fr3]
    call exc_print_hex
    mov rsi, msg_nl
    call exc_print_str
.halt:
    cli
    hlt
    jmp .halt

; ------------------------------------------------------------------
; Minimal port-I/O diagnostics (self-contained — no Rust dependency)
; ------------------------------------------------------------------
; Write AL to serial 0x3F8 and debug 0xE9.
exc_out_al:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    mov dx, 0xE9
    out dx, al
    pop rdx
    pop rax
    ret

; Print NUL-terminated string at [rsi].
exc_print_str:
    push rax
.next:
    lodsb
    test al, al
    jz .done
    call exc_out_al
    jmp .next
.done:
    pop rax
    ret

; Print RAX as 16 hex digits.
exc_print_hex:
    push rax
    push rcx
    push rdx
    mov rcx, 16
.loop:
    rol rax, 4
    mov dl, al
    and dl, 0x0F
    cmp dl, 10
    jl .digit
    add dl, 'A' - 10
    jmp .emit
.digit:
    add dl, '0'
.emit:
    mov al, dl
    call exc_out_al
    dec rcx
    jnz .loop
    pop rdx
    pop rcx
    pop rax
    ret

; ------------------------------------------------------------------
align 8
msg_exc:    db "PHOR-EXC VEC=", 0
msg_rip:    db " RIP=", 0
msg_cs:     db " CS=", 0
msg_flags:  db " RFL=", 0
msg_cr2:    db " CR2=", 0
msg_rsp:    db " RSP=", 0
msg_rax:    db " RAX=", 0
msg_tss1:   db " TSS1=", 0
msg_fr0:    db " FR0=", 0
msg_fr1:    db " FR1=", 0
msg_fr2:    db " FR2=", 0
msg_fr3:    db " FR3=", 0
msg_nl:     db 0x0D, 0x0A, 0

; ------------------------------------------------------------------
; IDT descriptor. The entries themselves are built at boot time in
; idt_storage (see long_mode) because NASM cannot express bit-masked
; relocations (offset >> 16 / & 0xFFFF) in ELF64 object files.
; ------------------------------------------------------------------
align 16
idt_desc:
    dw 4096 - 1
    dq idt_storage
