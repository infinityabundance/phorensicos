; smp_boot.asm — SMP boot and AP startup sequence
; Application Processor initialization, per-CPU data structures, IPI handling, topology
; Phorensic OS — Symmetric Multiprocessing Bootstrap
;
; File: src/asm/smp_boot.asm
; Architecture: x86_64
; Court: smp_boot_asm:v2
; Audit: Line-audited by Nucleus Audit Team

[BITS 64]
[ORG 0x0]  ; Relocatable — linked at nucleus base + offset

; ─────────────────────────────────────────────────────────────
; Constants
; ─────────────────────────────────────────────────────────────

; APIC register offsets
APIC_ID_REG             equ 0x020   ; Local APIC ID Register
APIC_VERSION_REG        equ 0x030   ; Local APIC Version Register
APIC_TPR_REG            equ 0x080   ; Task Priority Register
APIC_APR_REG            equ 0x090   ; Arbitration Priority Register
APIC_PPR_REG            equ 0x0A0   ; Processor Priority Register
APIC_EOI_REG            equ 0x0B0   ; End Of Interrupt Register
APIC_LDR_REG            equ 0x0D0   ; Logical Destination Register
APIC_DFR_REG            equ 0x0E0   ; Destination Format Register
APIC_SPURIOUS_REG       equ 0x0F0   ; Spurious Interrupt Vector Register
APIC_ICR_LOW            equ 0x300   ; Interrupt Command Register (low dword)
APIC_ICR_HIGH           equ 0x310   ; Interrupt Command Register (high dword)
APIC_LVT_TIMER          equ 0x320   ; LVT Timer Register
APIC_LVT_THERMAL        equ 0x330   ; LVT Thermal Register
APIC_LVT_PERFMON        equ 0x340   ; LVT Performance Monitor Register
APIC_LVT_LINT0          equ 0x350   ; LVT LINT0 Register
APIC_LVT_LINT1          equ 0x360   ; LVT LINT1 Register
APIC_LVT_ERROR          equ 0x370   ; LVT Error Register
APIC_TIMER_ICR          equ 0x380   ; Timer Initial Count Register
APIC_TIMER_CCR          equ 0x390   ; Timer Current Count Register
APIC_TIMER_DCR          equ 0x3E0   ; Timer Divide Configuration Register

; Per-CPU data offsets
CPU_SELF_PTR            equ 0x00   ; Pointer to self
CPU_KERNEL_STACK        equ 0x08   ; Kernel stack pointer
CPU_NUCLEUS_STACK       equ 0x10   ; Nucleus stack pointer
CPU_CURRENT_THREAD      equ 0x18   ; Current thread pointer
CPU_CURRENT_SESSION     equ 0x20   ; Current session pointer
CPU_CAPABILITY_ROOT     equ 0x28   ; Root capability table pointer
CPU_GENERATION          equ 0x30   ; Current generation counter
CPU_INT_ENABLED         equ 0x38   ; Interrupt enabled flag
CPU_APIC_ID             equ 0x40   ; Local APIC ID read at boot
CPU_CPU_NUMBER          equ 0x48   ; Logical CPU number (0 = BSP, 1+ = AP)
CPU_TOPOLOGY_LEVEL      equ 0x50   ; Topology level (0=thread, 1=core, 2=die, 3=package)
CPU_CORE_ID             equ 0x58   ; Physical core ID
CPU_PACKAGE_ID          equ 0x60   ; Physical package/socket ID
CPU_CACHE_LINE_SIZE     equ 0x68   ; Cache line size from CPUID
CPU_TLB_INFO            equ 0x70   ; TLB info from CPUID
CPU_PAGE_SIZE           equ 0x78   ; Large page size
CPU_BOOT_TIMESTAMP      equ 0x80   ; Timestamp of AP boot
CPU_IPI_PENDING         equ 0x88   ; IPI pending flag
CPU_IPI_VECTOR          equ 0x90   ; Last IPI vector received
CPU_RESERVED_1          equ 0x98   ; Reserved for alignment
CPU_RESIDUAL_LOG        equ 0x100  ; Residual log pointer (offset to larger buffer)

; Per-CPU data structure size
PER_CPU_DATA_SIZE       equ 0x1000 ; 4KB per-CPU data

; IPI delivery modes
IPI_FIXED               equ 0x00000000
IPI_LOWEST_PRIORITY     equ 0x00000100
IPI_SMI                 equ 0x00000200
IPI_NMI                 equ 0x00000400
IPI_INIT                equ 0x00000500
IPI_STARTUP             equ 0x00000600

; IPI destination shorthand
IPI_DEST_SELF           equ 0x00040000
IPI_DEST_ALL            equ 0x00080000
IPI_DEST_ALL_BUT_SELF   equ 0x000C0000

; APIC spurious vector
SPURIOUS_VECTOR         equ 0xFF
APIC_SOFTWARE_ENABLE    equ 0x100

; Boot trampoline constants
AP_TRAMPOLINE_BASE      equ 0x00008000  ; Where AP trampoline is copied
AP_INIT_CS              equ 0x08        ; 64-bit code segment
AP_INIT_DS              equ 0x10        ; 64-bit data segment
AP_STACK_SIZE           equ 0x4000      ; 16KB per-AP stack

; CPUID leaf constants
CPUID_LEAF_1            equ 0x00000001  ; Feature info
CPUID_LEAF_4            equ 0x00000004  ; Cache / topology
CPUID_LEAF_B            equ 0x0000000B  ; Extended topology
CPUID_EXT_LEAF_8        equ 0x80000008  ; Physical address size / topology

; ─────────────────────────────────────────────────────────────
; External Symbols
; ─────────────────────────────────────────────────────────────

extern per_cpu_data_array          ; Array of per-CPU structures
extern cpu_count                   ; Number of detected CPUs
extern bsp_apic_id                 ; BSP's APIC ID
extern ap_boot_stack               ; Stack pointer for AP init
extern ap_boot_flag                ; Flag to signal AP ready
extern smp_ready_flag              ; Global SMP ready flag
extern apic_base_address           ; Local APIC base MMIO address
extern init_ap_kernel              ; AP kernel initialization function
extern handle_ipi                  ; IPI handler function in kernel
extern record_boot_residual        ; Record boot residual
extern timer_calibrate_ap          ; AP-local timer calibration

; ─────────────────────────────────────────────────────────────
; BSP (Bootstrap Processor) SMP Initialization
; ─────────────────────────────────────────────────────────────

global smp_bsp_init
smp_bsp_init:
    ; Called by the nucleus after early boot is complete
    ; Expects: RDI = APIC base address
    ;          RSI = pointer to per-CPU data array
    ;          RDX = max CPU count

    ; Save parameters
    mov [apic_base_addr], rdi
    mov [per_cpu_data_ptr], rsi
    mov [max_cpu_count], rdx

    ; Step 1: Initialize BSP's per-CPU data
    mov rdi, [per_cpu_data_ptr]        ; per-CPU data for BSP (entry 0)
    mov rsi, qword 0                    ; CPU number = 0 (BSP)
    call init_per_cpu_data

    ; Step 2: Read BSP's APIC ID
    mov rax, [apic_base_addr]
    mov rdi, rax
    call read_apic_id
    mov [bsp_apic_id], eax
    mov rdi, [per_cpu_data_ptr]
    mov [rdi + CPU_APIC_ID], eax

    ; Step 3: Detect CPU topology via CPUID
    call detect_cpu_topology

    ; Step 4: Program BSP's local APIC
    mov rdi, [apic_base_addr]
    call program_local_apic

    ; Step 5: Count total available APs via ACPI MADT
    ; (The ACPI driver should have parsed the MADT and set cpu_count)
    mov rax, [cpu_count]
    mov [max_cpu_count], rax

    ; Step 6: Allocate and initialize per-CPU data for APs
    mov rcx, [max_cpu_count]
    mov rdi, [per_cpu_data_ptr]
    add rdi, PER_CPU_DATA_SIZE           ; Skip BSP's data
    mov rsi, 1                           ; Start with CPU 1

.init_ap_data_loop:
    cmp rsi, rcx
    jge .init_ap_data_done
    push rdi
    push rsi
    push rcx
    mov rdi, rdi
    call init_per_cpu_data
    pop rcx
    pop rsi
    pop rdi
    add rdi, PER_CPU_DATA_SIZE
    inc rsi
    jmp .init_ap_data_loop

.init_ap_data_done:

    ; Step 7: Set the smp_ready_flag to signal APs can start
    mov qword [smp_ready_flag], 0
    mov qword [ap_boot_flag], 0

    ; Record BSP residual
    mov rdi, 0              ; BSP CPU number
    call record_boot_residual

    ret

; ─────────────────────────────────────────────────────────────
; Start Application Processors (APs)
; ─────────────────────────────────────────────────────────────

global smp_start_aps
smp_start_aps:
    ; Called after BSP init to bring up APs
    ; Expects: RDI = number of APs to start

    push rbp
    mov rbp, rsp
    push rbx
    push r12
    push r13
    push r14
    push r15

    mov r12, rdi                    ; Number of APs to start
    mov r13, [apic_base_addr]       ; APIC base

    ; Step 1: Copy AP trampoline to fixed low memory
    mov rsi, ap_trampoline_start
    mov rdi, AP_TRAMPOLINE_BASE
    mov rcx, ap_trampoline_end - ap_trampoline_start
    rep movsb

    ; Step 2: Send INIT IPI to all APs
    mov rdi, r13
    mov rsi, IPI_INIT | IPI_DEST_ALL_BUT_SELF
    call send_ipi

    ; Wait 10ms (measured via simple delay loop)
    mov rcx, 100000
.delay_after_init:
    loop .delay_after_init

    ; Step 3: Send STARTUP IPI to all APs
    mov rdi, r13

    ; Calculate startup page vector (trampoline base / 4K)
    mov rsi, AP_TRAMPOLINE_BASE
    shr rsi, 12
    and rsi, 0xFF
    or rsi, IPI_STARTUP

    call send_ipi

    ; Wait for APs to acknowledge (poll ap_boot_flag)
    mov r14, 0                      ; AP count acknowledged
    mov r15, 100000                 ; Timeout counter

.poll_ap_loop:
    cmp r14, r12                    ; All APs started?
    jge .ap_start_done

    mov al, [ap_boot_flag]
    cmp al, 0
    je .check_timeout

    ; An AP has booted, increment count and clear flag
    inc r14
    mov byte [ap_boot_flag], 0

    ; Reset timeout counter
    mov r15, 100000
    jmp .poll_ap_loop

.check_timeout:
    dec r15
    jnz .poll_ap_loop

    ; Timeout - some APs may not have started
    ; Record how many did start and proceed
    mov [cpu_count], r14

.ap_start_done:

    ; Step 4: Send a second STARTUP IPI (per MP spec)
    mov rdi, r13
    mov rsi, AP_TRAMPOLINE_BASE
    shr rsi, 12
    and rsi, 0xFF
    or rsi, IPI_STARTUP
    call send_ipi

    ; Wait for any remaining APs
    mov r15, 50000
.poll_ap_loop2:
    cmp r14, r12
    jge .smp_ready
    mov al, [ap_boot_flag]
    cmp al, 0
    je .smp_poll2
    inc r14
    mov byte [ap_boot_flag], 0

.smp_poll2:
    dec r15
    jnz .poll_ap_loop2

.smp_ready:
    mov qword [smp_ready_flag], 1

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    mov rsp, rbp
    pop rbp
    ret

; ─────────────────────────────────────────────────────────────
; AP Trampoline (executed by APs in real mode initially)
; ─────────────────────────────────────────────────────────────

[BITS 16]
global ap_trampoline_start
ap_trampoline_start:
    ; APs enter here in real mode after SIPI
    ; CS:IP configured to AP_TRAMPOLINE_BASE:0x0000

    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Set up a minimal stack
    mov sp, 0x6000

    ; Enable A20 gate quickly
    in al, 0x92
    or al, 0x02
    out 0x92, al

    ; Load the GDT that was set up by BSP at a known location
    lgdt [cs:ap_gdt_desc - ap_trampoline_start + AP_TRAMPOLINE_BASE]

    ; Enter protected mode
    mov eax, cr0
    or eax, 0x00000001
    mov cr0, eax

    ; Far jump to flush pipeline
    jmp dword 0x08:(ap_protected_entry - ap_trampoline_start + AP_TRAMPOLINE_BASE)

[BITS 32]
ap_protected_entry:
    ; Protected mode entry
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Set up stack
    mov esp, 0x7000

    ; Check for long mode support via CPUID
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb ap_halt                     ; No long mode support, halt

    mov eax, 0x80000001
    cpuid
    test edx, 0x20000000          ; LM bit
    jz ap_halt                     ; No long mode, halt

    ; Enable PAE and PSE
    mov eax, cr4
    or eax, 0x00000020 | 0x00000010  ; PAE | PSE
    mov cr4, eax

    ; Load page table base (set up by BSP at known location)
    mov eax, [cs:pml4_addr - ap_trampoline_start + AP_TRAMPOLINE_BASE]
    mov cr3, eax

    ; Enable long mode (EFER.LME)
    mov ecx, 0xC0000080           ; EFER MSR
    rdmsr
    or eax, 0x00000100            ; LME bit
    wrmsr

    ; Enable paging
    mov eax, cr0
    or eax, 0x80000000            ; PG bit
    mov cr0, eax

    ; Far jump to 64-bit mode
    jmp dword 0x08:(ap_long_mode_entry - ap_trampoline_start + AP_TRAMPOLINE_BASE)

[BITS 64]
ap_long_mode_entry:
    ; Long mode entry point for APs

    ; Set up segment registers for long mode
    mov ax, 0x10                  ; Kernel data segment
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    ; Set up kernel stack for this AP
    mov rax, [ap_boot_stack]
    mov rsp, rax

    ; Signal BSP that this AP has booted
    mov byte [ap_boot_flag], 1

    ; Read this AP's APIC ID
    mov rdi, [apic_base_addr]
    call read_apic_id
    mov r12d, eax                 ; Save APIC ID

    ; Find our per-CPU data slot
    mov rdi, [per_cpu_data_ptr]
    xor rsi, rsi
.find_cpu_slot:
    cmp dword [rdi + CPU_APIC_ID], r12d
    je .cpu_slot_found
    add rdi, PER_CPU_DATA_SIZE
    inc rsi
    cmp rsi, [max_cpu_count]
    jl .find_cpu_slot
    ; Slot not found — use last slot
    dec rsi
    mov rdi, [per_cpu_data_ptr]
    mov rax, PER_CPU_DATA_SIZE
    mul rsi
    add rdi, rax

.cpu_slot_found:
    ; RDI = pointer to this AP's per-CPU data
    ; RSI = CPU number

    ; Initialize per-CPU data (already filled by BSP, just set runtime fields)
    mov [rdi + CPU_APIC_ID], r12d
    mov [rdi + CPU_CPU_NUMBER], esi
    mov [rdi + CPU_SELF_PTR], rdi

    ; Set up GS segment base to point to per-CPU data
    push rdi
    push rsi
    mov rdi, rdi
    mov rsi, 0                    ; GS base MSR index
    mov rax, rdi
    shr rax, 32
    mov edx, eax
    mov eax, edi
    mov ecx, 0xC0000101           ; GS_BASE MSR
    wrmsr
    pop rsi
    pop rdi

    ; Set up kernel stack
    mov rax, PER_CPU_DATA_SIZE
    mul rsi
    add rax, [per_cpu_data_ptr]
    add rax, 0x800               ; Stack grows down from near top of per-CPU area
    mov [rdi + CPU_KERNEL_STACK], rax
    mov [rdi + CPU_NUCLEUS_STACK], rax

    ; Program local APIC
    mov rdi, [apic_base_addr]
    call program_local_apic

    ; Calibrate AP-local timer
    call timer_calibrate_ap

    ; Record AP boot residual
    mov rdi, rsi                  ; CPU number
    call record_boot_residual

    ; Enable interrupts
    sti

    ; Call the AP kernel initialization function
    mov rdi, rsi                  ; CPU number
    mov rsi, [per_cpu_data_ptr]   ; Per-CPU data array pointer
    call init_ap_kernel

    ; AP initialization complete - idle loop
.ap_idle_loop:
    hlt
    jmp .ap_idle_loop

ap_halt:
    cli
    hlt
    jmp ap_halt

; ─────────────────────────────────────────────────────────────
; AP Trampoline Data
; ─────────────────────────────────────────────────────────────

align 8
pml4_addr:      dq 0              ; Set by BSP before sending SIPI
ap_gdt:
    dq 0x0000000000000000         ; Null descriptor
    dq 0x00CF9A000000FFFF         ; 64-bit code segment (ring 0)
    dq 0x00CF92000000FFFF         ; 64-bit data segment (ring 0)
ap_gdt_end:

ap_gdt_desc:
    dw ap_gdt_end - ap_gdt - 1    ; Limit
    dq ap_gdt - ap_trampoline_start + AP_TRAMPOLINE_BASE  ; Base

ap_trampoline_end:

; ─────────────────────────────────────────────────────────────
; Read Local APIC ID
; ─────────────────────────────────────────────────────────────

[BITS 64]
global read_apic_id
read_apic_id:
    ; RDI = APIC base address
    ; Returns: EAX = APIC ID
    mov rax, rdi
    add rax, APIC_ID_REG
    mov eax, [rax]
    shr eax, 24
    ret

; ─────────────────────────────────────────────────────────────
; Program Local APIC
; ─────────────────────────────────────────────────────────────

global program_local_apic
program_local_apic:
    ; RDI = APIC base address
    push rbx

    ; Enable spurious vector and software enable
    mov rax, rdi
    add rax, APIC_SPURIOUS_REG
    mov dword [rax], SPURIOUS_VECTOR | APIC_SOFTWARE_ENABLE

    ; Set task priority to 0 (accept all)
    mov rax, rdi
    add rax, APIC_TPR_REG
    mov dword [rax], 0

    ; Program LVT entries

    ; LINT0: masked (extINT)
    mov rax, rdi
    add rax, APIC_LVT_LINT0
    mov dword [rax], 0x00010000   ; Masked

    ; LINT1: NMI
    mov rax, rdi
    add rax, APIC_LVT_LINT1
    mov dword [rax], 0x00000400   ; NMI, not masked

    ; Error: handle with vector 0xFE
    mov rax, rdi
    add rax, APIC_LVT_ERROR
    mov dword [rax], 0x000000FE

    ; Performance counter: masked
    mov rax, rdi
    add rax, APIC_LVT_PERFMON
    mov dword [rax], 0x00010000

    ; Thermal: masked
    mov rax, rdi
    add rax, APIC_LVT_THERMAL
    mov dword [rax], 0x00010000

    ; Set logical destination to all ones (flat mode)
    mov rax, rdi
    add rax, APIC_DFR_REG
    mov dword [rax], 0xFFFFFFFF   ; Flat model

    mov rax, rdi
    add rax, APIC_LDR_REG
    mov byte [rax], 0xFF          ; Target all logical CPUs

    pop rbx
    ret

; ─────────────────────────────────────────────────────────────
; Send IPI (Inter-Processor Interrupt)
; ─────────────────────────────────────────────────────────────

global send_ipi
send_ipi:
    ; RDI = APIC base address
    ; RSI = IPI command value (low dword)
    push rbx

    ; Wait for previous IPI to complete
.ipi_wait:
    mov rax, rdi
    add rax, APIC_ICR_LOW
    mov ebx, [rax]
    test ebx, 0x00001000          ; Delivery Status bit
    jnz .ipi_wait

    ; Write high dword of ICR (destination field)
    mov rax, rdi
    add rax, APIC_ICR_HIGH
    ; For destination shorthand, high dword is ignored
    ; For specific destination, would write APIC ID here
    mov dword [rax], 0

    ; Write low dword to send IPI
    mov rax, rdi
    add rax, APIC_ICR_LOW
    mov dword [rax], esi

    ; Wait for delivery to complete
.ipi_delivery_wait:
    mov ebx, [rax]
    test ebx, 0x00001000
    jnz .ipi_delivery_wait

    pop rbx
    ret

; ─────────────────────────────────────────────────────────────
; Detect CPU Topology
; ─────────────────────────────────────────────────────────────

global detect_cpu_topology
detect_cpu_topology:
    ; Uses CPUID leaves 1, 4, and B to determine topology
    ; Stores results in per-CPU data

    push rdi

    ; Check if CPUID leaf 0xB is supported (x2APIC/extended topology)
    mov eax, 0
    cpuid
    cmp eax, 0x0B
    jb .use_legacy_topology

    ; Use leaf 0xB for extended topology enumeration
    mov r8, 0                     ; Level index

.topology_loop:
    mov eax, 0x0B
    mov ecx, r8d
    cpuid

    ; Check if this level is valid
    test ecx, 0xFF00             ; Level type
    jz .topology_done

    ; EAX = bits (APIC ID shift)
    ; EBX = number of logical processors at this level
    ; ECX = level number and type
    ; EDX = x2APIC ID

    cmp r8d, 0
    jne .check_level_1

    ; Level 0: SMT (thread)
    ; Shift value
    mov r9, rdi
    mov [r9 + CPU_TOPOLOGY_LEVEL], 0
    jmp .next_level

.check_level_1:
    cmp r8d, 1
    jne .check_level_2

    ; Level 1: Core
    mov r9, rdi
    mov byte [r9 + CPU_TOPOLOGY_LEVEL], 1
    mov [r9 + CPU_CORE_ID], edx   ; Core ID is in x2APIC ID at this level
    jmp .next_level

.check_level_2:
    cmp r8d, 2
    jne .next_level

    ; Level 2: Package/die
    mov r9, rdi
    mov byte [r9 + CPU_TOPOLOGY_LEVEL], 2
    mov [r9 + CPU_PACKAGE_ID], edx

.next_level:
    inc r8d
    cmp r8d, 3
    jl .topology_loop

.topology_done:
    jmp .topology_exit

.use_legacy_topology:
    ; Fallback: use CPUID leaf 1 and leaf 4
    mov eax, 1
    cpuid

    ; EBX[23:16] = initial APIC ID
    mov r9, rdi
    mov [r9 + CPU_APIC_ID], ebx
    shr ebx, 16
    and ebx, 0xFF

    ; EBX[31:24] = maximum addressable IDs for logical processors
    mov r10, rbx
    shr r10, 24
    and r10, 0xFF

    ; ECX[23:16] = core count (max addressable IDs for cores)
    mov r10, ecx
    shr r10, 16
    and r10, 0xFF

    ; Determine package ID from APIC ID clustering
    mov rax, [r9 + CPU_APIC_ID]
    shr rax, 2                    ; Simplified: assume 2 bits for SMT
    mov [r9 + CPU_CORE_ID], eax
    shr eax, 4                    ; Simplified: assume 4 bits for cores
    mov [r9 + CPU_PACKAGE_ID], eax
    mov byte [r9 + CPU_TOPOLOGY_LEVEL], 2

.topology_exit:
    pop rdi
    ret

; ─────────────────────────────────────────────────────────────
; Initialize Per-CPU Data Structure
; ─────────────────────────────────────────────────────────────

global init_per_cpu_data
init_per_cpu_data:
    ; RDI = pointer to per-CPU data block
    ; RSI = CPU number
    push rdi
    push rsi

    xor rax, rax

    ; Zero the full structure
    mov rcx, PER_CPU_DATA_SIZE
    xor rax, rax
    rep stosb

    pop rsi
    pop rdi

    ; Set self pointer
    mov [rdi + CPU_SELF_PTR], rdi
    mov [rdi + CPU_CPU_NUMBER], esi

    ; Set default stack pointers
    mov rax, rdi
    add rax, 0x800               ; Stack grows down from top half of per-CPU area
    mov [rdi + CPU_KERNEL_STACK], rax
    mov [rdi + CPU_NUCLEUS_STACK], rax

    ; Set default APIC ID (will be overwritten at boot)
    mov dword [rdi + CPU_APIC_ID], 0

    ; Set generation counter
    mov qword [rdi + CPU_GENERATION], 0

    ; Clear IPI state
    mov qword [rdi + CPU_IPI_PENDING], 0
    mov qword [rdi + CPU_IPI_VECTOR], 0

    ; Mark as not booted
    mov qword [rdi + CPU_BOOT_TIMESTAMP], 0

    ; Enable interrupts by default
    mov qword [rdi + CPU_INT_ENABLED], 1

    ret

; ─────────────────────────────────────────────────────────────
; Handle IPI (Inter-Processor Interrupt)
; ─────────────────────────────────────────────────────────────

global ipi_handler_entry
ipi_handler_entry:
    ; Called from interrupt handler with context saved
    ; Stack at entry: vector number, error code, saved registers
    ; RDI = vector number, RSI = register frame pointer

    push rax
    push rbx
    push rcx
    push rdx
    push rdi
    push rsi

    ; Get per-CPU data from GS base
    mov rdi, [gs:CPU_SELF_PTR]

    ; Record IPI vector
    mov rax, [rsp + 40]          ; Vector number from stack
    mov [rdi + CPU_IPI_VECTOR], rax
    mov qword [rdi + CPU_IPI_PENDING], 1

    ; Send EOI
    mov rsi, [apic_base_addr]
    mov rax, rsi
    add rax, APIC_EOI_REG
    mov dword [rax], 0

    ; Call kernel IPI handler
    mov rsi, [gs:CPU_SELF_PTR]
    mov rdi, [rsp + 40]          ; Vector number
    call handle_ipi

    ; Clear pending flag
    xor rdi, rdi
    mov [gs:CPU_IPI_PENDING], rdi

    pop rsi
    pop rdi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    iretq

; ─────────────────────────────────────────────────────────────
; Data Section
; ─────────────────────────────────────────────────────────────

section .data
align 8

global apic_base_addr
apic_base_addr:     dq 0x0      ; Set during init

global per_cpu_data_ptr
per_cpu_data_ptr:   dq 0x0      ; Pointer to per-CPU data array

global max_cpu_count
max_cpu_count:      dq 0x0      ; Maximum number of CPUs supported
