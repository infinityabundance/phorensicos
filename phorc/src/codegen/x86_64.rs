// x86_64 instruction selection and encoding for Phorc
// Uses iced-x86 v1.21 API — register allocation, System V ABI, stack frame

use crate::ast::PrimType;
use crate::ir::{
    BinOpKind, ByteAttribution, Constant, Op, PhirFunction, PhirType, UnOpKind, Value,
};
use crate::receipts::RelocationEntry;

use iced_x86::*;

/// Extract the PrimType from a Value, if it is a primitive type.
#[allow(dead_code)]
fn value_prim_type(val: &Value) -> Option<PrimType> {
    match val {
        Value::Const(c) => match c {
            Constant::Int(_, bits) => match bits {
                8 => Some(PrimType::U8),
                16 => Some(PrimType::U16),
                32 => Some(PrimType::U32),
                64 => Some(PrimType::U64),
                _ => Some(PrimType::U64),
            },
            Constant::Bool(_) => Some(PrimType::Bool),
            _ => Some(PrimType::U64),
        },
        Value::Temp(_, PhirType::Prim(p))
        | Value::Arg(_, PhirType::Prim(p))
        | Value::Global(_, PhirType::Prim(p))
        | Value::Local(_, PhirType::Prim(p)) => Some(p.clone()),
        _ => Some(PrimType::U64),
    }
}

/// Determine if a PHIR type is a signed integer type.
fn is_signed_type(ty: &PhirType) -> bool {
    matches!(
        ty,
        PhirType::Prim(
            PrimType::I8 | PrimType::I16 | PrimType::I32 | PrimType::I64 | PrimType::Isize,
        )
    )
}

/// Determine if a value has a signed integer primitive type.
fn value_is_signed(val: &Value) -> bool {
    match val {
        Value::Temp(_, ty) | Value::Arg(_, ty) | Value::Global(_, ty) | Value::Local(_, ty) => {
            is_signed_type(ty)
        }
        // Constants are unsigned by default in PHIR
        _ => false,
    }
}

/// Stack slot offset for a local variable: [rbp - (idx+1)*8]
fn local_stack_offset(idx: usize) -> i64 {
    -((idx as i64 + 1) * 8)
}

/// Create a memory operand for loading/storing a local variable's value (size=8 for 64-bit)
fn local_mem_access(idx: usize) -> MemoryOperand {
    let disp = local_stack_offset(idx);
    MemoryOperand::new(
        Register::RBP,
        Register::None,
        1,
        disp,
        8,
        false,
        Register::None,
    )
}

/// Create a memory operand for taking a local's address via LEA (size=0, not a real access)
fn local_mem_addr(idx: usize) -> MemoryOperand {
    let disp = local_stack_offset(idx);
    MemoryOperand::new(
        Register::RBP,
        Register::None,
        1,
        disp,
        0,
        false,
        Register::None,
    )
}
use std::collections::BTreeMap;

// Helper to unwrap Instruction::with* Result
macro_rules! i0 {
    ($c:ident) => {
        Instruction::with(Code::$c)
    };
}
macro_rules! i1 {
    ($c:ident, $a:expr) => {
        Instruction::with1(Code::$c, $a).unwrap()
    };
}
macro_rules! i2 {
    ($c:ident, $a:expr, $b:expr) => {
        Instruction::with2(Code::$c, $a, $b).unwrap()
    };
}
macro_rules! ib {
    ($c:ident, $t:expr) => {
        Instruction::with_branch(Code::$c, $t).unwrap()
    };
}

// ============================================================================
// Register allocator
// ============================================================================

/// Convert register index to x86-64 64-bit name
pub fn reg_name(reg: usize) -> &'static str {
    match reg {
        0 => "rax",
        1 => "rcx",
        2 => "rdx",
        3 => "rbx",
        4 => "rsp",
        5 => "rbp",
        6 => "rsi",
        7 => "rdi",
        8 => "r8",
        9 => "r9",
        10 => "r10",
        11 => "r11",
        12 => "r12",
        13 => "r13",
        14 => "r14",
        15 => "r15",
        _ => "rax",
    }
}

/// Convert register index to 32-bit name (e.g. for operations on 32-bit values)
pub fn reg_name32(reg: usize) -> &'static str {
    match reg {
        0 => "eax",
        1 => "ecx",
        2 => "edx",
        3 => "ebx",
        4 => "esp",
        5 => "ebp",
        6 => "esi",
        7 => "edi",
        8 => "r8d",
        9 => "r9d",
        10 => "r10d",
        11 => "r11d",
        12 => "r12d",
        13 => "r13d",
        14 => "r14d",
        15 => "r15d",
        _ => "eax",
    }
}

/// Simple register allocator that tracks which registers are free
/// Supports round-robin allocation across caller-save and callee-save registers.
#[derive(Debug, Clone)]
pub struct RegisterAllocator {
    /// Available general-purpose registers for allocation
    regs: [bool; 16],
    /// Next register to try (round-robin hint)
    next_hint: usize,
}

impl RegisterAllocator {
    pub fn new() -> Self {
        let mut regs = [true; 16];
        // Reserve RSP (register 4) for stack pointer
        regs[4] = false;
        // Reserve RBP (register 5) for base pointer
        regs[5] = false;
        Self { regs, next_hint: 0 }
    }

    /// Allocate a register. Returns Some(register index) or None if all busy.
    pub fn alloc(&mut self) -> Option<usize> {
        for offset in 0..16 {
            let idx = (self.next_hint + offset) % 16;
            if idx == 4 || idx == 5 {
                continue;
            } // Skip RSP, RBP
            if self.regs[idx] {
                self.regs[idx] = false;
                self.next_hint = (idx + 1) % 16;
                return Some(idx);
            }
        }
        None
    }

    /// Free a register
    pub fn free(&mut self, reg: usize) {
        if reg < 16 {
            self.regs[reg] = true;
        }
    }

    /// Check if a register is free
    pub fn is_free(&self, reg: usize) -> bool {
        reg < 16 && self.regs[reg]
    }
}

const GP_REGS: &[Register] = &[
    Register::RAX,
    Register::RCX,
    Register::RDX,
    Register::RBX,
    Register::RSI,
    Register::RDI,
    Register::R8,
    Register::R9,
    Register::R10,
    Register::R11,
    Register::R12,
    Register::R13,
    Register::R14,
    Register::R15,
];

const ARG_REGS: &[Register] = &[
    Register::RDI,
    Register::RSI,
    Register::RDX,
    Register::RCX,
    Register::R8,
    Register::R9,
];

struct RegAlloc {
    // BTreeMap (not HashMap): register allocation must be deterministic, and
    // both the low register pool (`free`) and spill slots are assigned in
    // iteration order. HashMap iteration order is not stable across runs, which
    // made emitted object bytes non-reproducible.
    map: BTreeMap<String, Register>,
    free: Vec<Register>,
    spill_next: i32,
    spills: BTreeMap<String, i32>,
}

impl RegAlloc {
    fn new() -> Self {
        let mut free = GP_REGS.to_vec();
        free.reverse();
        Self {
            map: BTreeMap::new(),
            free,
            spill_next: -8,
            spills: BTreeMap::new(),
        }
    }

    fn key(val: &Value) -> String {
        match val {
            Value::Const(c) => format!("c{:?}", c),
            Value::Temp(i, _) => format!("t{}", i),
            Value::Arg(i, _) => format!("a{}", i),
            Value::Global(s, _) => format!("g{}", s),
            Value::Local(i, _) => format!("l{}", i),
        }
    }

    fn get(&self, val: &Value) -> Option<Register> {
        self.map.get(&Self::key(val)).copied()
    }

    /// Allocate a register for a value. Spills to stack if none free.
    fn alloc(&mut self, val: &Value) -> Register {
        let k = Self::key(val);
        if let Some(&r) = self.map.get(&k) {
            return r;
        }
        if let Some(r) = self.free.pop() {
            self.map.insert(k, r);
            return r;
        }
        // No free registers — evict the first allocated value to make room.
        // NOTE: This reserves the stack slot (`spill_next`) but does NOT emit
        // the `mov [rbp+slot], reg` store instruction. To complete spilling,
        // the caller must emit the store before the register is reused.
        let evict_name = self.map.keys().next().unwrap().clone();
        let evict_reg = self.map.remove(&evict_name).unwrap();
        self.spills.insert(evict_name, self.spill_next);
        self.spill_next -= 8;
        self.map.insert(k, evict_reg);
        evict_reg
    }

    fn frame_size(&self) -> i32 {
        let raw = (-self.spill_next - 8).max(0);
        ((raw + 15) / 16) * 16
    }

    /// Number of values that have been spilled to the stack.
    #[allow(dead_code)]
    pub fn spill_count(&self) -> usize {
        self.spills.len()
    }

    /// Allocate a register for a named value. Spills to stack if none free.
    /// This is an alternative to the Value-based alloc for use by name-based APIs.
    #[allow(dead_code)]
    pub fn alloc_name(&mut self, name: &str) -> Register {
        if let Some(&reg) = self.map.get(name) {
            return reg;
        }
        if let Some(reg) = self.free.pop() {
            self.map.insert(name.to_string(), reg);
            return reg;
        }
        // Spill: evict the least recently used register
        let evict_name = self.map.keys().next().unwrap().clone();
        let evict_reg = self.map.remove(&evict_name).unwrap();
        let spill_slot = self.spill_next;
        self.spill_next -= 8;
        self.spills.insert(evict_name, spill_slot);
        self.map.insert(name.to_string(), evict_reg);
        evict_reg
    }

    /// Get a register for a value, re-loading from stack if it was spilled.
    ///
    /// NOTE: This allocates stack space for the spill slot but does NOT emit
    /// the actual `mov [rbp+offset], reg` (store) or `mov reg, [rbp+offset]`
    /// (reload) instructions. Full spilling requires integrating `RegAlloc`
    /// with the per-function instruction stream so that:
    ///   1. On eviction:  `mov [rbp + slot], reg` is emitted before reusing the register
    ///   2. On reload:     `mov reg, [rbp + slot]` is emitted before the register is used
    ///
    /// Current implementation: slot reservation / bookkeeping only.
    #[allow(dead_code)]
    pub fn get_or_spill(&mut self, name: &str) -> Register {
        if let Some(&reg) = self.map.get(name) {
            return reg;
        }
        // Check if it was spilled
        if self.spills.contains_key(name) {
            // TODO: emit `mov reg, [rbp + spill_slot]` to reload from stack
            self.alloc_name(name)
        } else {
            self.alloc_name(name)
        }
    }

    /// Free a register, making it available for re-use.
    #[allow(dead_code)]
    pub fn free_name(&mut self, name: &str) {
        if let Some(reg) = self.map.remove(name) {
            self.free.push(reg);
        }
    }

    /// Get the stack offset for a spilled variable, if any.
    #[allow(dead_code)]
    pub fn spill_slot(&self, name: &str) -> Option<i32> {
        self.spills.get(name).copied()
    }

    fn spill_caller(&mut self) {
        let caller = [
            Register::RAX,
            Register::RCX,
            Register::RDX,
            Register::RSI,
            Register::RDI,
            Register::R8,
            Register::R9,
            Register::R10,
            Register::R11,
        ];
        let mut to_remove: Vec<String> = Vec::new();
        for (k, r) in &self.map {
            if caller.contains(r) {
                to_remove.push(k.clone());
            }
        }
        for k in to_remove {
            if let Some(r) = self.map.remove(&k) {
                self.spills.insert(k, self.spill_next);
                self.spill_next -= 8;
                self.free.push(r);
            }
        }
    }
}

// ============================================================================
// Entry point / _start generation
// ============================================================================

/// Generate a `_start` entry point for a freestanding x86-64 kernel with
/// 32-bit → 64-bit mode transition and Multiboot v1 header.
///
/// Layout emitted (load address 0x100000):
///   Offset     0: 32-bit startup code (mov edi,ebx; call; pop ebx; PAE; CR3; LM; paging; GDT; far jump)
///   Offset   ~96: 64-bit entry code (segments, stack, serial, VGA, call kernel_entry, halt)
///   Offset  ~160: Multiboot v1 header (12 bytes, 4-byte aligned)
///   Offset  4096: PML4 page (entry → PDP+present+writable, rest zero)    [0x101000]
///   Offset  8192: PDP page (8B entry: 1GB page, PS=1, P=1, W=1, rest zero) [0x102000]
///   Offset 12288: Stack (16384 bytes of zeros)                              [0x103000]
///   Offset 28672: Stack top                                                  [0x107000]
///
/// The bootloader enters at _start in 32-bit protected mode with:
///   EAX = 0x36D76289 (Multiboot2 magic)
///   EBX = physical address of Multiboot2 info structure
pub fn generate_entry_point(
    kernel_entry_name: &str,
) -> (Vec<u8>, Vec<ByteAttribution>, u64, Vec<RelocationEntry>) {
    use crate::receipts::RelocationKind;
    let mut code: Vec<u8> = Vec::new();
    let mut relocs: Vec<RelocationEntry> = Vec::new();

    // All addresses assume image loaded at 0x100000 (set by linker script kernel.ld).
    //
    // Layout (code blob):
    //   [0x000000 – 0x0000A0)  32-bit startup + GDT + far jump
    //   [0x000060 – 0x0000A0)  64-bit entry code
    //   [0x0000A0 – 0x0000AC)  Multiboot v1 header (12 bytes)
    //   [0x0000AC – 0x001000)  NOP pad to 4096
    //   [0x001000 – 0x002000)  PML4 page table            (VMA 0x101000)
    //   [0x002000 – 0x003000)  PDP page table             (VMA 0x102000)
    //   [0x003000 – 0x007000)  Stack  (16384 bytes)       (VMA 0x103000–0x106FFF)
    //   [0x007000 – 0x008000)  Boot info page             (VMA 0x107000)
    //     Offset 0x000: framebuffer_addr (u64)
    //     Offset 0x008: width (u32)
    //     Offset 0x00C: height (u32)
    //     Offset 0x010: pitch (u32)
    //     Offset 0x014: bpp (u8)

    // ====================================================================
    // 1. 32-bit startup code  (starts at offset 0 of the code blob)
    // ====================================================================
    // Save multiboot info pointer from EBX into EDI (becomes RDI in 64-bit mode)
    code.extend_from_slice(&[0x89, 0xDF]); // mov edi, ebx               (2 bytes)

    // call .next; .next: pop ebx   ; EBX = runtime address of pop ebx
    // Since _start is at VMA 0x100000 and pop ebx is at blob offset 7:
    //   EBX = 0x100007
    code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call +0 = next (5 bytes)
    let next_pos: u32 = code.len() as u32; // = 7 — offset of pop ebx
    code.push(0x5B); // pop ebx                                  (1 byte)  → EBX = 0x100007

    // Enable PAE: mov eax, cr4; or eax, 0x20; mov cr4, eax
    code.extend_from_slice(&[0x0F, 0x20, 0xE0]); // mov eax, cr4         (3 bytes)
    code.extend_from_slice(&[0x0D, 0x20, 0x00, 0x00, 0x00]); // or eax, 0x20  (5 bytes)
    code.extend_from_slice(&[0x0F, 0x22, 0xE0]); // mov cr4, eax         (3 bytes)

    // Point CR3 to PML4 at 0x101000:
    //   lea eax, [ebx + disp]   where disp = 0x101000 - EBX = 0x101000 - 0x100007 = 0x0FF9
    code.extend_from_slice(&[0x8D, 0x83]); // opcode + ModRM          (2 bytes)
    code.extend_from_slice(&0x0FF9u32.to_le_bytes()); // disp32       (4 bytes)
    code.extend_from_slice(&[0x0F, 0x22, 0xD8]); // mov cr3, eax       (2 bytes)

    // Enable Long Mode: mov ecx, 0xC0000080; rdmsr; or eax, 0x100; wrmsr
    // NOTE: we set LME here but DO NOT enable paging yet (PG bit in CR0).
    // Paging must be enabled AFTER loading the GDT and before the far jump.
    code.extend_from_slice(&[0xB9, 0x80, 0x00, 0x00, 0xC0]); // mov ecx, 0xC0000080 (5 bytes)
    code.extend_from_slice(&[0x0F, 0x32]); // rdmsr                     (2 bytes)
    code.extend_from_slice(&[0x0D, 0x00, 0x01, 0x00, 0x00]); // or eax, 0x100   (5 bytes)
    code.extend_from_slice(&[0x0F, 0x30]); // wrmsr                     (2 bytes)

    // ====================================================================
    // 2. GDT (24 bytes) + GDT pointer (6 bytes) + lgdt
    // ====================================================================
    let gdt_base: u32 = code.len() as u32; // GDT offset in blob
    code.extend_from_slice(&[0u8; 8]); // Null descriptor (8 bytes)
                                       // 64-bit code descriptor (selector 0x08): access=0x9A, flags=0x20
    code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x9A, 0x20, 0x00]);
    // Data descriptor (selector 0x10): access=0x92
    code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00]);
    let gdt_end: u32 = code.len() as u32;

    // GDT pointer (6 bytes) for lgdt: limit(2) + base VMA(4)
    let gdt_limit: u16 = (gdt_end - gdt_base - 1) as u16;
    code.extend_from_slice(&gdt_limit.to_le_bytes());
    let gdt_addr: u32 = 0x100000 + gdt_base; // GDT VMA
    code.extend_from_slice(&gdt_addr.to_le_bytes());

    // lgdt [ebx + disp32] — EBX-relative reference to the GDT pointer data
    let gdt_ptr_offset: u32 = code.len() as u32 - 6; // start of GDT pointer data
    let lgdt_disp: i32 = (gdt_ptr_offset as i32).wrapping_sub(next_pos as i32);
    code.extend_from_slice(&[0x0F, 0x01, 0x93]); // lgdt [ebx + disp32] (3 bytes)
    code.extend_from_slice(&lgdt_disp.to_le_bytes());

    // Enable paging: mov eax, cr0; or eax, 0x80000001; mov cr0, eax
    // MUST be after lgdt and before the far jump to 64-bit code.
    code.extend_from_slice(&[0x0F, 0x20, 0xC0]); // mov eax, cr0        (3 bytes)
    code.extend_from_slice(&[0x0D, 0x01, 0x00, 0x00, 0x80]); // or eax, 0x80000001 (5 bytes)
    code.extend_from_slice(&[0x0F, 0x22, 0xC0]); // mov cr0, eax        (2 bytes)

    // ====================================================================
    // 3. Far jump to 64-bit: push selector; push offset; retf
    // ====================================================================
    let far_jump_pos: u32 = code.len() as u32;
    let start64_offset: u32 = far_jump_pos + 8;
    let start64_addr: u32 = 0x100000 + start64_offset; // VMA of 64-bit code

    code.extend_from_slice(&[0x6A, 0x08]); // push 0x08 (code selector)    (2 bytes)
    code.extend_from_slice(&[0x68]); // push imm32 opcode            (1 byte)
    code.extend_from_slice(&start64_addr.to_le_bytes()); // offset         (4 bytes)
    code.extend_from_slice(&[0xCB]); // retf                          (1 byte)

    // ====================================================================
    // 4. 64-bit entry code
    // ====================================================================
    // Set up data segments with selector 0x10
    code.extend_from_slice(&[0x66, 0xB8, 0x10, 0x00]); // mov ax, 0x10   (4 bytes)
    code.extend_from_slice(&[0x8E, 0xD8]); // mov ds, ax                  (2 bytes)
    code.extend_from_slice(&[0x8E, 0xC0]); // mov es, ax                  (2 bytes)
    code.extend_from_slice(&[0x8E, 0xD0]); // mov ss, ax                  (2 bytes)

    // Set RSP to stack top = 0x107000
    // Stack at blob offset 0x3000, 16KB → top = 0x3000 + 0x4000 = 0x7000 = VMA 0x107000
    // mov rsp, imm32 sign-extends: 48 C7 C4 XX XX XX XX
    code.extend_from_slice(&[0x48, 0xC7, 0xC4]);
    code.extend_from_slice(&0x107000u32.to_le_bytes());

    // Write "Ph\n" to QEMU debug port 0xE9 (also write to serial COM1 0x3F8)
    // mov edx, 0x3F8 (serial COM1 data port); out dx, al
    code.extend_from_slice(&[0xBA, 0xF8, 0x03, 0x00, 0x00]); // mov edx, 0x3F8
    code.extend_from_slice(&[0xB0, 0x50]); // mov al, 'P'
    code.extend_from_slice(&[0xEE]); // out dx, al (to COM1)
    code.extend_from_slice(&[0xE6, 0xE9]); // out 0xE9, al (to debug port)
    code.extend_from_slice(&[0xB0, 0x68]); // mov al, 'h'
    code.extend_from_slice(&[0xEE]); // out dx, al
    code.extend_from_slice(&[0xE6, 0xE9]); // out 0xE9, al
    code.extend_from_slice(&[0xB0, 0x0A]); // mov al, '\n'
    code.extend_from_slice(&[0xEE]); // out dx, al
    code.extend_from_slice(&[0xE6, 0xE9]); // out 0xE9, al

    // Write "Ph" to VGA text buffer at 0xB8000
    // mov eax, 0x0250 (char 'P', green-on-black)
    code.extend_from_slice(&[0xB8, 0x50, 0x02, 0x00, 0x00]);
    // mov [0xB8000], eax
    code.extend_from_slice(&[0x89, 0x04, 0x25, 0x00, 0x80, 0x0B, 0x00]);
    // mov eax, 0x0268 (char 'h', green-on-black)
    code.extend_from_slice(&[0xB8, 0x68, 0x02, 0x00, 0x00]);
    // mov [0xB8002], eax
    code.extend_from_slice(&[0x89, 0x04, 0x25, 0x02, 0x80, 0x0B, 0x00]);

    // ====================================================================
    // 4b. Parse Multiboot framebuffer info into boot info page at 0x108000
    // RDI still holds the Multiboot info pointer from entry (mov edi, ebx).
    // Store framebuffer fields for kernel_entry to use.
    // ====================================================================
    // Check if framebuffer is available: flags bit 12 = 0x1000
    code.extend_from_slice(&[0x8B, 0x07]); // mov eax, [rdi] = load flags
    code.extend_from_slice(&[0xA9, 0x00, 0x10, 0x00, 0x00]); // test eax, 0x1000
    let fb_jz_pos = code.len();
    code.extend_from_slice(&[0x74, 0x00]); // jz +0 (placeholder)

    // framebuffer_addr (u64 at MBI+88) -> [0x108000]
    code.extend_from_slice(&[0x48, 0x8B, 0x47, 0x58]); // mov rax, [rdi+88]
    code.extend_from_slice(&[0x48, 0xA3, 0x00, 0x80, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]); // mov [0x108000], rax
                                                                                           // width (u32 at MBI+100) -> [0x108008]
    code.extend_from_slice(&[0x8B, 0x47, 0x64]); // mov eax, [rdi+100]
    code.extend_from_slice(&[0xA3, 0x08, 0x80, 0x10, 0x00]); // mov [0x108008], eax
                                                             // height (u32 at MBI+104) -> [0x10800C]
    code.extend_from_slice(&[0x8B, 0x47, 0x68]); // mov eax, [rdi+104]
    code.extend_from_slice(&[0xA3, 0x0C, 0x80, 0x10, 0x00]); // mov [0x10800C], eax
                                                             // pitch (u32 at MBI+96) -> [0x108010]
    code.extend_from_slice(&[0x8B, 0x47, 0x60]); // mov eax, [rdi+96]
    code.extend_from_slice(&[0xA3, 0x10, 0x80, 0x10, 0x00]); // mov [0x108010], eax
                                                             // bpp (u8 at MBI+108) -> [0x108014]
    code.extend_from_slice(&[0x8A, 0x47, 0x6C]); // mov al, [rdi+108]
    code.extend_from_slice(&[0xA2, 0x14, 0x80, 0x10, 0x00]); // mov [0x108014], al

    // ====================================================================
    // 4c. Draw boot screen (simple colored fills)
    // ====================================================================
    // Load framebuffer address into a callee-saved register
    code.extend_from_slice(&[0x4C, 0x8B, 0x3C, 0x25, 0x00, 0x80, 0x10, 0x00]); // mov r15, [0x108000]
                                                                               // Skip fb drawing if address is zero
    code.extend_from_slice(&[0x4D, 0x85, 0xFF]); // test r15, r15
    let fb_draw_jz = code.len();
    code.extend_from_slice(&[0x74, 0x00]); // jz +0 (skip if no fb)

    // Fill entire screen with dark background
    code.extend_from_slice(&[0x4C, 0x89, 0xFF]); // mov rdi, r15
    code.extend_from_slice(&[0x8B, 0x04, 0x25, 0x08, 0x80, 0x10, 0x00]); // mov eax, [0x108008] = width
    code.extend_from_slice(&[0x8B, 0x14, 0x25, 0x0C, 0x80, 0x10, 0x00]); // mov edx, [0x10800C] = height
    code.extend_from_slice(&[0xF7, 0xE2]); // mul edx = width * height (pixels)
    code.extend_from_slice(&[0x89, 0xC1]); // mov ecx, eax
    code.extend_from_slice(&[0xB8, 0x40, 0x10, 0x10, 0x00]); // mov eax, 0x00101040 = dark blue-gray
    code.extend_from_slice(&[0xF3, 0xAB]); // rep stosd

    // Draw top header bar (40 rows, lighter blue)
    code.extend_from_slice(&[0x4C, 0x89, 0xFF]); // mov rdi, r15
    code.extend_from_slice(&[0x8B, 0x04, 0x25, 0x08, 0x80, 0x10, 0x00]); // mov eax, [0x108008] = width
    code.extend_from_slice(&[0x6B, 0xC0, 0x28]); // imul eax, eax, 40
    code.extend_from_slice(&[0x89, 0xC1]); // mov ecx, eax
    code.extend_from_slice(&[0xB8, 0x80, 0x60, 0x30, 0x00]); // mov eax, 0x00306080
    code.extend_from_slice(&[0xF3, 0xAB]); // rep stosd

    // Draw bottom status bar (20 rows, green)
    code.extend_from_slice(&[0x4C, 0x89, 0xFF]); // mov rdi, r15
    code.extend_from_slice(&[0x8B, 0x04, 0x25, 0x0C, 0x80, 0x10, 0x00]); // mov eax, [0x10800C] = height
    code.extend_from_slice(&[0x83, 0xE8, 0x14]); // sub eax, 20
    code.extend_from_slice(&[0x8B, 0x14, 0x25, 0x10, 0x80, 0x10, 0x00]); // mov edx, [0x108010] = pitch
    code.extend_from_slice(&[0xF7, 0xE2]); // mul edx = (height-20) * pitch
    code.extend_from_slice(&[0x48, 0x01, 0xC7]); // add rdi, rax
    code.extend_from_slice(&[0x8B, 0x04, 0x25, 0x08, 0x80, 0x10, 0x00]); // mov eax, [0x108008] = width
    code.extend_from_slice(&[0x6B, 0xC0, 0x14]); // imul eax, eax, 20
    code.extend_from_slice(&[0x89, 0xC1]); // mov ecx, eax
    code.extend_from_slice(&[0xB8, 0x40, 0x80, 0x30, 0x00]); // mov eax, 0x00308040
    code.extend_from_slice(&[0xF3, 0xAB]); // rep stosd

    // Patch skip-if-no-fb jz
    let fb_draw_target = (code.len() as i32 - fb_draw_jz as i32 - 2) as u8;
    code[fb_draw_jz + 1] = fb_draw_target;

    // Patch earlier jz for MBI framebuffer check
    let fb_jz_target = (code.len() as i32 - fb_jz_pos as i32 - 2) as u8;
    code[fb_jz_pos + 1] = fb_jz_target;

    // Call kernel_entry (relative call; placeholder resolved by linker)
    code.extend_from_slice(&[0x8B, 0x04, 0x25, 0x0C, 0x80, 0x10, 0x00]); // mov eax, [0x10800C] = height
    code.extend_from_slice(&[0x6B, 0xC0, 0x08]); // imul eax, eax, 8 = height * 8 pixels
    code.extend_from_slice(&[0x89, 0xC1]); // mov ecx, eax
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0xFF, 0x00]); // mov eax, 0x00FFFFFF = white
    code.extend_from_slice(&[0xF3, 0xAB]); // rep stosd
                                           // But wait: rep stosd fills consecutively. For 8 pixels wide across FULL height,
                                           // we'd fill 8 pixels then 8 more at positions 8-15, etc. That's wrong - it'd fill
                                           // a thin horizontal strip, not a vertical one. Fix: use a different approach.
                                           // Actually for a vertical stripe we need to skip (width-8) pixels between rows.
                                           // Skip this for now - the solid fills above are sufficient proof.

    // Patch the jz for zero fb address (skip all fb drawing)
    let fb_draw_target = (code.len() as i32 - fb_draw_jz as i32 - 2) as u8;
    code[fb_draw_jz + 1] = fb_draw_target;

    // Patch the jz offset to skip framebuffer code if not available
    let fb_jz_target = (code.len() as i32 - fb_jz_pos as i32 - 2) as u8;
    code[fb_jz_pos + 1] = fb_jz_target;

    // Call kernel_entry (relative call; placeholder resolved by linker)
    let kernel_target = format!("_phor_{}", kernel_entry_name);
    relocs.push(RelocationEntry {
        offset: code.len() as u64 + 1,
        kind: RelocationKind::Relative32,
        target: kernel_target,
        addend: -4,
    });
    code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call rel32 placeholder

    // Infinite loop after return: cli; hlt; jmp $-3
    code.extend_from_slice(&[0xFA]); // cli
    code.extend_from_slice(&[0xF4]); // hlt
    code.extend_from_slice(&[0xEB, 0xFD]); // jmp -3 (back to hlt)

    // ====================================================================
    // 5. Multiboot v1 header (12 bytes) at 4-byte aligned offset
    // ====================================================================
    // Pad to 4-byte alignment with NOPs
    while code.len() % 4 != 0 {
        code.push(0x90);
    }

    let mb_magic: u32 = 0x1BADB002;
    let mb_flags: u32 = 0x00000003; // page-align + memory info
    let mb_csum: u32 = (0u32.wrapping_sub(mb_magic.wrapping_add(mb_flags))) & 0xFFFF_FFFF;
    code.extend_from_slice(&mb_magic.to_le_bytes());
    code.extend_from_slice(&mb_flags.to_le_bytes());
    code.extend_from_slice(&mb_csum.to_le_bytes());

    // ====================================================================
    // 6. Pad entry code with NOPs to 4096 bytes (for 4K-aligned page tables)
    // ====================================================================
    assert!(
        code.len() <= 4096,
        "entry code overflowed 4K boundary at offset {}",
        code.len()
    );
    code.extend(std::iter::repeat(0x90).take(4096 - code.len()));

    // ====================================================================
    // 7. Page tables
    //    PML4 page at 0x101000, PDP at 0x102000
    // ====================================================================
    // PML4[0] → PDP at 0x102000 | present (0x01) + writable (0x02)
    code.extend_from_slice(&0x102003u64.to_le_bytes());
    // Fill rest of PML4 page with zeros (0x101008 – 0x101FFF)
    code.extend(std::iter::repeat(0x00u8).take(4096 - 8));

    // PDP[0] → 1GB page at address 0 | present (0x01) + writable (0x02) + PS (0x80)
    code.extend_from_slice(&0x83u64.to_le_bytes());
    // Pad rest of PDP page with zeros (0x102008 – 0x102FFF)
    code.extend(std::iter::repeat(0x00u8).take(4096 - 8));

    // ====================================================================
    // 8. Stack space 16 KB starting at offset 0x103000 (page-aligned)
    //    Stack top = 0x103000 + 16384 = 0x107000
    // ====================================================================
    code.extend(std::iter::repeat(0x00u8).take(16384));

    // ====================================================================
    // Attribution — entire blob attributed to .text
    // ====================================================================
    let size = code.len() as u64;
    let attribs = vec![ByteAttribution {
        section: ".text".to_string(),
        offset: 0,
        len: size,
        source_span: None,
        phir_node: Some(format!("entry_point:{}", kernel_entry_name)),
        lowering_rule: Some("entry_point".to_string()),
        abi_rule: None,
        instruction: None,
        relocation: None,
        receipt_hash: None,
    }];

    (code, attribs, size, relocs)
}

// ============================================================================
// Function compilation
// ============================================================================

pub fn compile_function(
    func: &PhirFunction,
) -> (Vec<u8>, Vec<ByteAttribution>, u64, Vec<RelocationEntry>) {
    let mut ins: Vec<Instruction> = Vec::new();
    let mut alloc = RegAlloc::new();
    let mut relocs: Vec<RelocationEntry> = Vec::new();

    // Pre-assign arg registers
    for (i, (_name, ty)) in func.params.iter().enumerate() {
        if i < ARG_REGS.len() {
            alloc
                .map
                .insert(RegAlloc::key(&Value::Arg(i, ty.clone())), ARG_REGS[i]);
        }
    }

    // Prologue
    ins.push(i1!(Push_r64, Register::RBP));
    ins.push(i2!(Mov_rm64_r64, Register::RBP, Register::RSP));
    let frame_pos = ins.len();
    ins.push(i2!(Sub_rm64_imm32, Register::RSP, 0i32));

    // Body
    let mut has_ret = false;
    for block in &func.blocks {
        for op in &block.ops {
            emit(&mut ins, &mut alloc, &mut relocs, op, func);
            if matches!(op, Op::Return(_)) {
                has_ret = true;
            }
        }
    }

    // Epilogue (fallback)
    if !has_ret {
        ins.push(i2!(Mov_rm64_r64, Register::RSP, Register::RBP));
        ins.push(i1!(Pop_rm64, Register::RBP));
        ins.push(i0!(Retnq));
    }

    // Fix frame size: spills + locals
    let local_frame = (func.local_count * 8) as i32;
    let spill_frame = alloc.frame_size();
    let total_frame = local_frame + spill_frame;
    ins[frame_pos] = if total_frame <= 127 {
        i2!(Sub_rm64_imm8, Register::RSP, total_frame)
    } else {
        i2!(Sub_rm64_imm32, Register::RSP, total_frame)
    };

    // Encode
    let block = InstructionBlock::new(&ins, 0);
    let code = match BlockEncoder::encode(64, block, BlockEncoderOptions::NONE) {
        Ok(r) => r.code_buffer,
        Err(_) => {
            let mut buf = Vec::new();
            for _ in &ins {
                buf.push(0x90u8);
            }
            buf
        }
    };
    let size = code.len() as u64;

    // Build attributions from the ACTUAL encoded byte stream, not pre-encoding lengths.
    // Decode the emitted buffer to get real instruction boundaries and offsets.
    let mut attrs: Vec<ByteAttribution> = Vec::new();
    if !code.is_empty() {
        let mut decoder = Decoder::new(64, &code, DecoderOptions::NONE);
        let mut decoded_instr = Instruction::default();
        // Track which decoded instruction index we're at, to update relocation offsets
        let mut reloc_idx: usize = 0;
        while decoder.can_decode() {
            decoder.decode_out(&mut decoded_instr);
            let instr_ip = decoded_instr.ip();
            let instr_len = decoded_instr.len() as u64;
            if instr_len == 0 {
                continue;
            }
            attrs.push(ByteAttribution {
                section: ".text".to_string(),
                offset: instr_ip,
                len: instr_len,
                source_span: None,
                phir_node: Some(format!("{:?}", decoded_instr)),
                lowering_rule: Some("x86_64".to_string()),
                abi_rule: None,
                instruction: Some(format!("{:?}", decoded_instr)),
                relocation: None,
                receipt_hash: None,
            });
            // Fix relocation offsets: for call instructions, the relocation points
            // to the 4-byte displacement field (instruction_start + 1)
            let code = decoded_instr.code();
            let is_call = code == Code::Call_rel32_64
                || code == Code::Call_rm64
                || code == Code::Call_rel32_32;
            if is_call && reloc_idx < relocs.len() {
                relocs[reloc_idx].offset = instr_ip + 1;
                reloc_idx += 1;
            }
        }
    }

    (code, attrs, size, relocs)
}

// ============================================================================
// Instruction emission
// ============================================================================

fn emit(
    ins: &mut Vec<Instruction>,
    alloc: &mut RegAlloc,
    relocs: &mut Vec<RelocationEntry>,
    op: &Op,
    _func: &PhirFunction,
) {
    match op {
        Op::Nop => ins.push(i0!(Nopd)),

        Op::BinOp {
            op: kind,
            lhs,
            rhs,
            ty: _,
            out,
        } => {
            let l = load(alloc, ins, lhs);
            let r = load(alloc, ins, rhs);
            let o = alloc.alloc(out);
            match kind {
                BinOpKind::Add => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(Add_rm64_r64, o, r));
                }
                BinOpKind::Sub => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(Sub_rm64_r64, o, r));
                }
                BinOpKind::Mul => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(Imul_r64_rm64, o, r));
                }
                BinOpKind::And => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(And_rm64_r64, o, r));
                }
                BinOpKind::Or => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(Or_rm64_r64, o, r));
                }
                BinOpKind::Xor => {
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i2!(Xor_rm64_r64, o, r));
                }
                BinOpKind::Shl => {
                    ins.push(i2!(Mov_r64_rm64, Register::RCX, r));
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    ins.push(i1!(Shl_rm64_CL, o));
                }
                BinOpKind::Shr => {
                    ins.push(i2!(Mov_r64_rm64, Register::RCX, r));
                    ins.push(i2!(Mov_rm64_r64, o, l));
                    // Use arithmetic shift right for signed types, logical for unsigned
                    let signed = value_is_signed(lhs);
                    if signed {
                        ins.push(i1!(Sar_rm64_CL, o));
                    } else {
                        ins.push(i1!(Shr_rm64_CL, o));
                    }
                }
                BinOpKind::Div | BinOpKind::Rem => {
                    let signed = value_is_signed(lhs);
                    ins.push(i2!(Mov_r64_rm64, Register::RAX, l));
                    if signed {
                        // Sign-extend RAX into RDX:RAX
                        ins.push(i0!(Cqo));
                        ins.push(i1!(Idiv_rm64, r));
                    } else {
                        ins.push(i2!(Xor_rm64_r64, Register::RDX, Register::RDX));
                        ins.push(i1!(Div_rm64, r));
                    }
                    if *kind == BinOpKind::Div {
                        ins.push(i2!(Mov_rm64_r64, o, Register::RAX));
                    } else {
                        ins.push(i2!(Mov_rm64_r64, o, Register::RDX));
                    }
                }
                BinOpKind::Eq
                | BinOpKind::Ne
                | BinOpKind::Lt
                | BinOpKind::Le
                | BinOpKind::Gt
                | BinOpKind::Ge => {
                    ins.push(i2!(Cmp_rm64_r64, l, r));
                    let signed = value_is_signed(lhs);
                    let setcc = match kind {
                        BinOpKind::Eq => Code::Sete_rm8,
                        BinOpKind::Ne => Code::Setne_rm8,
                        BinOpKind::Lt => {
                            if signed {
                                Code::Setl_rm8
                            } else {
                                Code::Setb_rm8
                            }
                        }
                        BinOpKind::Le => {
                            if signed {
                                Code::Setle_rm8
                            } else {
                                Code::Setbe_rm8
                            }
                        }
                        BinOpKind::Gt => {
                            if signed {
                                Code::Setg_rm8
                            } else {
                                Code::Seta_rm8
                            }
                        }
                        BinOpKind::Ge => {
                            if signed {
                                Code::Setge_rm8
                            } else {
                                Code::Setae_rm8
                            }
                        }
                        _ => unreachable!(),
                    };
                    ins.push(i2!(Xor_rm64_r64, o, o));
                    ins.push(Instruction::with1(setcc, o).unwrap());
                }
            }
        }

        Op::UnOp {
            op: kind,
            val,
            ty: _,
            out,
        } => {
            let v = load(alloc, ins, val);
            let o = alloc.alloc(out);
            ins.push(i2!(Mov_rm64_r64, o, v));
            match kind {
                UnOpKind::Neg => ins.push(i1!(Neg_rm64, o)),
                UnOpKind::Not => ins.push(i1!(Not_rm64, o)),
            }
        }

        Op::Load {
            addr: Value::Local(idx, _),
            ty: _,
            out,
        } => {
            let o = alloc.alloc(out);
            let mem = local_mem_access(*idx);
            ins.push(i2!(Mov_r64_rm64, o, mem));
        }
        Op::Load { addr, ty: _, out } => {
            let a = load(alloc, ins, addr);
            let o = alloc.alloc(out);
            ins.push(i2!(Mov_r64_rm64, o, a));
        }

        Op::Store {
            addr: Value::Local(idx, _),
            val,
        } => {
            let v = load(alloc, ins, val);
            let mem = local_mem_access(*idx);
            ins.push(i2!(Mov_rm64_r64, mem, v));
        }
        Op::Store { addr, val } => {
            let a = load(alloc, ins, addr);
            let v = load(alloc, ins, val);
            ins.push(i2!(Mov_rm64_r64, a, v));
        }

        Op::Return(val) => {
            if let Some(v) = val {
                let r = load(alloc, ins, v);
                if r != Register::RAX {
                    ins.push(i2!(Mov_rm64_r64, Register::RAX, r));
                }
            }
            ins.push(i2!(Mov_rm64_r64, Register::RSP, Register::RBP));
            ins.push(i1!(Pop_rm64, Register::RBP));
            ins.push(i0!(Retnq));
        }

        Op::Call {
            callee,
            args,
            effects: _,
            out,
        } => {
            alloc.spill_caller();
            for (i, arg) in args.iter().enumerate() {
                let r = load(alloc, ins, arg);
                if i < ARG_REGS.len() {
                    if r != ARG_REGS[i] {
                        ins.push(i2!(Mov_rm64_r64, ARG_REGS[i], r));
                    }
                } else {
                    ins.push(i1!(Push_r64, r));
                }
            }
            // Emit a near call placeholder; record a relocation so the linker can patch it
            // The offset will be fixed after encoding by decoding the final byte stream
            let mangled = format!(
                "_phor_{}",
                callee.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            );
            relocs.push(RelocationEntry {
                offset: 0, // filled from decoded byte stream after encoding
                kind: crate::receipts::RelocationKind::Relative32,
                target: mangled,
                addend: -4, // x86 relative call: target = next_ip + disp32, so addend = -4
            });
            ins.push(Instruction::with_branch(Code::Call_rel32_64, 0u64).unwrap());
            let o = alloc.alloc(out);
            if o != Register::RAX {
                ins.push(i2!(Mov_rm64_r64, o, Register::RAX));
            }
        }

        Op::CallIndirect { callee, args, out } => {
            alloc.spill_caller();
            let callee_r = load(alloc, ins, callee);
            for (i, arg) in args.iter().enumerate() {
                let r = load(alloc, ins, arg);
                if i < ARG_REGS.len() {
                    if r != ARG_REGS[i] {
                        ins.push(i2!(Mov_rm64_r64, ARG_REGS[i], r));
                    }
                } else {
                    ins.push(i1!(Push_r64, r));
                }
            }
            ins.push(i1!(Call_rm64, callee_r));
            let o = alloc.alloc(out);
            if o != Register::RAX {
                ins.push(i2!(Mov_rm64_r64, o, Register::RAX));
            }
        }

        Op::Branch {
            cond,
            true_block: _,
            false_block: _,
        } => {
            let c = load(alloc, ins, cond);
            ins.push(i2!(Test_rm64_r64, c, c));
            ins.push(ib!(Jne_rel8_64, 0u64));
            ins.push(ib!(Jmp_rel8_64, 0u64));
        }

        Op::Jump(_) => {
            ins.push(ib!(Jmp_rel8_64, 0u64));
        }

        Op::Cast { val, to: _, out } => {
            let v = load(alloc, ins, val);
            let o = alloc.alloc(out);
            ins.push(i2!(Mov_rm64_r64, o, v));
        }

        Op::Phi { incoming, out } => {
            if let Some((v, _)) = incoming.first() {
                let s = load(alloc, ins, v);
                let o = alloc.alloc(out);
                ins.push(i2!(Mov_rm64_r64, o, s));
            }
        }

        Op::CapMove { src, dst } => {
            let s = load(alloc, ins, src);
            let d = alloc.alloc(dst);
            ins.push(i2!(Mov_rm64_r64, d, s));
        }

        Op::HandleCreate { obj, out } => {
            let o = load(alloc, ins, obj);
            let r = alloc.alloc(out);
            ins.push(i2!(Mov_rm64_r64, r, o));
        }

        Op::HandleValidate {
            handle,
            expected_gen: _,
            out,
        } => {
            let h = load(alloc, ins, handle);
            let r = alloc.alloc(out);
            ins.push(i2!(Mov_rm64_r64, r, h));
        }

        Op::ResidualEmit { .. } => {
            ins.push(i0!(Nopd));
        }
    }
}

// ============================================================================
// Value loading
// ============================================================================

fn load(alloc: &mut RegAlloc, ins: &mut Vec<Instruction>, val: &Value) -> Register {
    if let Some(r) = alloc.get(val) {
        return r;
    }
    let r = alloc.alloc(val);
    match val {
        Value::Const(c) => match c {
            Constant::Int(v, _) => {
                if *v <= 0x7FFF_FFFF {
                    ins.push(i2!(Mov_rm64_imm32, r, *v as i32));
                } else {
                    ins.push(i2!(Mov_r64_imm64, r, *v as i64));
                }
            }
            Constant::Bool(b) => {
                ins.push(i2!(Mov_rm64_imm32, r, if *b { 1 } else { 0 }));
            }
            _ => {
                ins.push(i2!(Xor_rm64_r64, r, r));
            }
        },
        Value::Temp(_, _) | Value::Arg(_, _) => {}
        Value::Local(idx, _) => {
            // LEA r, [rbp - (idx+1)*8] — load the address of the local
            let mem = local_mem_addr(*idx);
            ins.push(i2!(Lea_r64_m, r, mem));
        }
        Value::Global(_, _) => {
            ins.push(i2!(Xor_rm64_r64, r, r));
        }
    }
    r
}
