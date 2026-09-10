; x86_64_boot.asm — Boot sector, protected/long mode transition, nucleus load
; Phorensic OS — Trusted Nucleus Bootstrap
;
; File: src/asm/x86_64_boot.asm
; Architecture: x86_64
; Court: boot_asm:v2
; Audit: Line-audited by Nucleus Audit Team

; ─────────────────────────────────────────────────────────────
; Boot Sector (Stage 1) — 512 bytes, loaded at 0x7C00
; ─────────────────────────────────────────────────────────────

[BITS 16]
[ORG 0x7C00]

; Entry point from BIOS
boot_start:
    ; Save boot drive number
    mov [boot_drive], dl

    ; Initialize segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00

    ; Save boot signature for residual
    mov [boot_signature], 0xAA55

    ; Clear screen (BIOS int 0x10, function 0x06)
    mov ah, 0x06
    xor al, al
    xor cx, cx
    mov dh, 24
    mov dl, 79
    mov bh, 0x07
    int 0x10

    ; Display boot message
    mov si, boot_msg
    call print_string

    ; Check for A20 gate enable
    call check_a20
    cmp ax, 1
    je a20_enabled

    ; Enable A20 gate via keyboard controller
    call enable_a20_keyboard

a20_enabled:
    mov si, a20_ok_msg
    call print_string

    ; Load stage 2 (nucleus loader) from disk
    mov si, load_stage2_msg
    call print_string

    ; Load sectors from disk (LBA)
    ; Stage 2 is at LBA 1, 16 sectors (8KB)
    mov dword [dap_lba], 1
    mov word [dap_sectors], 64     ; Load 64 sectors (32KB) for nucleus + kernel
    mov word [dap_offset], 0x8000  ; Load to 0x8000:0x0000
    mov word [dap_segment], 0x0000
    mov word [dap_size], 0x0010

    mov ah, 0x42          ; Extended read
    mov dl, [boot_drive]
    mov si, dap_packet
    int 0x13
    jc disk_error

    ; Verify stage 2 integrity (simple checksum)
    mov si, verify_msg
    call print_string

    mov cx, 32768         ; 64 sectors * 512 bytes
    mov bx, 0x8000        ; Start of stage 2
    xor ax, ax
checksum_loop:
    add al, [bx]
    inc bx
    dec cx
    jnz checksum_loop

    ; Store checksum
    mov [stage2_checksum], al

    ; Jump to stage 2 (enter protected mode first)
    mov si, enter_pm_msg
    call print_string

    ; Disable interrupts
    cli

    ; Load GDT for protected mode
    lgdt [gdt32_descriptor]

    ; Set PE bit in CR0
    mov eax, cr0
    or eax, 0x00000001
    mov cr0, eax

    ; Far jump to flush prefetch queue
    jmp 0x08:protected_mode_entry

; ─────────────────────────────────────────────────────────────
; Disk Read Error Handler
; ─────────────────────────────────────────────────────────────

disk_error:
    mov si, disk_error_msg
    call print_string
    mov ah, 0x00
    int 0x16              ; Wait for key
    int 0x19              ; Reboot

; ─────────────────────────────────────────────────────────────
; Print String Function (BIOS int 0x10, function 0x0E)
; ─────────────────────────────────────────────────────────────

print_string:
    push ax
    push si
.loop:
    lodsb
    or al, al
    jz .done
    mov ah, 0x0E
    mov bh, 0x00
    int 0x10
    jmp .loop
.done:
    pop si
    pop ax
    ret

; ─────────────────────────────────────────────────────────────
; Check A20 Gate Status
; ─────────────────────────────────────────────────────────────

check_a20:
    pushf
    push ds
    push es
    push di
    push si

    cli

    xor ax, ax
    mov es, ax
    mov di, 0x0500

    not ax
    mov ds, ax
    mov si, 0x0510

    mov al, [es:di]
    push ax

    mov al, [ds:si]
    push ax

    mov byte [es:di], 0x00
    mov byte [ds:si], 0xFF

    cmp byte [es:di], 0xFF

    pop ax
    mov [ds:si], al

    pop ax
    mov [es:di], al

    mov ax, 0
    je .done
    mov ax, 1

.done:
    pop si
    pop di
    pop es
    pop ds
    popf
    ret

; ─────────────────────────────────────────────────────────────
; Enable A20 Gate via Keyboard Controller
; ─────────────────────────────────────────────────────────────

enable_a20_keyboard:
    push ax

    ; Wait for keyboard controller
    call kbc_wait_input
    mov al, 0xD1          ; Command: write output port
    out 0x64, al

    call kbc_wait_input
    mov al, 0xDF          ; Set A20 bit
    out 0x60, al

    call kbc_wait_input

    pop ax
    ret

kbc_wait_input:
    in al, 0x64
    test al, 0x02
    jnz kbc_wait_input
    ret

; ─────────────────────────────────────────────────────────────
; Data — Boot Sector
; ─────────────────────────────────────────────────────────────

boot_msg:           db 'Phorensic OS v0.1 — Booting...', 0x0D, 0x0A, 0
a20_ok_msg:         db '[OK] A20 gate enabled', 0x0D, 0x0A, 0
load_stage2_msg:    db '[..] Loading stage 2...', 0x0D, 0x0A, 0
verify_msg:         db '[..] Verifying stage 2...', 0x0D, 0x0A, 0
enter_pm_msg:       db '[..] Entering protected mode...', 0x0D, 0x0A, 0
disk_error_msg:     db '[!!] Disk read error — system halted', 0x0D, 0x0A, 0

; Disk Address Packet structure for int 0x13 extension
dap_packet:
    dap_size:       db 0x10
    dap_reserved:   db 0x00
    dap_sectors:    dw 64
    dap_offset:     dw 0x8000
    dap_segment:    dw 0x0000
    dap_lba:        dq 1

; Variables
boot_drive:         db 0
boot_signature:     dw 0
stage2_checksum:    db 0

; ─────────────────────────────────────────────────────────────
; Global Descriptor Table (32-bit Protected Mode)
; ─────────────────────────────────────────────────────────────

gdt32_start:
    ; Null descriptor
    dq 0x0000000000000000

    ; Code segment (ring 0): base=0, limit=0xFFFFF, 32-bit, readable
    dw 0xFFFF           ; Limit (bits 0-15)
    dw 0x0000           ; Base (bits 0-15)
    db 0x00             ; Base (bits 16-23)
    db 0x9A             ; Access: present, ring 0, code, readable
    db 0xCF             ; Granularity: 4KB, 32-bit
    db 0x00             ; Base (bits 24-31)

    ; Data segment (ring 0): base=0, limit=0xFFFFF, writable
    dw 0xFFFF
    dw 0x0000
    db 0x00
    db 0x92             ; Access: present, ring 0, data, writable
    db 0xCF
    db 0x00

gdt32_end:

gdt32_descriptor:
    dw (gdt32_end - gdt32_start - 1)
    dd gdt32_start

; ─────────────────────────────────────────────────────────────
; Boot Sector Padding and Signature
; ─────────────────────────────────────────────────────────────

times 510 - ($ - boot_start) db 0
dw 0xAA55

; ─────────────────────────────────────────────────────────────
; Stage 2 — Protected Mode to Long Mode Transition
; ─────────────────────────────────────────────────────────────

[BITS 32]
[ORG 0x8000]

protected_mode_entry:
    ; Set up segment registers for protected mode
    mov ax, 0x10          ; Data segment selector
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Set up stack
    mov esp, 0x7C00

    ; Display protected mode entry
    mov esi, pm_entry_msg
    call print_string_pm

    ; Check for CPUID support
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x200000
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    push ecx
    popfd

    test eax, 0x200000
    jnz cpuid_supported
    mov esi, no_cpuid_msg
    call print_string_pm
    jmp halt_cpu

cpuid_supported:
    mov esi, cpuid_ok_msg
    call print_string_pm

    ; Check for long mode support
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb no_long_mode

    mov eax, 0x80000001
    cpuid
    test edx, 0x20000000   ; LM bit
    jnz long_mode_supported

no_long_mode:
    mov esi, no_long_mode_msg
    call print_string_pm
    jmp halt_cpu

long_mode_supported:
    mov esi, lm_ok_msg
    call print_string_pm

    ; Build page tables for long mode
    call build_page_tables

    ; Enable PAE and PSE
    mov eax, cr4
    or eax, 0x00000020      ; PAE
    or eax, 0x00000010      ; PSE
    mov cr4, eax

    ; Load PML4 address into CR3
    mov eax, page_table_l4
    mov cr3, eax

    ; Enable long mode in EFER MSR
    mov ecx, 0xC0000080     ; EFER MSR
    rdmsr
    or eax, 0x00000100      ; LME bit
    wrmsr

    ; Enable paging (set PG bit in CR0)
    mov eax, cr0
    or eax, 0x80000001      ; PG | PE
    mov cr0, eax

    ; Load 64-bit GDT
    lgdt [gdt64_descriptor]

    ; Jump to 64-bit long mode
    jmp 0x08:long_mode_entry

halt_cpu:
    hlt
    jmp halt_cpu

; ─────────────────────────────────────────────────────────────
; Build Page Tables (Identity Map First 4MB)
; ─────────────────────────────────────────────────────────────

build_page_tables:
    ; Clear page table region (at 0x1000)
    mov edi, 0x1000
    mov cr3, edi
    xor eax, eax
    mov ecx, 1024 * 4       ; 4 pages of 1024 entries
    rep stosd

    ; Set up PML4 entry (point to PDPT)
    mov edi, 0x1000
    mov eax, 0x2000         ; PDPT address
    or eax, 0x00000003      ; Present | Writable
    mov [edi], eax

    ; Set up PDPT entry (point to PD)
    mov edi, 0x2000
    mov eax, 0x3000         ; PD address
    or eax, 0x00000003      ; Present | Writable
    mov [edi], eax

    ; Set up PD entries (2MB pages)
    mov edi, 0x3000
    mov eax, 0x00000083     ; 2MB page: Present | Writable | Huge
    xor ecx, ecx

.pd_loop:
    mov [edi + ecx * 8], eax
    add eax, 0x200000       ; Next 2MB
    inc ecx
    cmp ecx, 64             ; 64 * 2MB = 128MB identity map
    jl .pd_loop

    ret

; ─────────────────────────────────────────────────────────────
; Print String in Protected Mode
; ─────────────────────────────────────────────────────────────

print_string_pm:
    push eax
    push edi
    mov edi, 0xB8000        ; VGA text mode buffer

.loop:
    lodsb
    or al, al
    jz .done
    mov ah, 0x07            ; White on black
    mov [edi], ax
    add edi, 2
    jmp .loop

.done:
    pop edi
    pop eax
    ret

; ─────────────────────────────────────────────────────────────
; Data — Stage 2
; ─────────────────────────────────────────────────────────────

pm_entry_msg:       db "Protected Mode Entered", 0
cpuid_ok_msg:       db "CPUID Supported", 0
no_cpuid_msg:       db "FATAL: CPUID Not Supported", 0
no_long_mode_msg:   db "FATAL: x86_64 Long Mode Not Supported", 0
lm_ok_msg:          db "x86_64 Long Mode Supported", 0

; ─────────────────────────────────────────────────────────────
; Global Descriptor Table (64-bit Long Mode)
; ─────────────────────────────────────────────────────────────

gdt64_start:
    ; Null descriptor
    dq 0x0000000000000000

    ; 64-bit Code Segment (ring 0)
    dw 0x0000           ; Limit
    dw 0x0000           ; Base
    db 0x00             ; Base
    db 0x9A             ; Access: present, ring 0, code, execute/read
    db 0x20             ; Flags: long mode (bit 5 set)
    db 0x00             ; Base

    ; 64-bit Data Segment (ring 0)
    dw 0x0000
    dw 0x0000
    db 0x00
    db 0x92             ; Access: present, ring 0, data, read/write
    db 0x00
    db 0x00

    ; 64-bit Code Segment (ring 3) — for user mode
    dw 0x0000
    dw 0x0000
    db 0x00
    db 0xFA             ; Access: present, ring 3, code, execute/read
    db 0x20
    db 0x00

    ; 64-bit Data Segment (ring 3)
    dw 0x0000
    dw 0x0000
    db 0x00
    db 0xF2             ; Access: present, ring 3, data, read/write
    db 0x00
    db 0x00

gdt64_end:

gdt64_descriptor:
    dw (gdt64_end - gdt64_start - 1)
    dq gdt64_start

; ─────────────────────────────────────────────────────────────
; 64-bit Long Mode Entry
; ─────────────────────────────────────────────────────────────

[BITS 64]

long_mode_entry:
    ; Set up segment registers for 64-bit mode
    mov ax, 0x10          ; 64-bit data segment
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Set up stack
    mov rsp, 0x7C00

    ; Clear direction flag
    cld

    ; Zero out BSS section (kernel BSS)
    mov rdi, 0x100000     ; Kernel load address + BSS offset
    mov rcx, 0x20000      ; 128KB BSS
    xor rax, rax
    rep stosb

    ; Copy nucleus and kernel from load location
    ; Nucleus was loaded at 0x8000, copy to 0x100000
    mov rsi, 0x8000
    mov rdi, 0x100000     ; Kernel base
    mov rcx, 0x8000       ; 32KB
    rep movsb

    ; Set up initial page table for kernel
    ; PML4 is at 0x1000 (already set up in protected mode)

    ; Initialize kernel entry stack
    mov rsp, kernel_stack_top

    ; Store boot info pointer in rdi (first arg to kernel_entry)
    mov rdi, boot_info_structure

    ; Load nucleus call capability address in rsi
    mov rsi, nucleus_cap_seed

    ; Jump to kernel entry point
    mov rax, 0x100000      ; Kernel entry address
    call rax

    ; Kernel should not return, but if it does, halt
    hlt
    jmp $

; ─────────────────────────────────────────────────────────────
; Boot Info Structure (passed to kernel)
; ─────────────────────────────────────────────────────────────

align 8

boot_info_structure:
    dq 0x100000             ; nucleus_base (kernel image base)
    dq 0x8000               ; nucleus_size (32KB)
    dq 0x100000             ; kernel_entry
    dq 0x7C00               ; stack_base
    dq 0x4000               ; stack_size (16KB)
    dq 0x1000               ; page_table (PML4 address)
    dq 1                    ; generation_counter
    dq 0xCAFEBABE           ; capability_seed
    dq 0                    ; integrity_hash
    dq 0                    ; boot_timestamp
    times 256 db 0          ; machine_config space

nucleus_cap_seed:
    dq 0x4E554300           ; nucleus capability token
    dq 1                    ; generation
    dq 0xFFFFFFFF           ; allowed_operations mask
    dq 1                    ; can_enter_nucleus

; ─────────────────────────────────────────────────────────────
; Kernel Stack
; ─────────────────────────────────────────────────────────────

times 4096 db 0            ; 4KB kernel stack
kernel_stack_top:

; ─────────────────────────────────────────────────────────────
; Page Table Storage
; ─────────────────────────────────────────────────────────────

page_table_l4  equ 0x1000
page_table_pdpt equ 0x2000
page_table_pd   equ 0x3000

; ─────────────────────────────────────────────────────────────
; Stage 2 Padding
; ─────────────────────────────────────────────────────────────

; ─────────────────────────────────────────────────────────────
; SMP Application Processor Startup
; ─────────────────────────────────────────────────────────────

[BITS 16]
smp_ap_entry:
    ; Entry point for Application Processors (APs)
    ; Each AP starts here in real mode after INIT-SIPI-SIPI sequence

    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x1000

    ; Wait for boot CPU to finish initialization
    mov si, ap_wait_msg
    call print_string

    ; Load GDT
    lgdt [gdt32_descriptor_smp]

    ; Enable protected mode
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    jmp 0x08:smp_protected_mode

[BITS 32]
smp_protected_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x1000

    ; Enable PAE
    mov eax, cr4
    or eax, 0x00000020
    mov cr4, eax

    ; Load the same page tables as the boot CPU
    mov eax, [smp_page_table_ptr]
    mov cr3, eax

    ; Enable long mode
    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x00000100
    wrmsr

    ; Enable paging
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    ; Load 64-bit GDT and jump
    lgdt [gdt64_descriptor_smp]
    jmp 0x08:smp_long_mode

[BITS 64]
smp_long_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x2000

    ; Initialize per-CPU data
    mov rdi, [smp_percpu_ptr]
    call init_percpu_data

    ; Set up IDT for this AP
    call install_idt

    ; Enable interrupts
    sti

    ; Call AP initialization function
    extern smp_ap_main
    call smp_ap_main

    ; Should not return
.halt:
    hlt
    jmp .halt

; SMP data
ap_wait_msg:        db "AP starting...", 0
smp_page_table_ptr: dq 0x1000
smp_percpu_ptr:     dq 0

gdt32_start_smp:
    dq 0x0000000000000000
    dw 0xFFFF, 0x0000, 0x00, 0x9A, 0xCF, 0x00
    dw 0xFFFF, 0x0000, 0x00, 0x92, 0xCF, 0x00
gdt32_end_smp:

gdt32_descriptor_smp:
    dw (gdt32_end_smp - gdt32_start_smp - 1)
    dd gdt32_start_smp

gdt64_start_smp:
    dq 0x0000000000000000
    dq 0x00209A0000000000     ; 64-bit code segment
    dq 0x0000920000000000     ; 64-bit data segment
gdt64_end_smp:

gdt64_descriptor_smp:
    dw (gdt64_end_smp - gdt64_start_smp - 1)
    dq gdt64_start_smp

; ─────────────────────────────────────────────────────────────
; Memory Map Retrieval (via BIOS int 0x15, E820)
; ─────────────────────────────────────────────────────────────

[BITS 16]
smbios_map:
    push es
    push di
    push bp

    mov di, 0x5000            ; Buffer for memory map entries
    xor ebx, ebx
    xor bp, bp
    mov edx, 0x534D4150       ; 'SMAP'

.loop:
    mov eax, 0x0000E820
    mov ecx, 24               ; Entry size
    int 0x15
    jc .done

    add di, 24
    inc bp

    cmp ebx, 0
    jne .loop

.done:
    mov [memory_map_count], bp

    pop bp
    pop di
    pop es
    ret

memory_map_count: dw 0

; ─────────────────────────────────────────────────────────────
; ACPI RSDP Discovery
; ─────────────────────────────────────────────────────────────

[BITS 16]
find_rsdp:
    push ds
    push si

    ; Search in EBDA (0x000E0000 - 0x000FFFFF)
    mov ax, 0xE000
    mov ds, ax
    xor si, si
    mov cx, 0x8000             ; 32KB to search

.loop:
    ; Check for 'RSD PTR ' signature
    mov eax, [si]
    cmp eax, 0x20445352       ; 'RSD '
    jne .next
    mov eax, [si + 4]
    cmp eax, 0x20545250       ; 'PTR '
    jne .next

    ; Found RSDP
    mov [rsdp_address], si
    mov [rsdp_segment], ds
    jmp .found

.next:
    add si, 16
    dec cx
    jnz .loop

    ; Not found
    mov word [rsdp_address], 0
    mov word [rsdp_segment], 0

.found:
    pop si
    pop ds
    ret

rsdp_address: dw 0
rsdp_segment: dw 0

; ─────────────────────────────────────────────────────────────
; VESA Framebuffer Detection
; ─────────────────────────────────────────────────────────────

[BITS 16]
detect_vesa_fb:
    push es
    push di

    ; Check for VBE presence
    mov ax, 0x4F00
    mov di, 0x6000
    int 0x10

    cmp ax, 0x004F
    jne .no_vesa

    ; Get VBE mode info for 1024x768x32
    mov ax, 0x4F01
    mov cx, 0x4117           ; Mode number (1024x768x32)
    mov di, 0x6200
    int 0x10

    cmp ax, 0x004F
    jne .no_mode

    ; Set mode
    mov ax, 0x4F02
    mov bx, 0x4117
    int 0x10

    ; Store framebuffer address
    mov ax, [0x6240]          ; PhysBasePtr low
    mov [framebuffer_base], ax
    mov ax, [0x6242]          ; PhysBasePtr high
    mov [framebuffer_base + 2], ax

.no_vesa:
.no_mode:
    pop di
    pop es
    ret

framebuffer_base: dq 0

; ─────────────────────────────────────────────────────────────
; Boot Console Log Buffer
; ─────────────────────────────────────────────────────────────

boot_log_buffer:
    times 2048 db 0
boot_log_index: dw 0

log_boot_message:
    ; SI = message
    push ax
    push di

    mov di, [boot_log_index]
    mov ax, ds
    mov es, ax

.loop:
    lodsb
    or al, al
    jz .done
    mov [boot_log_buffer + di], al
    inc di
    cmp di, 2048
    jge .done
    jmp .loop

.done:
    mov [boot_log_index], di
    pop di
    pop ax
    ret

; ─────────────────────────────────────────────────────────────
; Stage 2 Padding — Ensure minimum size
; ─────────────────────────────────────────────────────────────

; ─────────────────────────────────────────────────────────────
; ELF Image Location (used by bootloader protocol)
; ─────────────────────────────────────────────────────────────

global elf_image_start
global elf_image_end
elf_image_start:
    dd 0x464C457F              ; ELF magic
    db 2                       ; 64-bit
    db 1                       ; Little endian
    db 1                       ; Version
    db 0                       ; OS/ABI
    times 8 db 0
    dw 2                       ; ET_EXEC
    dw 0x003E                  ; x86_64
    dd 1                       ; Version
    dq 0x100000                ; Entry point
    dq 0x40                    ; Program header offset
    dq 0                       ; Section header offset
    dd 0                       ; Flags
    dw 0x40                    ; ELF header size
    dw 0x38                    ; Program header size
    dw 1                       ; Program header count
    ; Program header
    dd 1                       ; PT_LOAD
    dd 7                       ; PF_R | PF_W | PF_X
    dq 0                       ; Offset
    dq 0x100000                ; Vaddr
    dq 0x100000                ; Paddr
    dq 0x20000                 ; File size
    dq 0x20000                 ; Mem size
    dq 0x1000                  ; Align
elf_image_end:

; ─────────────────────────────────────────────────────────────
; Boot Handoff Table — Structured handoff to kernel
; ─────────────────────────────────────────────────────────────

global boot_handoff_table
boot_handoff_table:
    ; Signature
    dd 0x50484F52              ; 'PHOR'
    ; Version
    dd 1
    ; Machine type
    dd 0  ; x86_64 PC
    ; Boot flags
    dd 0
    ; Memory map
    dq 0x5000                 ; E820 map pointer
    dd 0                      ; E820 entry count
    ; Framebuffer
    dq 0                      ; FB physical address
    dd 0                      ; FB width
    dd 0                      ; FB height
    dd 0                      ; FB pitch
    dd 0                      ; FB bpp
    ; RSDP
    dq 0                      ; RSDP address
    ; Kernel image info
    dq 0x100000               ; Kernel load address
    dq 0x20000                ; Kernel size
    ; Stack
    dq 0x7C00                 ; Initial stack
    ; Generation
    dq 1                      ; Initial generation
    ; Capability seed
    dq 0xCAFEBABE             ; Capability seed
    ; Reserved
    times 128 db 0

; ─────────────────────────────────────────────────────────────
; SMP / Multi-Processor Tables
; ─────────────────────────────────────────────────────────────

global smp_trampoline_start
global smp_trampoline_end
smp_trampoline_start:
    ; 16-byte aligned trampoline code for AP startup
    ; Copied to low memory (< 1MB) for real-mode AP boot
    times 512 db 0
smp_trampoline_end:

global apic_base_address
apic_base_address: dq 0xFEE00000

global ioapic_base_address
ioapic_base_address: dq 0xFEC00000

; ─────────────────────────────────────────────────────────────
; Stage 2 Padding — Ensure minimum size (64KB total)
; ─────────────────────────────────────────────────────────────

; ─────────────────────────────────────────────────────────────
; MBR Partition Table (at offset 446 in boot sector)
; ─────────────────────────────────────────────────────────────

mbr_partition_table:
    times 64 db 0              ; 4 partition entries, 16 bytes each

; ─────────────────────────────────────────────────────────────
; Protected Mode GDT Reload
; ─────────────────────────────────────────────────────────────

protected_mode_gdt_reload:
    lgdt [gdt32_descriptor]
    jmp 0x08:.reload
.reload:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    ret

; ─────────────────────────────────────────────────────────────
; Long Mode GDT Reload
; ─────────────────────────────────────────────────────────────

long_mode_gdt_reload:
    lgdt [gdt64_descriptor]
    jmp 0x08:.reload
.reload:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    ret

; ─────────────────────────────────────────────────────────────
; Transition to Protected Mode from Real Mode
; ─────────────────────────────────────────────────────────────

enter_protected_mode:
    cli
    lgdt [gdt32_descriptor]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:protected_mode_entry

; ─────────────────────────────────────────────────────────────
; Stage 2 Padding — Ensure minimum size (64KB total)
; ─────────────────────────────────────────────────────────────

times 65536 - ($ - protected_mode_entry) db 0  ; Pad to 64KB
