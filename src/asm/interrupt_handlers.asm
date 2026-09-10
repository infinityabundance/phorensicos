; interrupt_handlers.asm — IDT setup, interrupt service routines, trap handling
; Phorensic OS — Trusted Nucleus Interrupt Subsystem
;
; File: src/asm/interrupt_handlers.asm
; Architecture: x86_64
; Court: interrupt_asm:v2
; Audit: Line-audited by Nucleus Audit Team

[BITS 64]
[ORG 0x0]  ; Relocatable — linked at nucleus base + offset

; ─────────────────────────────────────────────────────────────
; Constants
; ─────────────────────────────────────────────────────────────

IDT_ENTRIES         equ 256
IDT_ENTRY_SIZE      equ 16
IDT_SIZE            equ IDT_ENTRIES * IDT_ENTRY_SIZE

KERNEL_CS           equ 0x08          ; Kernel code segment selector
KERNEL_DS           equ 0x10          ; Kernel data segment selector
NUCLEUS_CS          equ 0x08          ; Nucleus uses same ring 0 code segment

IST_INDEX_DOUBLE_FAULT equ 1          ; IST index for double fault
IST_INDEX_NMI          equ 2          ; IST index for NMI
IST_INDEX_PF           equ 3          ; IST index for page fault

; ─────────────────────────────────────────────────────────────
; Macros
; ─────────────────────────────────────────────────────────────

; Save all general-purpose registers
%macro save_context 0
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rbp, rsp        ; Save frame pointer
%endmacro

; Restore all general-purpose registers
%macro restore_context 0
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
%endmacro

; Define an ISR without error code
%macro ISR_NO_ERRCODE 1
align 8
isr_%1:
    ; Push dummy error code (for stack consistency)
    push 0
    push %1             ; Push vector number
    jmp isr_common_stub
%endmacro

; Define an ISR with error code
%macro ISR_ERRCODE 1
align 8
isr_%1:
    ; Error code already pushed by CPU
    push %1             ; Push vector number
    jmp isr_common_stub
%endmacro

; Define an IRQ handler
%macro IRQ_HANDLER 2
align 8
irq_%1:
    push 0              ; Dummy error code
    push %2             ; Vector number (IRQ base + offset)
    jmp isr_common_stub
%endmacro

; ─────────────────────────────────────────────────────────────
; CPU Exception Handlers (Vectors 0-31)
; ─────────────────────────────────────────────────────────────

ISR_NO_ERRCODE 0       ; Division by Zero
ISR_NO_ERRCODE 1       ; Debug
ISR_NO_ERRCODE 2       ; Non-Maskable Interrupt
ISR_NO_ERRCODE 3       ; Breakpoint
ISR_NO_ERRCODE 4       ; Overflow
ISR_NO_ERRCODE 5       ; Bound Range
ISR_NO_ERRCODE 6       ; Invalid Opcode
ISR_NO_ERRCODE 7       ; Device Not Available
ISR_ERRCODE   8        ; Double Fault (with IST)
ISR_NO_ERRCODE 9       ; Coprocessor Segment Overrun
ISR_ERRCODE   10       ; Invalid TSS
ISR_ERRCODE   11       ; Segment Not Present
ISR_ERRCODE   12       ; Stack-Segment Fault
ISR_ERRCODE   13       ; General Protection Fault
ISR_ERRCODE   14       ; Page Fault
ISR_NO_ERRCODE 15      ; Reserved
ISR_NO_ERRCODE 16      ; x87 FPU Error
ISR_ERRCODE   17       ; Alignment Check
ISR_NO_ERRCODE 18      ; Machine Check
ISR_NO_ERRCODE 19      ; SIMD Floating-Point Exception
ISR_NO_ERRCODE 20      ; Virtualization Exception
ISR_NO_ERRCODE 21      ; Control Protection Exception
ISR_NO_ERRCODE 22      ; Reserved
ISR_NO_ERRCODE 23      ; Reserved
ISR_NO_ERRCODE 24      ; Reserved
ISR_NO_ERRCODE 25      ; Reserved
ISR_NO_ERRCODE 26      ; Reserved
ISR_NO_ERRCODE 27      ; Reserved
ISR_NO_ERRCODE 28      ; Reserved
ISR_NO_ERRCODE 29      ; Reserved
ISR_ERRCODE   30       ; Security Exception
ISR_NO_ERRCODE 31      ; Reserved

; ─────────────────────────────────────────────────────────────
; Hardware Interrupt Handlers (IRQs 0-15 → Vectors 32-47)
; ─────────────────────────────────────────────────────────────

IRQ_HANDLER 0,  32     ; PIT / Timer
IRQ_HANDLER 1,  33     ; Keyboard
IRQ_HANDLER 2,  34     ; Cascade
IRQ_HANDLER 3,  35     ; COM2
IRQ_HANDLER 4,  36     ; COM1 (Serial)
IRQ_HANDLER 5,  37     ; LPT2 / Sound
IRQ_HANDLER 6,  38     ; Floppy
IRQ_HANDLER 7,  39     ; LPT1 / Spurious
IRQ_HANDLER 8,  40     ; CMOS RTC
IRQ_HANDLER 9,  41     ; Free / ACPI
IRQ_HANDLER 10, 42     ; Free
IRQ_HANDLER 11, 43     ; Free
IRQ_HANDLER 12, 44     ; PS/2 Mouse
IRQ_HANDLER 13, 45     ; FPU
IRQ_HANDLER 14, 46     ; Primary ATA
IRQ_HANDLER 15, 47     ; Secondary ATA

; Additional vectors for MSI/MSI-X (48-255)
; These are populated dynamically by device drivers
; Pre-allocate stubs for vectors 48-63
%assign vec 48
%rep 16
IRQ_HANDLER vec, vec
%assign vec vec + 1
%endrep

; ─────────────────────────────────────────────────────────────
; Common ISR Stub
; ─────────────────────────────────────────────────────────────

align 8
isr_common_stub:
    ; Save all general registers (vector and error already pushed)
    save_context

    ; Stack layout at this point:
    ;   rsp+0:  r15 (last pushed by save_context)
    ;   ...
    ;   rsp+120: vector number
    ;   rsp+128: error code
    ;   rsp+136: RIP
    ;   rsp+144: CS
    ;   rsp+152: RFLAGS
    ;   rsp+160: RSP (if privilege change)
    ;   rsp+168: SS (if privilege change)

    ; Get vector number from stack
    mov rdi, [rsp + 120]    ; Vector number (first arg to handler)
    mov rsi, rsp            ; Register frame pointer (second arg)

    ; Check if we came from user mode
    mov ax, [rsp + 144]     ; CS value
    and ax, 0x03            ; Check CPL bits
    mov rdx, rax            ; CPL (third arg)

    ; Switch to kernel GS.base (for per-CPU data)
    swapgs

    ; Call the nucleus interrupt dispatcher
    ; The dispatcher will look up the registered handler and dispatch
    extern nucleus_interrupt_dispatch
    call nucleus_interrupt_dispatch

    ; Restore GS.base
    swapgs

    ; Check if reschedule is needed (return value in rax)
    cmp rax, 1
    jne .no_reschedule

    ; Reschedule requested — call scheduler
    extern scheduler_check_preempt
    call scheduler_check_preempt

.no_reschedule:
    ; Restore registers
    restore_context

    ; Remove vector number and error code from stack
    add rsp, 16

    ; Return from interrupt
    iretq

; ─────────────────────────────────────────────────────────────
; Special: Double Fault Handler (IST-based stack switch)
; ─────────────────────────────────────────────────────────────

align 8
global double_fault_handler
double_fault_handler:
    ; Save minimal context
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp

    ; Get error code (already on stack from CPU)
    mov rdi, [rsp + 56]     ; Error code
    mov rsi, rsp            ; Frame pointer

    ; Call double fault handler
    extern nucleus_double_fault
    call nucleus_double_fault

    ; Double fault should not return — if it does, halt
.halt:
    hlt
    jmp .halt

; ─────────────────────────────────────────────────────────────
; Special: NMI Handler (IST-based)
; ─────────────────────────────────────────────────────────────

align 8
global nmi_handler
nmi_handler:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp

    mov rdi, 2              ; NMI vector
    mov rsi, rsp

    extern nucleus_nmi_handler
    call nucleus_nmi_handler

    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    iretq

; ─────────────────────────────────────────────────────────────
; Special: Page Fault Handler
; ─────────────────────────────────────────────────────────────

align 8
global page_fault_handler
page_fault_handler:
    save_context

    ; Read CR2 (faulting address)
    mov rdi, cr2            ; Fault address (first arg)
    mov rsi, [rsp + 120]    ; Error code (second arg)
    mov rdx, [rsp + 136]    ; RIP (third arg)

    ; Call page fault handler
    extern nucleus_page_fault
    call nucleus_page_fault

    ; If handler returned 0, skip offending instruction
    cmp rax, 0
    je .skip

    restore_context
    add rsp, 16
    iretq

.skip:
    ; Cannot handle — panic
    mov rdi, cr2
    mov rsi, [rsp + 120]
    extern nucleus_panic_page_fault
    call nucleus_panic_page_fault

.halt:
    hlt
    jmp .halt

; ─────────────────────────────────────────────────────────────
; Special: Spurious Interrupt Handler
; ─────────────────────────────────────────────────────────────

align 8
global spurious_interrupt_handler
spurious_interrupt_handler:
    push rax

    ; Read ISR to check if this is a real interrupt
    mov al, 0x0B            ; OCW3: read ISR
    out 0x20, al
    in al, 0x20
    test al, al
    jnz .real_interrupt

    ; Spurious — just return without EOI
    pop rax
    iretq

.real_interrupt:
    ; Real interrupt — proceed normally
    pop rax
    push 0                  ; Dummy error code
    push 0xFF               ; Spurious vector marker
    jmp isr_common_stub

; ─────────────────────────────────────────────────────────────
; Syscall Entry (via SYSCALL/SYSENTER)
; ─────────────────────────────────────────────────────────────

align 8
global syscall_entry
syscall_entry:
    ; Swap to kernel GS.base
    swapgs

    ; Save user context on kernel stack
    mov [rsp - 8], rcx     ; Save return RIP (RCX = RIP after SYSCALL)
    mov [rsp - 16], r11    ; Save RFLAGS (R11 = RFLAGS after SYSCALL)

    ; Switch to kernel stack
    mov rcx, rsp
    mov rsp, [gs:0x08]     ; Kernel stack pointer from per-CPU data

    ; Push user registers
    push r15
    push r14
    push r13
    push r12
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx                ; User stack pointer
    push rbx

    ; Save user return RIP and RFLAGS
    push qword [rcx - 16]   ; RFLAGS
    push qword [rcx - 8]    ; RIP

    ; Arguments: rdi = syscall number, rsi = arg1, rdx = arg2, r10 = arg3, r8 = arg4, r9 = arg5
    mov rdi, rax            ; Syscall number
    mov rsi, rdi            ; Original RDI
    mov rdx, rsi            ; Original RSI
    mov rcx, rdx            ; Original RDX
    mov r8, r10             ; Original R10
    mov r9, r8              ; Original R8

    ; Call the kernel syscall dispatcher
    extern kernel_syscall_dispatch
    call kernel_syscall_dispatch

    ; Restore registers and return
    add rsp, 16             ; Remove saved RIP and RFLAGS
    pop rbx
    pop rcx                 ; User stack pointer (skip)
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r12
    pop r13
    pop r14
    pop r15

    ; Switch back to user stack
    mov rsp, rcx

    ; Restore user context
    mov rcx, [rsp - 8]     ; Restore RIP
    mov r11, [rsp - 16]    ; Restore RFLAGS

    swapgs

    ; Return to user mode via SYSRET
    ; rax contains return value from kernel dispatcher
    sysretq

; ─────────────────────────────────────────────────────────────
; IDT Structure and Initialization
; ─────────────────────────────────────────────────────────────

section .data
align 16

; Interrupt Descriptor Table (256 entries * 16 bytes = 4KB)
global idt
idt:
    times IDT_SIZE db 0

; IDT Descriptor for LIDT instruction
global idt_descriptor
idt_descriptor:
    dw IDT_SIZE - 1         ; Limit
    dq idt                  ; Base address

; ─────────────────────────────────────────────────────────────
; IDT Installation — Call from nucleus initialization
; ─────────────────────────────────────────────────────────────

section .text

global install_idt
install_idt:
    push rbp
    mov rbp, rsp

    ; Install exception handlers (vectors 0-31)
    extern install_exception_handlers
    call install_exception_handlers

    ; Install IRQ handlers (vectors 32-47)
    extern install_irq_handlers
    call install_irq_handlers

    ; Set up IST stacks for critical handlers
    extern setup_ist_stacks
    call setup_ist_stacks

    ; Load the IDT
    lidt [idt_descriptor]

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Install a Single IDT Entry
; ─────────────────────────────────────────────────────────────

; rdi = vector number
; rsi = handler address
; rdx = segment selector (usually KERNEL_CS)
; rcx = flags (present, DPL, gate type)
global idt_set_entry
idt_set_entry:
    push rbp
    mov rbp, rsp

    ; Calculate entry offset
    mov rax, rdi
    shl rax, 4              ; Multiply by 16 (entry size)
    lea r8, [idt + rax]     ; r8 = &idt[vector]

    ; Set low 16 bits of handler address
    mov [r8], si            ; Handler address bits 0-15

    ; Set segment selector
    mov [r8 + 2], dx        ; Segment selector

    ; Set IST index (bits 0-2 of byte 4)
    mov byte [r8 + 4], 0    ; IST = 0 for now

    ; Set flags (byte 5)
    mov [r8 + 5], cl        ; Flags: gate type, DPL, present

    ; Set high 16 bits of handler address
    shr rsi, 16
    mov [r8 + 6], si        ; Handler address bits 16-31

    ; Set bits 32-63 of handler address
    shr rsi, 16
    mov [r8 + 8], rsi       ; Handler address bits 32-47
    shr rsi, 16
    mov [r8 + 12], si       ; Handler address bits 48-63

    ; Set reserved bits (bytes 10-11)
    mov dword [r8 + 10], 0

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Install Exception Handlers (Vectors 0-31)
; ─────────────────────────────────────────────────────────────

global install_exception_handlers
install_exception_handlers:
    push rbp
    mov rbp, rsp

    ; Set up each exception handler with proper flags
    ; Flags: 0x8E = present, ring 0, interrupt gate (32-bit)
    ; For long mode: 0x8E = present, ring 0, interrupt gate (64-bit)

    mov rdx, KERNEL_CS
    mov cl, 0x8E            ; Present | Ring 0 | Interrupt Gate (64-bit)

    %assign vec 0
    %rep 32
        mov rdi, vec
        mov rsi, isr_%[vec]
        call idt_set_entry
    %assign vec vec + 1
    %endrep

    ; Special: double fault gets IST
    mov rdi, 8
    mov rsi, double_fault_handler
    mov rdx, KERNEL_CS
    mov cl, 0x8E
    call idt_set_entry

    ; Update double fault entry for IST
    mov rdi, 8
    mov byte [idt + 8*16 + 4], IST_INDEX_DOUBLE_FAULT

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Install IRQ Handlers (Vectors 32-47)
; ─────────────────────────────────────────────────────────────

global install_irq_handlers
install_irq_handlers:
    push rbp
    mov rbp, rsp

    mov rdx, KERNEL_CS
    mov cl, 0x8E            ; Present | Ring 0 | Interrupt Gate

    %assign irq 0
    %rep 16
        mov rdi, 32 + irq
        mov rsi, irq_%[irq]
        call idt_set_entry
    %assign irq irq + 1
    %endrep

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Set Up IST Stacks
; ─────────────────────────────────────────────────────────────

section .bss
align 4096

; IST stacks (each 4KB, with guard page)
ist_stack_double_fault:
    resb 4096
ist_stack_double_fault_top:

ist_stack_nmi:
    resb 4096
ist_stack_nmi_top:

ist_stack_page_fault:
    resb 4096
ist_stack_page_fault_top:

section .text

global setup_ist_stacks
setup_ist_stacks:
    push rbp
    mov rbp, rsp

    ; Set IST entries in TSS
    ; TSS is managed by the kernel, we set the stack pointers here
    extern tss_set_ist
    mov rdi, IST_INDEX_DOUBLE_FAULT
    mov rsi, ist_stack_double_fault_top
    call tss_set_ist

    mov rdi, IST_INDEX_NMI
    mov rsi, ist_stack_nmi_top
    call tss_set_ist

    mov rdi, IST_INDEX_PF
    mov rsi, ist_stack_page_fault_top
    call tss_set_ist

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Enable/Disable Interrupts
; ─────────────────────────────────────────────────────────────

global enable_interrupts
enable_interrupts:
    sti
    ret

global disable_interrupts
disable_interrupts:
    cli
    ret

; ─────────────────────────────────────────────────────────────
; Send End-of-Interrupt to PICs
; ─────────────────────────────────────────────────────────────

global pic_send_eoi
pic_send_eoi:
    ; rdi = IRQ number
    push rbp
    mov rbp, rsp

    cmp rdi, 8
    jl .slave_done

    ; Send EOI to slave PIC
    mov al, 0x20
    out 0xA0, al

    sub rdi, 8

.slave_done:
    ; Send EOI to master PIC
    mov al, 0x20
    out 0x20, al

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Get Current Interrupt State
; ─────────────────────────────────────────────────────────────

global get_interrupt_state
get_interrupt_state:
    pushfq
    pop rax
    and rax, 0x200          ; IF bit
    shr rax, 9
    ret

; ─────────────────────────────────────────────────────────────
; Get Faulting Address (CR2)
; ─────────────────────────────────────────────────────────────

global read_cr2
read_cr2:
    mov rax, cr2
    ret

; ─────────────────────────────────────────────────────────────
; Halt CPU
; ─────────────────────────────────────────────────────────────

global halt_cpu
halt_cpu:
    hlt
    ret

global halt_cpu_forever
halt_cpu_forever:
.loop:
    hlt
    jmp .loop

; ─────────────────────────────────────────────────────────────
; APIC Local Vector Table Management
; ─────────────────────────────────────────────────────────────

global apic_write_lvt
apic_write_lvt:
    ; rdi = APIC register offset
    ; rsi = value to write
    push rbp
    mov rbp, rsp

    ; Compute APIC base (from IA32_APIC_BASE MSR)
    mov rcx, 0x0000001B       ; IA32_APIC_BASE MSR
    rdmsr
    and eax, 0xFFFFF000        ; Mask to page-aligned base
    mov r8, rax               ; r8 = APIC base

    ; Write to LVT register
    add r8, rdi
    mov [r8], esi

    ; Read back to ensure completion
    mov eax, [r8]

    pop rbp
    ret

global apic_read_lvt
apic_read_lvt:
    ; rdi = APIC register offset
    push rbp
    mov rbp, rsp

    mov rcx, 0x0000001B
    rdmsr
    and eax, 0xFFFFF000
    add rax, rdi
    mov rax, [rax]

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; I/O APIC Redirection Entry Management
; ─────────────────────────────────────────────────────────────

global ioapic_write_redir_entry
ioapic_write_redir_entry:
    ; rdi = I/O APIC base address
    ; rsi = redirection entry index
    ; rdx = value low (bits 0-31)
    ; rcx = value high (bits 32-63)
    push rbp
    mov rbp, rsp

    ; Write low dword: IOREDTBL[index]
    mov rax, rsi
    shl rax, 1                ; index * 2
    add rax, 0x10             ; IOREDTBL base = 0x10
    mov [rdi + 0x00], eax      ; IOREGSEL = offset
    mov [rdi + 0x10], edx      ; IOREGWIN = low value

    ; Write high dword: IOREDTBL[index] + 1
    inc rax                    ; Next register
    mov [rdi + 0x00], eax
    mov [rdi + 0x10], ecx

    pop rbp
    ret

global ioapic_read_redir_entry
ioapic_read_redir_entry:
    ; rdi = I/O APIC base address
    ; rsi = redirection entry index
    ; Returns: rax = value (low 32 bits), rdx = value (high 32 bits)
    push rbp
    mov rbp, rsp

    mov rax, rsi
    shl rax, 1
    add rax, 0x10
    mov [rdi + 0x00], eax
    mov rdx, [rdi + 0x10]

    inc rax
    mov [rdi + 0x00], eax
    mov rcx, [rdi + 0x10]
    shl rcx, 32
    or rax, rcx

    mov rdx, rcx
    shr rdx, 32

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; TSS Management
; ─────────────────────────────────────────────────────────────

section .data
global tss
tss:
    times 104 db 0             ; TSS is 104 bytes

global tss_descriptor
tss_descriptor:
    ; Constructed dynamically at runtime
    dw 0x0067                 ; Limit = 104 - 1
    dw 0                      ; Base low
    db 0                      ; Base mid
    db 0x89                   ; Present, 64-bit TSS (available)
    db 0x40                   ; Granularity
    db 0                      ; Base high
    dd 0                      ; Base upper (bits 32-63)
    dd 0                      ; Reserved

section .text

global tss_set_ist
tss_set_ist:
    ; rdi = IST index (1-7)
    ; rsi = stack pointer
    push rbp
    mov rbp, rsp

    ; TSS IST offsets: byte offset = 36 + (index * 8)
    mov rax, rdi
    dec rax                    ; IST indices start at 1, offset at index 0
    shl rax, 3                ; Multiply by 8
    add rax, 36               ; IST1 offset in TSS

    ; Set the IST entry in the TSS
    mov [tss + rax], rsi

    pop rbp
    ret

global tss_load
tss_load:
    push rbp
    mov rbp, rsp

    ; Update TSS descriptor with actual base
    mov rax, tss
    mov word [tss_descriptor + 2], ax   ; Base low 16 bits
    shr rax, 16
    mov byte [tss_descriptor + 4], al   ; Base mid 8 bits
    shr rax, 8
    mov byte [tss_descriptor + 7], al   ; Base high 8 bits
    shr rax, 8
    mov dword [tss_descriptor + 8], eax ; Base upper 32 bits

    ; LTR instruction
    mov ax, 0x28               ; TSS segment selector (5th entry in GDT)
    ltr ax

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Interrupt Remapping (PIC → APIC mode)
; ─────────────────────────────────────────────────────────────

global pic_remap
pic_remap:
    ; Remap PIC vectors away from CPU exceptions
    ; Master PIC: vectors 32-39
    ; Slave PIC: vectors 40-47
    push rbp
    mov rbp, rsp

    ; Initialize PICs with ICW1
    mov al, 0x11               ; ICW1: edge triggered, cascade, ICW4 needed
    out 0x20, al               ; Master PIC
    out 0xA0, al               ; Slave PIC

    ; ICW2: vector offsets
    mov al, 0x20               ; Master: vectors 32-39
    out 0x21, al
    mov al, 0x28               ; Slave: vectors 40-47
    out 0xA1, al

    ; ICW3: cascading
    mov al, 0x04               ; Master: slave on IRQ2
    out 0x21, al
    mov al, 0x02               ; Slave: cascade ID 2
    out 0xA1, al

    ; ICW4: 8086 mode, normal EOI
    mov al, 0x01
    out 0x21, al
    out 0xA1, al

    ; Mask all interrupts
    mov al, 0xFF
    out 0x21, al
    out 0xA1, al

    pop rbp
    ret

global pic_mask_irq
pic_mask_irq:
    ; rdi = IRQ number (0-15)
    push rbp
    mov rbp, rsp

    cmp rdi, 8
    jl .master

    ; Slave PIC
    sub rdi, 8
    in al, 0xA1
    or al, 1 << rdi           ; Set mask bit
    out 0xA1, al
    jmp .done

.master:
    in al, 0x21
    or al, 1 << rdi
    out 0x21, al

.done:
    pop rbp
    ret

global pic_unmask_irq
pic_unmask_irq:
    ; rdi = IRQ number (0-15)
    push rbp
    mov rbp, rsp

    cmp rdi, 8
    jl .master

    sub rdi, 8
    in al, 0xA1
    and al, ~(1 << rdi)
    out 0xA1, al
    jmp .done

.master:
    in al, 0x21
    and al, ~(1 << rdi)
    out 0x21, al

.done:
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Performance Counter Access
; ─────────────────────────────────────────────────────────────

global read_pmc
read_pmc:
    ; rdi = performance counter index (ECX)
    push rbp
    mov rbp, rsp
    mov rcx, rdi
    rdpmc
    shl rdx, 32
    or rax, rdx
    pop rbp
    ret

global read_tsc
read_tsc:
    push rbp
    mov rbp, rsp
    rdtsc
    shl rdx, 32
    or rax, rdx
    pop rbp
    ret

global read_tscp
read_tscp:
    push rbp
    mov rbp, rsp
    rdtscp
    shl rdx, 32
    or rax, rdx
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; CPU Feature Detection (CPUID wrappers)
; ─────────────────────────────────────────────────────────────

global cpuid_query
cpuid_query:
    ; rdi = leaf (EAX input)
    ; rsi = subleaf (ECX input)
    push rbp
    mov rbp, rsp

    mov eax, edi
    mov ecx, esi
    cpuid

    ; Return values: rax = EAX, rbx = EBX, rcx = ECX, rdx = EDX
    ; The calling convention already returns rax = EAX
    ; For full results, caller uses register variables

    pop rbp
    ret

global read_msr
read_msr:
    ; rdi = MSR number
    push rbp
    mov rbp, rsp

    mov rcx, rdi
    rdmsr
    shl rdx, 32
    or rax, rdx

    pop rbp
    ret

global write_msr
write_msr:
    ; rdi = MSR number
    ; rsi = value
    push rbp
    mov rbp, rsp

    mov rcx, rdi
    mov rax, rsi
    shr rsi, 32
    mov rdx, rsi
    wrmsr

    pop rbp
    ret
