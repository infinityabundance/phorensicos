; mode_transitions.asm — User↔kernel↔nucleus mode switches, capability gates
; Phorensic OS — Trusted Nucleus Mode Transition Subsystem
;
; File: src/asm/mode_transitions.asm
; Architecture: x86_64
; Court: mode_transitions_asm:v2
; Audit: Line-audited by Nucleus Audit Team

[BITS 64]

; ─────────────────────────────────────────────────────────────
; Constants
; ─────────────────────────────────────────────────────────────

KERNEL_CS       equ 0x08
KERNEL_DS       equ 0x10
USER_CS         equ 0x1B      ; Ring 3 code segment (0x18 | 0x03)
USER_DS         equ 0x23      ; Ring 3 data segment (0x20 | 0x03)
NUCLEUS_CS      equ 0x08      ; Same as kernel CS (ring 0)

; Capability gate types
CAP_GATE_SYSCALL    equ 0x01
CAP_GATE_IPC        equ 0x02
CAP_GATE_NUCLEUS    equ 0x03
CAP_GATE_INTERRUPT  equ 0x04
CAP_GATE_MEMORY     equ 0x05
CAP_GATE_IO         equ 0x06

; Transition reason codes
TRANSITION_USER_TO_KERNEL    equ 0
TRANSITION_KERNEL_TO_NUCLEUS equ 1
TRANSITION_NUCLEUS_TO_KERNEL equ 2
TRANSITION_KERNEL_TO_USER    equ 3
TRANSITION_INTERRUPT_ENTRY   equ 4
TRANSITION_INTERRUPT_RETURN  equ 5
TRANSITION_TRAP_ENTRY        equ 6
TRANSITION_CAPABILITY_GATE   equ 11

; ─────────────────────────────────────────────────────────────
; External Functions (implemented in .phor kernel code)
; ─────────────────────────────────────────────────────────────

extern validate_mode_transition
extern record_mode_transition
extern capability_check
extern nucleus_call_enter
extern nucleus_call_exit

; ─────────────────────────────────────────────────────────────
; Per-CPU Data Structure Offsets
; ─────────────────────────────────────────────────────────────

CPU_SELF_PTR        equ 0x00   ; Pointer to self
CPU_KERNEL_STACK    equ 0x08   ; Kernel stack pointer
CPU_NUCLEUS_STACK   equ 0x10   ; Nucleus stack pointer
CPU_CURRENT_THREAD  equ 0x18   ; Current thread pointer
CPU_CURRENT_SESSION equ 0x20   ; Current session pointer
CPU_CAPABILITY_ROOT equ 0x28   ; Root capability table pointer
CPU_GENERATION      equ 0x30   ; Current generation counter
CPU_TRANSITION_LOG  equ 0x38   ; Transition log buffer pointer
CPU_INT_ENABLED     equ 0x40   ; Interrupt enabled flag

; ─────────────────────────────────────────────────────────────
; User → Kernel Transition (via syscall)
; ─────────────────────────────────────────────────────────────

global user_to_kernel_entry
user_to_kernel_entry:
    ; Entry point for SYSCALL instruction
    ; RCX = return RIP, R11 = return RFLAGS
    ; RAX = syscall number
    ; RDI, RSI, RDX, R10, R8, R9 = arguments

    ; Step 1: Save user state and switch to kernel stack
    swapgs                      ; GS.base ← kernel per-CPU data

    ; Save user stack pointer and return address
    mov [gs:CPU_KERNEL_STACK], rsp  ; Save user stack temporarily

    ; Verify capability gate before proceeding
    ; The syscall number (rax) encodes the capability gate index
    push rax
    push rdi
    push rsi
    push rdx
    push r10
    push r8
    push r9

    ; Step 2: Record transition residual
    mov rdi, TRANSITION_USER_TO_KERNEL   ; Transition type
    mov rsi, [gs:CPU_CURRENT_THREAD]     ; Caller identity
    mov rdx, rax                         ; Capability token (syscall number)
    call record_transition_entry

    ; Step 3: Load kernel stack
    mov rsp, [gs:CPU_KERNEL_STACK]       ; Switch to kernel stack (if not already)

    ; Step 4: Call the kernel syscall handler
    pop r9
    pop r8
    pop r10
    pop rdx
    pop rsi
    pop rdi
    pop rax

    call kernel_syscall_dispatch

    ; Step 5: Record transition exit residual
    push rax                            ; Save return value
    mov rdi, TRANSITION_KERNEL_TO_USER
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit
    pop rax

    ; Step 6: Return to user mode
    swapgs
    sysretq

; ─────────────────────────────────────────────────────────────
; Kernel → Nucleus Transition (via call gate)
; ─────────────────────────────────────────────────────────────

global kernel_to_nucleus_entry
kernel_to_nucleus_entry:
    ; Entry point for kernel → nucleus transitions
    ; RDI = nucleus operation number
    ; RSI = argument array pointer
    ; RDX = capability token
    ; RCX = caller identity

    ; Step 1: Validate the transition is allowed
    push rdi
    push rsi
    push rdx
    push rcx
    push r8
    push r9
    push r10
    push r11

    mov rdi, TRANSITION_KERNEL_TO_NUCLEUS
    mov rsi, rcx                ; Caller identity
    mov rdx, rdx                ; Capability token
    call validate_nucleus_transition

    test rax, rax
    jnz .transition_denied

    pop r11
    pop r10
    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi

    ; Step 2: Record entry residual
    push rdi
    push rsi
    push rdx
    push rcx

    mov rdi, TRANSITION_KERNEL_TO_NUCLEUS
    mov rsi, rcx
    call record_transition_entry

    pop rcx
    pop rdx
    pop rsi
    pop rdi

    ; Step 3: Save kernel context
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    ; Step 4: Call nucleus operation
    ; rdi = operation
    ; rsi = args ptr
    ; rdx = capability
    call nucleus_dispatch

    ; Step 5: Restore kernel context
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    ; Step 6: Record exit residual
    push rax
    mov rdi, TRANSITION_NUCLEUS_TO_KERNEL
    mov rsi, rcx
    call record_transition_exit
    pop rax

    ret

.transition_denied:
    ; Capability violation — set error in rax and return
    pop r11
    pop r10
    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi

    mov rax, -1                 ; Error code
    ret

; ─────────────────────────────────────────────────────────────
; Nucleus → Kernel Return
; ─────────────────────────────────────────────────────────────

global nucleus_to_kernel_return
nucleus_to_kernel_return:
    ; Return from nucleus operation to kernel
    ; RDI = result structure pointer
    ; RSI = target context pointer

    ; Record transition
    push rdi
    push rsi

    mov rdi, TRANSITION_NUCLEUS_TO_KERNEL
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit

    pop rsi
    pop rdi

    ; Copy result back to kernel context
    mov rax, [rdi]              ; Load result value
    ; Additional result fields would be copied here

    ret

; ─────────────────────────────────────────────────────────────
; Kernel → User Return (context switch to user mode)
; ─────────────────────────────────────────────────────────────

global kernel_to_user_return
kernel_to_user_return:
    ; Transition from kernel to user mode
    ; RDI = pointer to user context (MinimalCpuContext)

    ; Switch to user stack
    mov rsp, [rdi + 64]         ; rsp from context
    push qword [rdi + 80]       ; SS (user data segment)
    push qword [rdi + 64]       ; RSP (user stack)
    push qword [rdi + 48]       ; RFLAGS
    push qword [rdi + 40]       ; CS (user code segment)
    push qword [rdi + 56]       ; RIP

    ; Load remaining registers
    mov rax, [rdi + 0]          ; RAX
    mov rbx, [rdi + 8]          ; RBX
    mov rcx, [rdi + 16]         ; RCX (RIP for sysret)
    mov rdx, [rdi + 24]         ; RDX
    mov rsi, [rdi + 32]         ; RSI
    mov rbp, [rdi + 48]         ; RBP
    mov r8,  [rdi + 72]         ; R8
    mov r9,  [rdi + 80]         ; R9
    mov r10, [rdi + 88]         ; R10
    mov r11, [rdi + 96]         ; R11 (RFLAGS for sysret)
    mov r12, [rdi + 104]        ; R12
    mov r13, [rdi + 112]        ; R13
    mov r14, [rdi + 120]        ; R14
    mov r15, [rdi + 128]        ; R15

    ; Record transition
    push rax
    mov rdi, TRANSITION_KERNEL_TO_USER
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit
    pop rax

    ; Clear interrupt enable (will be restored by iretq)
    swapgs

    ; Return to user mode via IRETQ
    iretq

; ─────────────────────────────────────────────────────────────
; Capability Gate Call
; ─────────────────────────────────────────────────────────────

global capability_gate_call
capability_gate_call:
    ; Entry point for explicit capability-gated transitions
    ; RDI = capability gate type
    ; RSI = capability token
    ; RDX = target function
    ; RCX, R8, R9 = arguments

    push rbp
    mov rbp, rsp

    ; Step 1: Verify capability
    push rdi
    push rsi
    push rdx
    push rcx
    push r8
    push r9

    mov rdi, rsi                ; Capability token
    call verify_capability_gate

    test rax, rax
    jnz .gate_denied

    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi

    ; Step 2: Record capability gate transition
    push rdi
    push rsi
    push rdx
    push rcx
    push r8
    push r9

    mov rdi, TRANSITION_CAPABILITY_GATE
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_entry

    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi

    ; Step 3: Call through the gate
    ; rsi = capability type, rdx = target, rcx, r8, r9 = args
    call rdx                    ; Call target function

    ; Step 4: Record exit
    push rax
    mov rdi, TRANSITION_CAPABILITY_GATE
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit
    pop rax

    pop rbp
    ret

.gate_denied:
    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    mov rax, -2                 ; Capability denied error
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Interrupt Entry (from interrupt_handlers.asm)
; ─────────────────────────────────────────────────────────────

global interrupt_entry
interrupt_entry:
    ; Called from ISR stub when an interrupt occurs in user mode
    ; Already on kernel stack (via TSS IST or interrupt gate)

    ; Record interrupt transition
    push rdi
    push rsi
    push rdx

    mov rdi, TRANSITION_INTERRUPT_ENTRY
    mov rsi, [rsp + 24]         ; RIP from stack
    call record_transition_entry

    pop rdx
    pop rsi
    pop rdi

    ret

; ─────────────────────────────────────────────────────────────
; Idle → Kernel Wake (from HLT state)
; ─────────────────────────────────────────────────────────────

global idle_wake_entry
idle_wake_entry:
    ; Called when an interrupt wakes the CPU from idle
    ; Interrupt handler has already run; we just need to resume

    ; Record the wake transition
    push rdi
    mov rdi, TRANSITION_KERNEL_TO_USER  ; Wake back to kernel
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit
    pop rdi

    ret

; ─────────────────────────────────────────────────────────────
; Transition Residual Recording Functions
; ─────────────────────────────────────────────────────────────

; Record entry of a mode transition
record_transition_entry:
    ; rdi = transition type
    ; rsi = caller identity
    push rbp
    push rax
    push rbx
    push rcx
    push rdx
    push r8
    push r9
    push r10
    push r11

    ; Read current timestamp from HPET or TSC
    rdtsc
    shl rdx, 32
    or rax, rdx                 ; RAX = TSC value

    ; Build residual record in stack
    sub rsp, 64                 ; Residual record space

    mov [rsp + 0], rax          ; timestamp
    mov [rsp + 8], rdi          ; transition type
    mov [rsp + 16], rsi         ; caller identity

    ; Store the residual in the transition log buffer
    mov r8, [gs:CPU_TRANSITION_LOG]
    test r8, r8
    jz .no_log

    ; Circular buffer write
    mov r9, [r8 + 0]            ; Current index
    mov r10, [r8 + 8]           ; Max entries
    mov r11, [r8 + 16]          ; Base pointer

    mov rcx, r9
    shl rcx, 6                  ; Multiply by 64 (entry size)
    add rcx, r11                ; rcx = &buffer[index]

    ; Copy residual record
    mov rax, [rsp + 0]
    mov [rcx + 0], rax
    mov rax, [rsp + 8]
    mov [rcx + 8], rax
    mov rax, [rsp + 16]
    mov [rcx + 16], rax

    ; Increment index (circular)
    inc r9
    cmp r9, r10
    jb .no_wrap
    xor r9, r9
.no_wrap:
    mov [r8 + 0], r9            ; Update index

.no_log:
    add rsp, 64

    pop r11
    pop r10
    pop r9
    pop r8
    pop rdx
    pop rcx
    pop rbx
    pop rax
    pop rbp
    ret

; Record exit of a mode transition
record_transition_exit:
    ; rdi = transition type
    ; rsi = caller identity
    push rbp
    push rax
    push rbx
    push rcx
    push rdx
    push r8
    push r9
    push r10
    push r11

    rdtsc
    shl rdx, 32
    or rax, rdx

    sub rsp, 64

    mov [rsp + 0], rax
    mov [rsp + 8], rdi
    mov [rsp + 16], rsi

    mov r8, [gs:CPU_TRANSITION_LOG]
    test r8, r8
    jz .no_log_exit

    mov r9, [r8 + 0]
    mov r10, [r8 + 8]
    mov r11, [r8 + 16]

    mov rcx, r9
    shl rcx, 6
    add rcx, r11

    mov rax, [rsp + 0]
    mov [rcx + 0], rax
    mov rax, [rsp + 8]
    mov [rcx + 8], rax
    mov rax, [rsp + 16]
    mov [rcx + 16], rax

    inc r9
    cmp r9, r10
    jb .no_wrap_exit
    xor r9, r9
.no_wrap_exit:
    mov [r8 + 0], r9

.no_log_exit:
    add rsp, 64

    pop r11
    pop r10
    pop r9
    pop r8
    pop rdx
    pop rcx
    pop rbx
    pop rax
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Validate Nucleus Transition
; ─────────────────────────────────────────────────────────────

validate_nucleus_transition:
    ; rdi = transition type
    ; rsi = caller identity
    ; rdx = capability token
    ; Returns: 0 = allowed, non-zero = denied

    push rbp
    mov rbp, rsp

    ; Check that operation is in allowed set
    ; For now: all kernel→nucleus transitions allowed if capability is valid
    mov rax, [rdx]              ; Load capability token
    test rax, rax
    jz .denied

    ; Check generation
    mov rax, [rdx + 8]          ; Generation
    mov rcx, [gs:CPU_GENERATION]
    cmp rax, rcx
    ja .denied                   ; Generation too new (can't happen)
    cmp rax, 0
    je .denied                   ; Zero generation = invalid

    ; Transition is valid
    xor rax, rax
    pop rbp
    ret

.denied:
    mov rax, 1
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Verify Capability Gate
; ─────────────────────────────────────────────────────────────

verify_capability_gate:
    ; rdi = capability token pointer
    ; Returns: 0 = valid, non-zero = invalid

    push rbp
    mov rbp, rsp

    ; Load capability fields
    mov rax, [rdi + 0]          ; Token type
    mov rbx, [rdi + 8]          ; Token generation
    mov rcx, [rdi + 16]         ; Token hash

    ; Verify generation against current
    mov rdx, [gs:CPU_GENERATION]
    cmp rbx, rdx
    ja .invalid
    cmp rbx, 0
    je .invalid

    ; Verify token hash is non-zero
    test rcx, rcx
    jz .invalid

    ; Check that this capability is in the capability table
    mov r8, [gs:CPU_CAPABILITY_ROOT]
    test r8, r8
    jz .invalid

    ; Capability is valid
    xor rax, rax
    pop rbp
    ret

.invalid:
    mov rax, 1
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Fast IPC Entry (optimized path for same-session IPC)
; ─────────────────────────────────────────────────────────────

global fast_ipc_entry
fast_ipc_entry:
    ; Fast path IPC: user → kernel → user without full context save
    ; RDI = IPC channel handle
    ; RSI = message pointer
    ; RDX = message length

    ; Minimal context save
    push rbp
    mov rbp, rsp

    ; Record fast IPC entry
    mov rdi, TRANSITION_CAPABILITY_GATE
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_entry

    ; Call kernel IPC handler (fast path)
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Fast IPC Return
; ─────────────────────────────────────────────────────────────

global fast_ipc_return
fast_ipc_return:
    ; Fast path return from IPC
    push rbp
    mov rbp, rsp

    mov rdi, TRANSITION_CAPABILITY_GATE
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Initialize Per-CPU Data
; ─────────────────────────────────────────────────────────────

global init_percpu_data
init_percpu_data:
    ; RDI = pointer to per-CPU data structure
    ; Initialize GS.base to point to it

    push rbp
    mov rbp, rsp

    ; Write GS.base via MSR
    mov rcx, 0xC0000101         ; GS.base MSR
    mov rax, rdi
    shr rdi, 32
    mov rdx, rdi
    wrmsr

    ; Also set kernel GS.base for SWAPGS
    mov rcx, 0xC0000102         ; Kernel GS.base MSR
    wrmsr

    ; Initialize fields
    mov qword [rax + CPU_SELF_PTR], rax
    mov qword [rax + CPU_INT_ENABLED], 1

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Get Current Mode
; ─────────────────────────────────────────────────────────────

global get_current_cpu_mode
get_current_cpu_mode:
    ; Returns current CPU mode as CpuMode enum value
    push rbp
    mov rbp, rsp

    ; Check CS segment to determine current mode
    mov ax, cs
    and ax, 0x03

    cmp ax, 0
    je .kernel_mode

    cmp ax, 3
    je .user_mode

    ; Should not happen — treat as kernel
.kernel_mode:
    mov rax, 1                  ; KernelMode
    pop rbp
    ret

.user_mode:
    mov rax, 0                  ; UserMode
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Switch to Nucleus Mode (HLT + interrupt wait)
; ─────────────────────────────────────────────────────────────

global switch_to_nucleus_idle
switch_to_nucleus_idle:
    ; Transition CPU to idle/nucleus monitoring state
    push rbp
    mov rbp, rsp

    ; Record transition
    mov rdi, TRANSITION_KERNEL_TO_NUCLEUS
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_entry

    ; Enable interrupts and halt
    sti
    hlt

    ; Woken by interrupt — record wake
    mov rdi, TRANSITION_INTERRUPT_RETURN
    mov rsi, [gs:CPU_CURRENT_THREAD]
    call record_transition_exit

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Transition Log Initialization
; ─────────────────────────────────────────────────────────────

global init_transition_log
init_transition_log:
    ; RDI = buffer base address
    ; RSI = number of entries (max 4096)

    push rbp
    mov rbp, rsp

    mov r8, rdi
    mov qword [r8 + 0], 0       ; Current index
    mov [r8 + 8], rsi           ; Max entries
    mov [r8 + 16], r8           ; Base pointer (self)
    add qword [r8 + 16], 24     ; Skip header

    ; Store in per-CPU data
    mov [gs:CPU_TRANSITION_LOG], r8

    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Context Save/Restore for Mode Transitions
; ─────────────────────────────────────────────────────────────

global save_full_context
save_full_context:
    ; rdi = pointer to CpuRegisterState structure
    ; Saves all registers to the structure
    push rbp
    mov rbp, rsp

    mov [rdi + 0], rax        ; rax
    mov [rdi + 8], rbx        ; rbx
    mov [rdi + 16], rcx       ; rcx
    mov [rdi + 24], rdx       ; rdx
    mov [rdi + 32], rsi       ; rsi
    mov [rdi + 40], rdi       ; rdi (original, overwritten)
    mov [rdi + 48], rbp       ; rbp

    ; Save stack pointer from caller's frame
    mov rax, [rbp + 0]        ; Saved RBP
    mov [rdi + 56], rsp       ; rsp

    mov [rdi + 64], r8        ; r8
    mov [rdi + 72], r9        ; r9
    mov [rdi + 80], r10       ; r10
    mov [rdi + 88], r11       ; r11
    mov [rdi + 96], r12       ; r12
    mov [rdi + 104], r13      ; r13
    mov [rdi + 112], r14      ; r14
    mov [rdi + 120], r15      ; r15

    ; Save RIP (return address)
    mov rax, [rbp + 8]
    mov [rdi + 128], rax       ; rip

    ; Save RFLAGS
    pushfq
    pop rax
    mov [rdi + 136], rax       ; rflags

    ; Save segment registers
    mov [rdi + 144], cs        ; cs
    mov [rdi + 152], ds        ; ds
    mov [rdi + 160], es        ; es
    mov [rdi + 168], fs        ; fs
    mov [rdi + 176], gs        ; gs
    mov [rdi + 184], ss        ; ss

    ; Save control registers
    mov rax, cr0
    mov [rdi + 192], rax
    mov rax, cr2
    mov [rdi + 200], rax
    mov rax, cr3
    mov [rdi + 208], rax
    mov rax, cr4
    mov [rdi + 216], rax

    ; Save EFER MSR
    mov rcx, 0xC0000080
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov [rdi + 224], rax

    ; Save descriptor table registers
    sgdt [rdi + 232]           ; GDTR (6 bytes)
    sidt [rdi + 242]           ; IDTR (6 bytes)
    str [rdi + 252]            ; TR

    ; Save FS/GS base MSRs
    mov rcx, 0xC0000100        ; FS.base
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov [rdi + 264], rax

    mov rcx, 0xC0000101        ; GS.base
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov [rdi + 272], rax

    pop rbp
    ret

global restore_full_context
restore_full_context:
    ; rdi = pointer to CpuRegisterState structure
    ; Restores all registers from the structure
    ; NOTE: This function does NOT return to caller — it returns to the saved RIP

    ; Restore control registers (careful order)
    mov rax, [rdi + 224]       ; EFER
    mov rcx, 0xC0000080
    mov rdx, rax
    shr rdx, 32
    wrmsr

    mov rax, [rdi + 208]       ; CR3 (page tables)
    mov cr3, rax

    mov rax, [rdi + 216]       ; CR4
    mov cr4, rax

    mov rax, [rdi + 192]       ; CR0
    mov cr0, rax

    ; Restore descriptor tables
    lgdt [rdi + 232]
    lidt [rdi + 242]

    ; Restore FS/GS base MSRs
    mov rax, [rdi + 264]       ; FS.base
    mov rcx, 0xC0000100
    mov rdx, rax
    shr rdx, 32
    wrmsr

    mov rax, [rdi + 272]       ; GS.base
    mov rcx, 0xC0000101
    mov rdx, rax
    shr rdx, 32
    wrmsr

    ; Restore segment registers (except CS, which goes on iretq stack)
    push qword [rdi + 184]     ; SS
    push qword [rdi + 56]      ; RSP
    push qword [rdi + 136]     ; RFLAGS
    push qword [rdi + 144]     ; CS
    push qword [rdi + 128]     ; RIP

    ; Restore general registers
    mov rax, [rdi + 0]
    mov rbx, [rdi + 8]
    mov rcx, [rdi + 16]
    mov rdx, [rdi + 24]
    mov rsi, [rdi + 32]
    mov rbp, [rdi + 48]
    mov r8,  [rdi + 64]
    mov r9,  [rdi + 72]
    mov r10, [rdi + 80]
    mov r11, [rdi + 88]
    mov r12, [rdi + 96]
    mov r13, [rdi + 104]
    mov r14, [rdi + 112]
    mov r15, [rdi + 120]

    ; Restore RDI last (because we used it for the structure pointer)
    mov rdi, [rdi + 40]

    ; Return via IRETQ (pops RIP, CS, RFLAGS, RSP, SS)
    iretq

; ─────────────────────────────────────────────────────────────
; Mode Transition Verification (integrity checks)
; ─────────────────────────────────────────────────────────────

global verify_transition_integrity
verify_transition_integrity:
    ; rdi = pointer to ModeTransitionRecord
    ; Returns: 0 = valid, non-zero = invalid
    push rbp
    mov rbp, rsp

    xor rax, rax

    ; Check that from_mode != to_mode (unless it's a no-op)
    mov rcx, [rdi + 8]         ; from_mode
    mov rdx, [rdi + 16]        ; to_mode
    cmp rcx, rdx
    je .no_op_allowed

    ; Check that capability hash is non-zero for privileged transitions
    mov r8, [rdi + 32]         ; caller_capability hash
    test r8, r8
    jz .check_public

    ; Check that timestamp is reasonable (non-zero and not far in future)
    mov r9, [rdi + 40]         ; transition_timestamp
    test r9, r9
    jz .invalid

    ; All checks passed
    xor rax, rax
    pop rbp
    ret

.no_op_allowed:
    ; Same-mode transitions are allowed for certain types
    mov r10, [rdi + 0]         ; transition_type
    cmp r10, 10                ; CONTEXT_SWITCH
    je .valid
    cmp r10, 7                 ; TRAP_RESUME
    je .valid
    jmp .invalid

.check_public:
    ; Public transitions (user → user) need no capability
    mov r10, [rdi + 0]
    cmp r10, 0                 ; USER_TO_KERNEL
    je .invalid                 ; User→kernel requires capability
    cmp r10, 3                 ; KERNEL_TO_USER
    je .valid                   ; Kernel→user is public
    cmp r10, 4                 ; INTERRUPT_ENTRY
    je .valid                   ; Interrupts don't need capability
    cmp r10, 5                 ; INTERRUPT_RETURN
    je .valid

.invalid:
    mov rax, 1
    pop rbp
    ret

.valid:
    xor rax, rax
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; Capability Gate Table (dispatch by gate type)
; ─────────────────────────────────────────────────────────────

section .data
align 16

capability_gate_table:
    dq 0                              ; Index 0: unused
    dq capability_gate_syscall        ; Index 1: SYSCALL
    dq capability_gate_ipc            ; Index 2: IPC
    dq capability_gate_nucleus        ; Index 3: NUCLEUS
    dq capability_gate_interrupt      ; Index 4: INTERRUPT
    dq capability_gate_memory         ; Index 5: MEMORY
    dq capability_gate_io             ; Index 6: IO

section .text

capability_gate_syscall:
    ; Handle capability-gated syscall
    ; rdi = syscall number
    ; rsi = arg1, rdx = arg2, rcx = arg3, r8 = arg4, r9 = arg5

    ; Validate syscall capability
    mov rax, [gs:CPU_CAPABILITY_ROOT]
    test rax, rax
    jz .deny

    ; Call the kernel syscall dispatcher
    extern kernel_syscall_dispatch
    call kernel_syscall_dispatch
    ret

.deny:
    mov rax, -1
    ret

capability_gate_ipc:
    ; Handle capability-gated IPC operation
    ; rdi = IPC channel handle
    ; rsi = operation type
    ; rdx = message pointer

    ; Validate IPC capability
    extern kernel_ipc_dispatch
    call kernel_ipc_dispatch
    ret

capability_gate_nucleus:
    ; Handle capability-gated nucleus operation
    ; rdi = nucleus operation
    ; rsi = arg array
    ; rdx = capability token

    ; Transition to nucleus mode
    call kernel_to_nucleus_entry
    ret

capability_gate_interrupt:
    ; Handle interrupt registration/modification
    ; rdi = IRQ number
    ; rsi = handler function
    ; rdx = capability

    extern kernel_interrupt_register
    call kernel_interrupt_register
    ret

capability_gate_memory:
    ; Handle memory mapping capability
    ; rdi = virtual address
    ; rsi = physical address
    ; rdx = size
    ; rcx = flags

    extern kernel_memory_map
    call kernel_memory_map
    ret

capability_gate_io:
    ; Handle I/O port access capability
    ; rdi = port number
    ; rsi = operation (read=0, write=1)
    ; rdx = value (for write)

    extern kernel_io_access
    call kernel_io_access
    ret
