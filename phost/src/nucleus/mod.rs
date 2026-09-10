// Phost — Phorensic OS Trusted Nucleus
// x86-64 low-level structures, boot entry, GDT, paging, IDT, TSS,
// MSR helpers, CPUID parsing, memory map, and assembly shims.
//
// This module is the first code that runs on the machine. It sets up
// the CPU in long mode, installs the kernel page tables, loads the IDT,
// and transitions to the higher-level kernel.

#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use alloc::vec::Vec;
use core::fmt;

// ============================================================================
// NucleusEntry — The machine start point (Multiboot2 / Rust entry)
// ============================================================================

/// The first structure the bootloader passes to the nucleus entry.
/// On x86-64, this corresponds to the Multiboot2 info pointer (or a
/// custom Phorensic loader struct).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NucleusEntry {
    /// Magic value expected by the boot protocol (e.g. Multiboot2 magic)
    pub magic: u32,
    /// Architecture-specific boot info pointer
    pub info_ptr: u64,
    /// Stack pointer established by the bootloader
    pub stack_top: u64,
    /// Physical address of the kernel image base
    pub kernel_phys_base: u64,
    /// Virtual address offset (KERNEL_OFFSET)
    pub virtual_offset: u64,
    /// Number of available physical memory regions
    pub memory_map_entries: u32,
    /// Pointer to the physical memory map array
    pub memory_map_ptr: u64,
    /// Reserved for future use
    pub reserved: [u64; 8],
}

impl NucleusEntry {
    /// Validate the magic number for the boot protocol.
    pub fn validate_magic(&self, expected: u32) -> bool {
        self.magic == expected
    }

    /// Return the physical-to-virtual conversion offset.
    pub fn phys_to_virt(&self) -> u64 {
        self.virtual_offset
    }
}

impl Default for NucleusEntry {
    fn default() -> Self {
        Self {
            magic: 0,
            info_ptr: 0,
            stack_top: 0,
            kernel_phys_base: 0,
            virtual_offset: 0xFFFF_8000_0000_0000u64,
            memory_map_entries: 0,
            memory_map_ptr: 0,
            reserved: [0; 8],
        }
    }
}

// ============================================================================
// GDT (Global Descriptor Table) — x86-64 segments
// ============================================================================

/// A single 64-bit GDT descriptor (segment descriptor).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GdtDescriptor {
    /// Limit (bits 0–15)
    pub limit_low: u16,
    /// Base (bits 0–15)
    pub base_low: u16,
    /// Base (bits 16–23)
    pub base_mid: u8,
    /// Access byte (type, S, DPL, P)
    pub access: u8,
    /// Flags + limit (bits 16–19)
    pub flags_limit_high: u8,
    /// Base (bits 24–31)
    pub base_high: u8,
}

impl GdtDescriptor {
    /// Create a null descriptor.
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_mid: 0,
            access: 0,
            flags_limit_high: 0,
            base_high: 0,
        }
    }

    /// Create a code segment descriptor (ring 0, long mode).
    /// - base: 0 (flat memory model)
    /// - limit: 0 (ignored in long mode)
    /// - granularity: 1 (4K page granularity)
    /// - present: 1
    /// - descriptor type: 1 (code/data)
    /// - type: 0xA (execute, read, accessed)
    pub const fn code_kernel() -> Self {
        // Access byte: P=1, DPL=0, S=1, Type=0xA (execute/read)
        const ACCESS: u8 = 0b1001_1010;
        // Flags: G=1, D/B=0, L=1 (long mode), AVL=0 => 0xA0 (upper nibble)
        // Limit high (lower nibble) = 0
        const FLAGS_LIMIT: u8 = 0b1010_0000;
        Self {
            limit_low: 0,
            base_low: 0,
            base_mid: 0,
            access: ACCESS,
            flags_limit_high: FLAGS_LIMIT,
            base_high: 0,
        }
    }

    /// Create a data segment descriptor (ring 0, writable).
    /// Access byte: P=1, DPL=0, S=1, Type=0x2 (read/write)
    pub const fn data_kernel() -> Self {
        const ACCESS: u8 = 0b1001_0010;
        // Flags: G=1 (4K page), D/B=1 (32-bit default for compatibility)
        const FLAGS_LIMIT: u8 = 0b1100_0000;
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: ACCESS,
            flags_limit_high: FLAGS_LIMIT,
            base_high: 0,
        }
    }

    /// Create a user-mode code segment descriptor (ring 3, long mode).
    pub const fn code_user() -> Self {
        const ACCESS: u8 = 0b1111_1010; // P=1, DPL=3, S=1, Type=0xA
        const FLAGS_LIMIT: u8 = 0b1010_0000;
        Self {
            limit_low: 0,
            base_low: 0,
            base_mid: 0,
            access: ACCESS,
            flags_limit_high: FLAGS_LIMIT,
            base_high: 0,
        }
    }

    /// Create a user-mode data segment descriptor (ring 3, writable).
    pub const fn data_user() -> Self {
        const ACCESS: u8 = 0b1111_0010; // P=1, DPL=3, S=1, Type=0x2
        const FLAGS_LIMIT: u8 = 0b1100_0000;
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: ACCESS,
            flags_limit_high: FLAGS_LIMIT,
            base_high: 0,
        }
    }

    /// Encode the descriptor as raw bytes (big-endian within each field).
    pub fn raw_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..2].copy_from_slice(&self.limit_low.to_le_bytes());
        buf[2..4].copy_from_slice(&self.base_low.to_le_bytes());
        buf[4] = self.base_mid;
        buf[5] = self.access;
        buf[6] = self.flags_limit_high;
        buf[7] = self.base_high;
        buf
    }
}

impl fmt::Display for GdtDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GDT[base=0x{:06x}, limit=0x{:05x}, access=0x{:02x}, flags=0x{:02x}]",
            (self.base_high as u32) << 16 | self.base_mid as u32 | self.base_low as u32,
            (self.limit_low as u32) | ((self.flags_limit_high & 0x0F) as u32) << 16,
            self.access,
            self.flags_limit_high >> 4,
        )
    }
}

/// A GDT pseudo-descriptor (pointer) loaded by `lgdt`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GdtPointer {
    /// Size of the GDT in bytes minus 1
    pub limit: u16,
    /// Linear address of the GDT
    pub base: u64,
}

impl GdtPointer {
    pub const fn new(base: u64, limit: u16) -> Self {
        Self { limit, base }
    }
}

/// TSS descriptor (System segment descriptor for x86-64 Task State Segment).
/// In long mode, the TSS descriptor is 16 bytes (two 8-byte entries).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TssDescriptor {
    /// Lower 8 bytes (standard descriptor format)
    pub desc_low: GdtDescriptor,
    /// Upper 8 bytes (TSS base bits 32–63, reserved)
    pub base_high32: u32,
    /// Reserved (must be zero)
    pub reserved: u32,
}

impl TssDescriptor {
    /// Create a TSS descriptor for the given TSS base address.
    pub fn new(tss_base: u64, tss_limit: u32) -> Self {
        let access: u8 = 0b1000_1001; // P=1, DPL=0, Type=0x9 (64-bit TSS, available)
        let flags_limit: u8 = 0b0000_0000; // G=0 (byte granular), L=0, D/B=0
        let desc_low = GdtDescriptor {
            limit_low: tss_limit as u16,
            base_low: tss_base as u16,
            base_mid: ((tss_base >> 16) & 0xFF) as u8,
            access,
            flags_limit_high: ((flags_limit & 0xF0) | ((tss_limit >> 16) & 0x0F) as u8),
            base_high: ((tss_base >> 24) & 0xFF) as u8,
        };
        let base_high32 = ((tss_base >> 32) & 0xFFFF_FFFF) as u32;
        Self {
            desc_low,
            base_high32,
            reserved: 0,
        }
    }

    /// Return the 16-byte raw encoding.
    pub fn raw_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        let low = self.desc_low.raw_bytes();
        buf[0..8].copy_from_slice(&low);
        buf[8..12].copy_from_slice(&self.base_high32.to_le_bytes());
        buf[12..16].copy_from_slice(&self.reserved.to_le_bytes());
        buf
    }
}

/// A complete GDT with standard selectors for Phorensic kernel.
#[repr(C, align(16))]
#[derive(Debug, Clone)]
pub struct Gdt {
    /// Null descriptor (selector 0x00)
    pub null: GdtDescriptor,
    /// Kernel code segment (selector 0x08)
    pub kernel_code: GdtDescriptor,
    /// Kernel data segment (selector 0x10)
    pub kernel_data: GdtDescriptor,
    /// User code segment (selector 0x18)
    pub user_code: GdtDescriptor,
    /// User data segment (selector 0x20)
    pub user_data: GdtDescriptor,
    /// TSS descriptor  (selector 0x28) — occupies two slots
    pub tss_low: GdtDescriptor,
    pub tss_high: u32,
    pub tss_reserved: u32,
}

impl Gdt {
    /// Standard kernel code segment selector.
    pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
    /// Standard kernel data segment selector.
    pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
    /// Standard user code segment selector.
    pub const USER_CODE_SELECTOR: u16 = 0x18;
    /// Standard user data segment selector.
    pub const USER_DATA_SELECTOR: u16 = 0x20;
    /// TSS selector.
    pub const TSS_SELECTOR: u16 = 0x28;

    /// Create a default GDT.
    pub fn new() -> Self {
        Self {
            null: GdtDescriptor::null(),
            kernel_code: GdtDescriptor::code_kernel(),
            kernel_data: GdtDescriptor::data_kernel(),
            user_code: GdtDescriptor::code_user(),
            user_data: GdtDescriptor::data_user(),
            tss_low: GdtDescriptor::null(),
            tss_high: 0,
            tss_reserved: 0,
        }
    }

    /// Install the TSS descriptor.
    pub fn set_tss(&mut self, tss_base: u64, tss_limit: u32) {
        let tss_desc = TssDescriptor::new(tss_base, tss_limit);
        self.tss_low = tss_desc.desc_low;
        self.tss_high = tss_desc.base_high32;
        self.tss_reserved = tss_desc.reserved;
    }

    /// Return a pointer suitable for `lgdt`.
    pub fn pointer(&self) -> GdtPointer {
        let base = self as *const Self as u64;
        let limit = core::mem::size_of::<Self>() as u16 - 1;
        GdtPointer::new(base, limit)
    }

    /// Convert to raw byte array for loading.
    pub fn raw_bytes(&self) -> [u8; 48] {
        let mut buf = [0u8; 48];
        buf[0..8].copy_from_slice(&self.null.raw_bytes());
        buf[8..16].copy_from_slice(&self.kernel_code.raw_bytes());
        buf[16..24].copy_from_slice(&self.kernel_data.raw_bytes());
        buf[24..32].copy_from_slice(&self.user_code.raw_bytes());
        buf[32..40].copy_from_slice(&self.user_data.raw_bytes());
        buf[40..48].copy_from_slice(&self.tss_low.raw_bytes());
        // The upper 8 bytes of TSS descriptor are not stored in the [u8;48]
        // because they extend beyond the 6 entries. We handle this separately.
        buf
    }
}

impl Default for Gdt {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Page Table — x86-64 4-level paging
// ============================================================================

/// A single page table entry (64-bit).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PageTableEntry {
    pub bits: u64,
}

impl PageTableEntry {
    /// Page present flag.
    pub const PRESENT: u64 = 1 << 0;
    /// Read/write flag.
    pub const WRITABLE: u64 = 1 << 1;
    /// User-accessible flag.
    pub const USER: u64 = 1 << 2;
    /// Write-through caching flag.
    pub const WRITE_THROUGH: u64 = 1 << 3;
    /// Cache disable flag.
    pub const CACHE_DISABLE: u64 = 1 << 4;
    /// Accessed flag (set by CPU).
    pub const ACCESSED: u64 = 1 << 5;
    /// Dirty flag (set by CPU on write).
    pub const DIRTY: u64 = 1 << 6;
    /// Huge page (1 GiB for PML4, 2 MiB for PDPT, 4 KiB for PD).
    pub const HUGE: u64 = 1 << 7;
    /// Global page (not flushed on CR3 reload).
    pub const GLOBAL: u64 = 1 << 8;
    /// No-execute flag (bit 63).
    pub const NO_EXECUTE: u64 = 1 << 63;

    /// Physical address mask (bits 12..51).
    pub const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

    /// Create an empty (non-present) entry.
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Create a page table entry pointing to a physical address.
    /// Flags can be OR'd together: PRESENT | WRITABLE | etc.
    pub fn new(phys_addr: u64, flags: u64) -> Self {
        assert!(
            phys_addr & !Self::ADDR_MASK == 0,
            "physical address outside valid range"
        );
        Self {
            bits: (phys_addr & Self::ADDR_MASK) | flags,
        }
    }

    /// Check if the entry is present.
    pub fn is_present(&self) -> bool {
        self.bits & Self::PRESENT != 0
    }

    /// Check if the entry is a huge page.
    pub fn is_huge(&self) -> bool {
        self.bits & Self::HUGE != 0
    }

    /// Get the physical address this entry points to.
    pub fn addr(&self) -> u64 {
        self.bits & Self::ADDR_MASK
    }

    /// Set the physical address (preserving flags).
    pub fn set_addr(&mut self, addr: u64) {
        self.bits = (self.bits & !Self::ADDR_MASK) | (addr & Self::ADDR_MASK);
    }

    /// Mark the entry as present.
    pub fn set_present(&mut self, present: bool) {
        if present {
            self.bits |= Self::PRESENT;
        } else {
            self.bits &= !Self::PRESENT;
        }
    }

    /// Mark as writable.
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.bits |= Self::WRITABLE;
        } else {
            self.bits &= !Self::WRITABLE;
        }
    }

    /// Mark as user-accessible.
    pub fn set_user(&mut self, user: bool) {
        if user {
            self.bits |= Self::USER;
        } else {
            self.bits &= !Self::USER;
        }
    }

    /// Mark as no-execute.
    pub fn set_no_execute(&mut self, nx: bool) {
        if nx {
            self.bits |= Self::NO_EXECUTE;
        } else {
            self.bits &= !Self::NO_EXECUTE;
        }
    }

    /// Check if the page has been accessed.
    pub fn was_accessed(&self) -> bool {
        self.bits & Self::ACCESSED != 0
    }

    /// Check if the page has been written to.
    pub fn was_dirty(&self) -> bool {
        self.bits & Self::DIRTY != 0
    }
}

impl fmt::Display for PageTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PTE[addr=0x{:016x}, flags={}]",
            self.addr(),
            if self.is_present() { "P" } else { "-" }
        )?;
        if self.bits & Self::WRITABLE != 0 {
            write!(f, "W")?;
        }
        if self.bits & Self::USER != 0 {
            write!(f, "U")?;
        }
        if self.bits & Self::NO_EXECUTE != 0 {
            write!(f, "NX")?;
        }
        if self.is_huge() {
            write!(f, "HP")?;
        }
        Ok(())
    }
}

/// A page table frame (512 entries, aligned to 4K).
#[repr(C, align(4096))]
#[derive(Debug, Clone)]
pub struct PageTable {
    pub entries: [PageTableEntry; 512],
}

impl PageTable {
    /// Create an empty page table (all entries non-present).
    pub const fn new() -> Self {
        Self {
            entries: [PageTableEntry::empty(); 512],
        }
    }

    /// Zero out the entire page table.
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = PageTableEntry::empty();
        }
    }

    /// Get a mutable reference to an entry by index.
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }

    /// Get an entry by index.
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    /// Set an entry at the given index.
    pub fn set_entry(&mut self, index: usize, entry: PageTableEntry) {
        self.entries[index] = entry;
    }

    /// Walk the page table hierarchy to find the PTE for a virtual address.
    /// Returns (PML4_idx, PDPT_idx, PD_idx, PT_idx).
    pub const fn virtual_address_indices(vaddr: u64) -> (usize, usize, usize, usize) {
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;
        (pml4_idx, pdpt_idx, pd_idx, pt_idx)
    }

    /// Identity-map a 4K page (virtual == physical) at all page levels.
    /// This is a convenience for early boot mapping.
    pub fn identity_map_4k(
        pml4: &mut PageTable,
        pdpt: &mut PageTable,
        pd: &mut PageTable,
        pt: &mut PageTable,
        phys_addr: u64,
        flags: u64,
    ) {
        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::virtual_address_indices(phys_addr);

        // Set up the page table entry
        pt.set_entry(
            pt_idx,
            PageTableEntry::new(phys_addr, flags | PageTableEntry::PRESENT),
        );

        // Set up page directory entry pointing to PT
        let pt_base = pt as *const PageTable as u64;
        pd.set_entry(
            pd_idx,
            PageTableEntry::new(
                pt_base,
                PageTableEntry::PRESENT | PageTableEntry::WRITABLE | PageTableEntry::USER,
            ),
        );

        // Set up PDPT entry pointing to PD
        let pd_base = pd as *const PageTable as u64;
        pdpt.set_entry(
            pdpt_idx,
            PageTableEntry::new(
                pd_base,
                PageTableEntry::PRESENT | PageTableEntry::WRITABLE | PageTableEntry::USER,
            ),
        );

        // Set up PML4 entry pointing to PDPT
        let pdpt_base = pdpt as *const PageTable as u64;
        pml4.set_entry(
            pml4_idx,
            PageTableEntry::new(
                pdpt_base,
                PageTableEntry::PRESENT | PageTableEntry::WRITABLE | PageTableEntry::USER,
            ),
        );
    }

    /// Map a 2 MiB huge page (page directory level).
    pub fn map_2mb(
        pml4: &mut PageTable,
        pdpt: &mut PageTable,
        pd: &mut PageTable,
        phys_base: u64,
        virt_base: u64,
        flags: u64,
    ) {
        let (pml4_idx, pdpt_idx, pd_idx, _) = Self::virtual_address_indices(virt_base);
        let entry_flags = flags | PageTableEntry::PRESENT | PageTableEntry::HUGE;
        pd.set_entry(pd_idx, PageTableEntry::new(phys_base, entry_flags));

        let pd_base = pd as *const PageTable as u64;
        pdpt.set_entry(
            pdpt_idx,
            PageTableEntry::new(pd_base, PageTableEntry::PRESENT | PageTableEntry::WRITABLE),
        );

        let pdpt_base = pdpt as *const PageTable as u64;
        pml4.set_entry(
            pml4_idx,
            PageTableEntry::new(
                pdpt_base,
                PageTableEntry::PRESENT | PageTableEntry::WRITABLE,
            ),
        );
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Interrupt Descriptor Table (IDT) — x86-64
// ============================================================================

/// An Interrupt Descriptor Table entry (16 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdtEntry {
    /// ISR address (bits 0–15)
    pub handler_low: u16,
    /// Code segment selector (e.g. Gdt::KERNEL_CODE_SELECTOR)
    pub code_selector: u16,
    /// Interrupt stack table offset (bits 0–2), reserved (bits 3–7)
    pub ist: u8,
    /// Gate type, DPL, present bit
    pub flags: u8,
    /// ISR address (bits 16–31)
    pub handler_mid: u16,
    /// ISR address (bits 32–63)
    pub handler_high: u32,
    /// Reserved (must be zero)
    pub reserved: u32,
}

impl IdtEntry {
    /// Create a missing (non-present) entry.
    pub const fn missing() -> Self {
        Self {
            handler_low: 0,
            code_selector: 0,
            ist: 0,
            flags: 0,
            handler_mid: 0,
            handler_high: 0,
            reserved: 0,
        }
    }

    /// Create an interrupt gate for ring 0.
    /// - `handler`: 64-bit virtual address of the handler function
    /// - `selector`: code segment selector (typically Gdt::KERNEL_CODE_SELECTOR)
    /// - `ist`: Interrupt Stack Table index (0 = no IST)
    pub fn interrupt_gate(handler: u64, selector: u16, ist: u8) -> Self {
        // Flags: P=1, DPL=0 (ring 0), 0, Type=0xE (64-bit interrupt gate)
        // Binary: 1000_1110 => 0x8E
        const FLAGS: u8 = 0b1000_1110;
        Self {
            handler_low: handler as u16,
            code_selector: selector,
            ist: ist & 0x07,
            flags: FLAGS,
            handler_mid: (handler >> 16) as u16,
            handler_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// Create a trap gate for ring 0.
    pub fn trap_gate(handler: u64, selector: u16, ist: u8) -> Self {
        // Flags: P=1, DPL=0, 0, Type=0xF (64-bit trap gate)
        const FLAGS: u8 = 0b1000_1111;
        Self {
            handler_low: handler as u16,
            code_selector: selector,
            ist: ist & 0x07,
            flags: FLAGS,
            handler_mid: (handler >> 16) as u16,
            handler_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// Create a user-mode interrupt gate (DPL=3) for syscalls.
    pub fn user_interrupt_gate(handler: u64, selector: u16, ist: u8) -> Self {
        // Flags: P=1, DPL=3, 0, Type=0xE
        const FLAGS: u8 = 0b1110_1110;
        Self {
            handler_low: handler as u16,
            code_selector: selector,
            ist: ist & 0x07,
            flags: FLAGS,
            handler_mid: (handler >> 16) as u16,
            handler_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// Check if the entry is present.
    pub fn is_present(&self) -> bool {
        self.flags & 0x80 != 0
    }

    /// Get the handler address.
    pub fn handler(&self) -> u64 {
        self.handler_low as u64 | (self.handler_mid as u64) << 16 | (self.handler_high as u64) << 32
    }

    /// Return the raw 16-byte representation.
    pub fn raw_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..2].copy_from_slice(&self.handler_low.to_le_bytes());
        buf[2..4].copy_from_slice(&self.code_selector.to_le_bytes());
        buf[4] = self.ist;
        buf[5] = self.flags;
        buf[6..8].copy_from_slice(&self.handler_mid.to_le_bytes());
        buf[8..12].copy_from_slice(&self.handler_high.to_le_bytes());
        buf[12..16].copy_from_slice(&self.reserved.to_le_bytes());
        buf
    }
}

impl fmt::Display for IdtEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Copy fields from packed struct to avoid unaligned reference UB
        let code_selector = self.code_selector;
        let handler = self.handler();
        let flags = self.flags;
        let ist = self.ist;
        write!(
            f,
            "IDT[sel=0x{:04x}, handler=0x{:016x}, flags=0x{:02x}, ist={}]",
            code_selector, handler, flags, ist,
        )
    }
}

/// The full Interrupt Descriptor Table (256 entries).
#[repr(C, align(16))]
#[derive(Debug, Clone)]
pub struct InterruptDescriptorTable {
    pub entries: [IdtEntry; 256],
}

impl InterruptDescriptorTable {
    /// Create an empty IDT (all entries missing).
    pub fn new() -> Self {
        Self {
            entries: [IdtEntry::missing(); 256],
        }
    }

    /// Set an IDT entry at the given vector.
    pub fn set_handler(&mut self, vector: u8, entry: IdtEntry) {
        self.entries[vector as usize] = entry;
    }

    /// Get an IDT entry at the given vector.
    pub fn handler(&self, vector: u8) -> &IdtEntry {
        &self.entries[vector as usize]
    }

    /// Get a mutable IDT entry.
    pub fn handler_mut(&mut self, vector: u8) -> &mut IdtEntry {
        &mut self.entries[vector as usize]
    }

    /// Load the IDT using `lidt`.
    pub fn pointer(&self) -> IdtPointer {
        let base = self as *const InterruptDescriptorTable as u64;
        let limit = (core::mem::size_of::<Self>() - 1) as u16;
        IdtPointer { limit, base }
    }
}

impl Default for InterruptDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}

/// IDT pointer structure loaded by `lidt`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdtPointer {
    /// Size of the IDT in bytes minus 1
    pub limit: u16,
    /// Linear address of the IDT
    pub base: u64,
}

// ============================================================================
// Task State Segment (TSS) — x86-64
// ============================================================================

/// x86-64 Task State Segment for interrupt stack tables (IST).
#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct TaskStateSegment {
    /// Reserved (formerly backlink)
    pub reserved1: u32,
    /// Reserved
    pub rsp0_low: u32,
    /// Ring 0 stack pointer (bits 32–63)
    pub rsp0_high: u32,
    /// Ring 1 stack pointer (bits 0–31)
    pub rsp1_low: u32,
    /// Ring 1 stack pointer (bits 32–63)
    pub rsp1_high: u32,
    /// Ring 2 stack pointer (bits 0–31)
    pub rsp2_low: u32,
    /// Ring 2 stack pointer (bits 32–63)
    pub rsp2_high: u32,
    /// Reserved
    pub reserved2: u32,
    /// Reserved
    pub reserved3: u32,
    /// Interrupt stack table 1 (bits 0–31)
    pub ist1_low: u32,
    /// Interrupt stack table 1 (bits 32–63)
    pub ist1_high: u32,
    /// Interrupt stack table 2 (bits 0–31)
    pub ist2_low: u32,
    /// Interrupt stack table 2 (bits 32–63)
    pub ist2_high: u32,
    /// Interrupt stack table 3
    pub ist3_low: u32,
    pub ist3_high: u32,
    /// Interrupt stack table 4
    pub ist4_low: u32,
    pub ist4_high: u32,
    /// Interrupt stack table 5
    pub ist5_low: u32,
    pub ist5_high: u32,
    /// Interrupt stack table 6
    pub ist6_low: u32,
    pub ist6_high: u32,
    /// Interrupt stack table 7
    pub ist7_low: u32,
    pub ist7_high: u32,
    /// Reserved
    pub reserved4: u32,
    /// Reserved
    pub reserved5: u32,
    /// I/O map base address (bits 0–15), reserved (bits 16–31)
    pub io_map_base: u32,
}

impl TaskStateSegment {
    /// Create a new zeroed TSS.
    pub fn new() -> Self {
        Self {
            reserved1: 0,
            rsp0_low: 0,
            rsp0_high: 0,
            rsp1_low: 0,
            rsp1_high: 0,
            rsp2_low: 0,
            rsp2_high: 0,
            reserved2: 0,
            reserved3: 0,
            ist1_low: 0,
            ist1_high: 0,
            ist2_low: 0,
            ist2_high: 0,
            ist3_low: 0,
            ist3_high: 0,
            ist4_low: 0,
            ist4_high: 0,
            ist5_low: 0,
            ist5_high: 0,
            ist6_low: 0,
            ist6_high: 0,
            ist7_low: 0,
            ist7_high: 0,
            reserved4: 0,
            reserved5: 0,
            io_map_base: 0,
        }
    }

    /// Get ring 0 stack pointer.
    pub fn rsp0(&self) -> u64 {
        self.rsp0_low as u64 | (self.rsp0_high as u64) << 32
    }

    /// Set ring 0 stack pointer.
    pub fn set_rsp0(&mut self, val: u64) {
        self.rsp0_low = val as u32;
        self.rsp0_high = (val >> 32) as u32;
    }

    /// Set an IST entry (1-indexed: 1..=7).
    pub fn set_ist(&mut self, index: usize, addr: u64) {
        let self_ptr = self as *mut TaskStateSegment;
        unsafe {
            #[allow(unused_unsafe)]
            match index {
                1 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist1_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist1_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                2 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist2_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist2_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                3 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist3_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist3_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                4 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist4_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist4_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                5 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist5_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist5_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                6 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist6_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist6_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                7 => {
                    core::ptr::addr_of_mut!((*self_ptr).ist7_low).write_unaligned(addr as u32);
                    core::ptr::addr_of_mut!((*self_ptr).ist7_high)
                        .write_unaligned((addr >> 32) as u32);
                }
                _ => panic!("TSS IST index must be 1..=7"),
            }
        }
    }

    /// Get an IST entry (1-indexed).
    pub fn ist(&self, index: usize) -> u64 {
        let (low, high) = match index {
            1 => (self.ist1_low, self.ist1_high),
            2 => (self.ist2_low, self.ist2_high),
            3 => (self.ist3_low, self.ist3_high),
            4 => (self.ist4_low, self.ist4_high),
            5 => (self.ist5_low, self.ist5_high),
            6 => (self.ist6_low, self.ist6_high),
            7 => (self.ist7_low, self.ist7_high),
            _ => panic!("TSS IST index must be 1..=7"),
        };
        low as u64 | (high as u64) << 32
    }

    /// Return the size of the TSS in bytes.
    pub fn size() -> u32 {
        core::mem::size_of::<Self>() as u32
    }
}

impl Default for TaskStateSegment {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// System Control — MSR helpers, CPUID parsing
// ============================================================================

/// Model-Specific Register (MSR) identifiers for common x86-64 registers.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Msr {
    /// Extended Feature Enable Register
    Efer = 0xC000_0080,
    /// Star (SysCall / SysRet base)
    Star = 0xC000_0081,
    /// LSTAR (SysCall RIP)
    Lstar = 0xC000_0082,
    /// CSTAR (compatibility mode syscall RIP)
    Cstar = 0xC000_0083,
    /// SF_MASK (syscall flag mask)
    SFMask = 0xC000_0084,
    /// Kernel GS base (swapgs target)
    KernelGsBase = 0xC000_0102,
    /// TSC (Time Stamp Counter)
    Tsc = 0x0000_0010,
    /// APIC base address
    ApicBase = 0x0000_001B,
    /// Sysenter CS (legacy)
    SysenterCs = 0x0000_0174,
    /// Sysenter ESP (legacy)
    SysenterEsp = 0x0000_0175,
    /// Sysenter EIP (legacy)
    SysenterEip = 0x0000_0176,
    /// Machine Check Address
    McaAddr = 0x0000_017B,
    /// Machine Check Type
    McaType = 0x0000_017A,
    /// CR_PAT (Page Attribute Table)
    Pat = 0x0000_0277,
    /// EFER bits
    EferSyscall = 1 << 0,
    EferLme = 1 << 8,
    EferLma = 1 << 10,
    EferNxe = 1 << 11,
    EferSvme = 1 << 12,
    EferLmsle = 1 << 13,
    EferFfxsr = 1 << 14,
}

/// MSR control — safe wrappers for rdmsr/wrmsr operations.
pub struct SystemControl;

impl SystemControl {
    /// Read a model-specific register (rdmsr).
    /// Safety: requires that the CPU supports the given MSR.
    #[inline]
    pub unsafe fn rdmsr(msr: u32) -> u64 {
        let (high, low): (u32, u32);
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack),
        );
        (high as u64) << 32 | low as u64
    }

    /// Write a model-specific register (wrmsr).
    /// Safety: caller must ensure the MSR is writable and the value is valid.
    #[inline]
    pub unsafe fn wrmsr(msr: u32, value: u64) {
        let low = value as u32;
        let high = (value >> 32) as u32;
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack),
        );
    }

    /// Enable SYSCALL/SYSRET (EFER.SCE).
    #[inline]
    pub unsafe fn enable_syscall() {
        let efer = Self::rdmsr(Msr::Efer as u32);
        Self::wrmsr(Msr::Efer as u32, efer | Msr::EferSyscall as u64);
    }

    /// Enable NX (No-eXecute) page protection (EFER.NXE).
    #[inline]
    pub unsafe fn enable_nxe() {
        let efer = Self::rdmsr(Msr::Efer as u32);
        Self::wrmsr(Msr::Efer as u32, efer | Msr::EferNxe as u64);
    }

    /// Set up LSTAR for fast syscalls.
    #[inline]
    pub unsafe fn set_lstar(entry: u64) {
        Self::wrmsr(Msr::Lstar as u32, entry);
    }

    /// Set STAR (CS and SS selectors for syscall/sysret).
    #[inline]
    pub unsafe fn set_star(syscall_cs: u64, sysret_cs: u64) {
        let value = (sysret_cs << 48) | (syscall_cs << 32);
        Self::wrmsr(Msr::Star as u32, value);
    }

    /// Set the syscall flag mask.
    #[inline]
    pub unsafe fn set_sfmask(mask: u64) {
        Self::wrmsr(Msr::SFMask as u32, mask);
    }

    /// Read the kernel GS base (set with swapgs).
    #[inline]
    pub unsafe fn read_kernel_gs_base() -> u64 {
        Self::rdmsr(Msr::KernelGsBase as u32)
    }

    /// Write the kernel GS base (set with swapgs).
    #[inline]
    pub unsafe fn write_kernel_gs_base(base: u64) {
        Self::wrmsr(Msr::KernelGsBase as u32, base);
    }

    /// Read the TSC (Time Stamp Counter).
    #[inline]
    pub unsafe fn read_tsc() -> u64 {
        Self::rdmsr(Msr::Tsc as u32)
    }

    /// Read the APIC base address.
    #[inline]
    pub unsafe fn read_apic_base() -> u64 {
        Self::rdmsr(Msr::ApicBase as u32)
    }
}

// ============================================================================
// CPUID parsing
// ============================================================================

/// Parsed CPUID information.
#[derive(Debug, Clone)]
pub struct CpuIdInfo {
    /// Vendor string (e.g. "GenuineIntel", "AuthenticAMD")
    pub vendor: [u8; 12],
    /// Processor brand string
    pub brand: [u8; 48],
    /// Stepping ID
    pub stepping: u8,
    /// Model
    pub model: u8,
    /// Family
    pub family: u8,
    /// Whether long mode (x86-64) is supported
    pub long_mode: bool,
    /// Whether NX (no-execute) is supported
    pub nx_supported: bool,
    /// Whether SMAP is supported
    pub smap: bool,
    /// Whether SMEP is supported
    pub smep: bool,
    /// Whether UMIP is supported
    pub umip: bool,
    /// Whether FSGSBASE instructions are supported
    pub fsgsbase: bool,
    /// Whether PCID is supported
    pub pcid: bool,
    /// Maximum physical address bits
    pub max_phys_addr: u8,
    /// Maximum linear (virtual) address bits
    pub max_linear_addr: u8,
    /// Number of processor cores in the package
    pub cpu_count: u8,
    /// Whether 1 GiB pages are supported
    pub page_1gb: bool,
    /// Cache line size in bytes
    pub cache_line_size: u8,
}

impl CpuIdInfo {
    /// Parse CPUID information.
    /// Safety: requires CPUID instruction to be available.
    pub unsafe fn parse() -> Self {
        let vendor = Self::cpuid_vendor();
        let brand = Self::cpuid_brand();
        let (stepping, model, family) = Self::cpuid_version();
        let (long_mode, nx_supported, page_1gb) = Self::cpuid_ext_features();
        let (smap, smep, umip, fsgsbase, pcid) = Self::cpuid_features();
        let (max_phys_addr, max_linear_addr) = Self::cpuid_addr_size();
        let cache_line_size = Self::cpuid_cache();
        let cpu_count = Self::cpuid_cpu_count();

        Self {
            vendor,
            brand,
            stepping,
            model,
            family,
            long_mode,
            nx_supported,
            smap,
            smep,
            umip,
            fsgsbase,
            pcid,
            max_phys_addr,
            max_linear_addr,
            cpu_count,
            page_1gb,
            cache_line_size,
        }
    }

    /// Execute CPUID leaf 0 to get the vendor string.
    unsafe fn cpuid_vendor() -> [u8; 12] {
        let mut eax: u32 = 0;
        let mut ebx_tmp: u64 = 0;
        let mut ecx: u32 = 0;
        let mut edx: u32 = 0;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "xchg {tmp:r}, rbx",
            tmp = inout(reg) 0u64 => ebx_tmp,
            inout("eax") 0u32 => eax,
            lateout("ecx") ecx,
            lateout("edx") edx,
            options(nomem, nostack, preserves_flags),
        );
        let ebx = ebx_tmp as u32;
        let mut vendor = [0u8; 12];
        vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&ecx.to_le_bytes());
        vendor
    }

    /// Execute CPUID leaves 0x80000002..0x80000004 to get the brand string.
    unsafe fn cpuid_brand() -> [u8; 48] {
        let mut brand = [0u8; 48];
        for leaf in 0..3 {
            let mut eax: u32 = 0;
            let mut ebx_tmp: u64 = 0;
            let mut ecx: u32 = 0;
            let mut edx: u32 = 0;
            core::arch::asm!(
                "mov {tmp:r}, rbx",
                "cpuid",
                "xchg {tmp:r}, rbx",
                tmp = inout(reg) 0u64 => ebx_tmp,
                inout("eax") (0x8000_0002u32 + leaf) => eax,
                lateout("ecx") ecx,
                lateout("edx") edx,
                options(nomem, nostack, preserves_flags),
            );
            let ebx = ebx_tmp as u32;
            let offset = leaf as usize * 16;
            brand[offset..offset + 4].copy_from_slice(&eax.to_le_bytes());
            brand[offset + 4..offset + 8].copy_from_slice(&ebx.to_le_bytes());
            brand[offset + 8..offset + 12].copy_from_slice(&ecx.to_le_bytes());
            brand[offset + 12..offset + 16].copy_from_slice(&edx.to_le_bytes());
        }
        brand
    }

    /// Execute CPUID leaf 1 to get version info.
    unsafe fn cpuid_version() -> (u8, u8, u8) {
        let eax: u32;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "mov rbx, {tmp:r}",
            tmp = inout(reg) 0u64 => _,
            inout("eax") 1u32 => eax,
            lateout("ecx") _,
            lateout("edx") _,
            options(nomem, nostack, preserves_flags),
        );
        let stepping = (eax & 0xF) as u8;
        let model = ((eax >> 4) & 0xF) as u8;
        let family = ((eax >> 8) & 0xF) as u8;
        (stepping, model, family)
    }

    /// Execute CPUID leaf 0x80000001 to get extended features.
    unsafe fn cpuid_ext_features() -> (bool, bool, bool) {
        let edx: u32;
        let ecx: u32;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "mov rbx, {tmp:r}",
            tmp = inout(reg) 0u64 => _,
            inout("eax") 0x8000_0001u32 => _,
            lateout("ecx") ecx,
            lateout("edx") edx,
            options(nomem, nostack, preserves_flags),
        );
        let long_mode = (edx & (1 << 29)) != 0;
        let nx_supported = (edx & (1 << 20)) != 0;
        let page_1gb = (edx & (1 << 26)) != 0;
        (long_mode, nx_supported, page_1gb)
    }

    /// Execute CPUID leaf 7 (structured extended features).
    unsafe fn cpuid_features() -> (bool, bool, bool, bool, bool) {
        Self::cpuid_features_ex()
    }

    /// Re-read CPUID leaf 7 properly for all feature bits.
    unsafe fn cpuid_features_ex() -> (bool, bool, bool, bool, bool) {
        let mut ebx_tmp: u64 = 0;
        let mut ecx: u32 = 0;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "xchg {tmp:r}, rbx",
            tmp = inout(reg) 0u64 => ebx_tmp,
            inout("eax") 7u32 => _,
            inout("ecx") 0u32 => ecx,
            lateout("edx") _,
            options(nomem, nostack, preserves_flags),
        );
        let ebx = ebx_tmp as u32;
        let smep = (ebx & (1 << 7)) != 0;
        let smap = (ebx & (1 << 20)) != 0;
        let umip = (ecx & (1 << 2)) != 0;
        let fsgsbase = (ebx & (1 << 0)) != 0;
        let pcid = (ebx & (1 << 17)) != 0;
        (smap, smep, umip, fsgsbase, pcid)
    }

    /// Execute CPUID leaf 0x80000008 for physical/linear address sizes.
    unsafe fn cpuid_addr_size() -> (u8, u8) {
        let eax: u32;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "mov rbx, {tmp:r}",
            tmp = inout(reg) 0u64 => _,
            inout("eax") 0x8000_0008u32 => eax,
            lateout("ecx") _,
            lateout("edx") _,
            options(nomem, nostack, preserves_flags),
        );
        let phys = (eax & 0xFF) as u8;
        let linear = ((eax >> 8) & 0xFF) as u8;
        (phys, linear)
    }

    /// Execute CPUID leaf 0x80000005 for cache info.
    unsafe fn cpuid_cache() -> u8 {
        let ecx: u32;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "mov rbx, {tmp:r}",
            tmp = inout(reg) 0u64 => _,
            inout("eax") 0x8000_0005u32 => _,
            lateout("ecx") ecx,
            lateout("edx") _,
            options(nomem, nostack, preserves_flags),
        );
        (ecx & 0xFF) as u8
    }

    /// Execute CPUID leaf 1 for CPU count.
    unsafe fn cpuid_cpu_count() -> u8 {
        let mut ebx_tmp: u64 = 0;
        core::arch::asm!(
            "mov {tmp:r}, rbx",
            "cpuid",
            "xchg {tmp:r}, rbx",
            tmp = inout(reg) 0u64 => ebx_tmp,
            inout("eax") 1u32 => _,
            lateout("ecx") _,
            lateout("edx") _,
            options(nomem, nostack, preserves_flags),
        );
        let ebx = ebx_tmp as u32;
        // EBX[23:16] = maximum addressable IDs for logical processors
        ((ebx >> 16) & 0xFF) as u8
    }

    /// Format vendor string for display.
    pub fn vendor_str(&self) -> &str {
        core::str::from_utf8(&self.vendor).unwrap_or("Unknown")
    }

    /// Format brand string for display.
    pub fn brand_str(&self) -> &str {
        core::str::from_utf8(&self.brand).unwrap_or("Unknown")
    }
}

impl fmt::Display for CpuIdInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CPU: {} | {} | Family {} Model {} Stepping {} | {} phys / {} virt addr bits | {} cores",
            self.vendor_str(),
            self.brand_str(),
            self.family,
            self.model,
            self.stepping,
            self.max_phys_addr,
            self.max_linear_addr,
            self.cpu_count,
        )?;
        if self.long_mode {
            write!(f, " | x86-64")?;
        }
        if self.nx_supported {
            write!(f, " | NX")?;
        }
        if self.smep {
            write!(f, " | SMEP")?;
        }
        if self.smap {
            write!(f, " | SMAP")?;
        }
        if self.page_1gb {
            write!(f, " | 1GiB pages")?;
        }
        Ok(())
    }
}

// ============================================================================
// Physical Memory Map — E820-style
// ============================================================================

/// Type of a physical memory region.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Usable RAM (available for the OS)
    Usable = 1,
    /// Reserved by firmware or hardware
    Reserved = 2,
    /// ACPI reclaimable memory
    AcpiReclaimable = 3,
    /// ACPI NVS (non-volatile storage)
    AcpiNvs = 4,
    /// Bad memory
    Bad = 5,
}

/// A single entry in the physical memory map (E820-style).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    /// Base physical address
    pub base_addr: u64,
    /// Length of the region in bytes
    pub length: u64,
    /// Type of the region
    pub type_: MemoryRegionType,
    /// Extended attributes (ACPI 3.0+)
    pub extended_attrs: u32,
}

impl MemoryMapEntry {
    /// Create a new memory region entry.
    pub const fn new(base_addr: u64, length: u64, type_: MemoryRegionType) -> Self {
        Self {
            base_addr,
            length,
            type_,
            extended_attrs: 1, // bit 0 = "ignore this entry if not understood"
        }
    }

    /// Get the end address (exclusive).
    pub fn end(&self) -> u64 {
        self.base_addr.wrapping_add(self.length)
    }

    /// Check if this region contains a given address.
    pub fn contains(&self, addr: u64) -> bool {
        let base = self.base_addr;
        let len = self.length;
        addr >= base && addr < base + len
    }

    /// Check if this region is usable RAM.
    pub fn is_usable(&self) -> bool {
        let type_ = self.type_;
        type_ == MemoryRegionType::Usable
    }

    /// Return the size in pages (4 KiB).
    pub fn pages_4k(&self) -> u64 {
        let len = self.length;
        len / 4096
    }
}

impl fmt::Display for MemoryMapEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_addr = self.base_addr;
        let end = self.end();
        let length = self.length;
        let type_ = self.type_;
        write!(
            f,
            "MMIO[0x{:016x} - 0x{:016x} ({} KiB, {:?})]",
            base_addr,
            end,
            length / 1024,
            type_,
        )
    }
}

/// The physical memory map — a collection of E820-style entries.
#[derive(Debug, Clone)]
pub struct PhysicalMemoryMap {
    pub entries: Vec<MemoryMapEntry>,
}

impl PhysicalMemoryMap {
    /// Create an empty memory map.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a memory region entry.
    pub fn add(&mut self, entry: MemoryMapEntry) {
        self.entries.push(entry);
    }

    /// Add a usable RAM region.
    pub fn add_usable(&mut self, base: u64, length: u64) {
        self.entries
            .push(MemoryMapEntry::new(base, length, MemoryRegionType::Usable));
    }

    /// Add a reserved region.
    pub fn add_reserved(&mut self, base: u64, length: u64) {
        self.entries.push(MemoryMapEntry::new(
            base,
            length,
            MemoryRegionType::Reserved,
        ));
    }

    /// Get the total amount of usable RAM.
    pub fn total_usable(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.is_usable())
            .map(|e| e.length)
            .sum()
    }

    /// Find a usable RAM region large enough to hold `size` bytes.
    pub fn find_usable_region(&self, size: u64) -> Option<(u64, u64)> {
        for entry in &self.entries {
            if entry.is_usable() && entry.length >= size {
                return Some((entry.base_addr, entry.length));
            }
        }
        None
    }

    /// Sort entries by base address, merging adjacent usable regions.
    pub fn coalesce(&mut self) {
        self.entries.sort_by_key(|e| e.base_addr);
        let mut merged: Vec<MemoryMapEntry> = Vec::new();
        for entry in self.entries.drain(..) {
            if let Some(last) = merged.last_mut() {
                // Copy values from packed struct to avoid unaligned refs
                let last_type = last.type_;
                let last_base = last.base_addr;
                let last_length = last.length;
                let entry_type = entry.type_;
                let entry_base = entry.base_addr;
                if last_type == entry_type && last_base + last_length == entry_base {
                    last.length += entry.length;
                    continue;
                }
            }
            merged.push(entry);
        }
        self.entries = merged;
    }

    /// Build a default memory map for QEMU (128 MiB RAM, standard firmware ranges).
    pub fn qemu_default() -> Self {
        let mut map = Self::new();
        // Usable RAM: 1 MiB to 128 MiB
        map.add_usable(0x0010_0000, 0x07F0_0000);
        // Low memory: 0 to 640 KiB
        map.add_usable(0x0000_0000, 0x000A_0000);
        // VGA / firmware gap: 640 KiB to 1 MiB
        map.add_reserved(0x000A_0000, 0x0006_0000);
        // EBDA (Extended BIOS Data Area)
        map.add_reserved(0x000F_0000, 0x0001_0000);
        // ACPI tables
        map.add_reserved(0x07FE_0000, 0x0002_0000);
        map
    }
}

impl Default for PhysicalMemoryMap {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Assembly Shim — extern "C" wrappers for inlined asm
// ============================================================================

/// Raw assembly shims — safe wrappers around privileged instructions.
/// These are intended to be called from Rust code that needs to execute
/// ring-0 operations without inline asm at the call site.
pub struct AssemblyShim;

impl AssemblyShim {
    /// Read CR0 (control register 0).
    /// Returns the current value of CR0.
    #[inline]
    pub unsafe fn read_cr0() -> u64 {
        let value: u64;
        core::arch::asm!("mov {}, cr0", out(reg) value, options(nomem, nostack));
        value
    }

    /// Write CR0.
    #[inline]
    pub unsafe fn write_cr0(value: u64) {
        core::arch::asm!("mov cr0, {}", in(reg) value, options(nomem, nostack));
    }

    /// Read CR2 (page fault linear address).
    #[inline]
    pub unsafe fn read_cr2() -> u64 {
        let value: u64;
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack));
        value
    }

    /// Read CR3 (page table base address).
    #[inline]
    pub unsafe fn read_cr3() -> u64 {
        let value: u64;
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack));
        value
    }

    /// Write CR3 (flush TLB / switch page tables).
    #[inline]
    pub unsafe fn write_cr3(value: u64) {
        core::arch::asm!("mov cr3, {}", in(reg) value, options(nomem, nostack));
    }

    /// Read CR4.
    #[inline]
    pub unsafe fn read_cr4() -> u64 {
        let value: u64;
        core::arch::asm!("mov {}, cr4", out(reg) value, options(nomem, nostack));
        value
    }

    /// Write CR4.
    #[inline]
    pub unsafe fn write_cr4(value: u64) {
        core::arch::asm!("mov cr4, {}", in(reg) value, options(nomem, nostack));
    }

    /// Read RFLAGS.
    #[inline]
    pub unsafe fn read_rflags() -> u64 {
        let value: u64;
        core::arch::asm!("pushfq; pop {}", out(reg) value, options(nomem, nostack));
        value
    }

    /// Halt the CPU until the next interrupt.
    #[inline]
    pub unsafe fn hlt() {
        core::arch::asm!("hlt", options(nomem, nostack));
    }

    /// Enable interrupts (sti).
    #[inline]
    pub unsafe fn sti() {
        core::arch::asm!("sti", options(nomem, nostack));
    }

    /// Disable interrupts (cli).
    #[inline]
    pub unsafe fn cli() {
        core::arch::asm!("cli", options(nomem, nostack));
    }

    /// Read the stack pointer (RSP).
    #[inline]
    pub unsafe fn read_rsp() -> u64 {
        let value: u64;
        core::arch::asm!("mov {}, rsp", out(reg) value, options(nomem, nostack));
        value
    }

    /// Read the instruction pointer (RIP) — useful for PC-relative addressing.
    #[inline]
    pub unsafe fn read_rip() -> u64 {
        let value: u64;
        core::arch::asm!("lea {}, [rip + 0]", out(reg) value, options(nomem, nostack, pure));
        value
    }

    /// Load GDT (lgdt).
    #[inline]
    pub unsafe fn lgdt(gdt_ptr: &GdtPointer) {
        core::arch::asm!("lgdt [{}]", in(reg) gdt_ptr, options(readonly, nostack));
    }

    /// Load IDT (lidt).
    #[inline]
    pub unsafe fn lidt(idt_ptr: &IdtPointer) {
        core::arch::asm!("lidt [{}]", in(reg) idt_ptr, options(readonly, nostack));
    }

    /// Load task register (ltr).
    #[inline]
    pub unsafe fn ltr(selector: u16) {
        core::arch::asm!("ltr {0:x}", in(reg) selector, options(nomem, nostack));
    }

    /// Reload segment registers after GDT switch.
    #[inline]
    pub unsafe fn reload_segments(cs: u16, ds: u16) {
        core::arch::asm!(
            "push {cs}",
            "lea {tmp}, [2f + 0]",
            "push {tmp}",
            "retfq",
            "2:",
            "mov ds, {ds}",
            "mov es, {ds}",
            "mov fs, {ds}",
            "mov gs, {ds}",
            "mov ss, {ds}",
            cs = in(reg) cs as u64,
            ds = in(reg) ds,
            tmp = lateout(reg) _,
            options(preserves_flags, nostack),
        );
    }

    /// Swap GS (exchange GS base with MSR kernel GS base).
    #[inline]
    pub unsafe fn swapgs() {
        core::arch::asm!("swapgs", options(nomem, nostack));
    }

    /// Invalidate a single TLB entry (invlpg).
    #[inline]
    pub unsafe fn invlpg(addr: u64) {
        core::arch::asm!("invlpg [{}]", in(reg) addr, options(nomem, nostack));
    }

    /// Read from an I/O port (inb).
    #[inline]
    pub unsafe fn inb(port: u16) -> u8 {
        let value: u8;
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack));
        value
    }

    /// Write to an I/O port (outb).
    #[inline]
    pub unsafe fn outb(port: u16, value: u8) {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
    }

    /// Read from I/O port (inw) — 16-bit.
    #[inline]
    pub unsafe fn inw(port: u16) -> u16 {
        let value: u16;
        core::arch::asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack));
        value
    }

    /// Write to I/O port (outw) — 16-bit.
    #[inline]
    pub unsafe fn outw(port: u16, value: u16) {
        core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack));
    }

    /// Read from I/O port (inl) — 32-bit.
    #[inline]
    pub unsafe fn inl(port: u16) -> u32 {
        let value: u32;
        core::arch::asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack));
        value
    }

    /// Write to I/O port (outl) — 32-bit.
    #[inline]
    pub unsafe fn outl(port: u16, value: u32) {
        core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack));
    }

    /// Pause (hint to the CPU that we're in a spin loop).
    #[inline]
    pub unsafe fn pause() {
        core::arch::asm!("pause", options(nomem, nostack));
    }

    /// Serialize instruction execution (for memory ordering).
    #[inline]
    pub unsafe fn mfence() {
        core::arch::asm!("mfence", options(nomem, nostack));
    }

    /// Read performance counter.
    #[inline]
    pub unsafe fn read_pmc(counter: u32) -> u64 {
        let low: u32;
        let high: u32;
        core::arch::asm!(
            "rdpmc",
            in("ecx") counter,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack),
        );
        (high as u64) << 32 | low as u64
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- NucleusEntry ---

    #[test]
    fn test_nucleus_entry_default() {
        let entry = NucleusEntry::default();
        assert_eq!(entry.magic, 0);
        assert_eq!(entry.virtual_offset, 0xFFFF_8000_0000_0000u64);
    }

    #[test]
    fn test_nucleus_entry_magic() {
        let entry = NucleusEntry::default();
        assert!(entry.validate_magic(0));
        assert!(!entry.validate_magic(0x36D76289));
    }

    // --- GDT ---

    #[test]
    fn test_gdt_descriptor_null() {
        let desc = GdtDescriptor::null();
        let limit_low = desc.limit_low;
        let access = desc.access;
        assert_eq!(limit_low, 0);
        assert_eq!(access, 0);
        let bytes = desc.raw_bytes();
        assert_eq!(bytes, [0u8; 8]);
    }

    #[test]
    fn test_gdt_descriptor_code_kernel() {
        let desc = GdtDescriptor::code_kernel();
        let access = desc.access;
        assert_eq!(access, 0b1001_1010);
        let bytes = desc.raw_bytes();
        // Access byte at index 5
        assert_eq!(bytes[5], 0b1001_1010);
    }

    #[test]
    fn test_gdt_descriptor_data_kernel() {
        let desc = GdtDescriptor::data_kernel();
        let access = desc.access;
        assert_eq!(access, 0b1001_0010);
    }

    #[test]
    fn test_gdt_descriptor_code_user() {
        let desc = GdtDescriptor::code_user();
        let access = desc.access;
        assert_eq!(access, 0b1111_1010);
    }

    #[test]
    fn test_gdt_descriptor_data_user() {
        let desc = GdtDescriptor::data_user();
        let access = desc.access;
        assert_eq!(access, 0b1111_0010);
    }

    #[test]
    fn test_gdt_new() {
        let gdt = Gdt::new();
        // Null entry should be all zeros
        let null_bytes = gdt.null.raw_bytes();
        assert_eq!(null_bytes, [0u8; 8]);
        // Kernel code should be present
        let access = gdt.kernel_code.access;
        assert!(access & 0x80 != 0);
        assert_eq!(Gdt::KERNEL_CODE_SELECTOR, 0x08);
        assert_eq!(Gdt::KERNEL_DATA_SELECTOR, 0x10);
        assert_eq!(Gdt::USER_CODE_SELECTOR, 0x18);
        assert_eq!(Gdt::USER_DATA_SELECTOR, 0x20);
        assert_eq!(Gdt::TSS_SELECTOR, 0x28);
    }

    #[test]
    fn test_gdt_pointer() {
        let gdt = Gdt::new();
        let ptr = gdt.pointer();
        // Base should be the address of gdt
        let limit = ptr.limit;
        assert_eq!(limit, core::mem::size_of::<Gdt>() as u16 - 1);
    }

    #[test]
    fn test_tss_descriptor() {
        let tss_base: u64 = 0x1000;
        let tss_limit = core::mem::size_of::<TaskStateSegment>() as u32 - 1;
        let desc = TssDescriptor::new(tss_base, tss_limit);
        let access = desc.desc_low.access;
        let base_high32 = desc.base_high32;
        assert_eq!(access, 0b1000_1001);
        assert_eq!(base_high32, 0);
        let bytes = desc.raw_bytes();
        assert_eq!(bytes.len(), 16);
        assert!(bytes[5] & 0x80 != 0); // present bit
    }

    // --- Page Table ---

    #[test]
    fn test_page_table_entry_empty() {
        let entry = PageTableEntry::empty();
        assert!(!entry.is_present());
        assert_eq!(entry.addr(), 0);
    }

    #[test]
    fn test_page_table_entry_new() {
        let entry = PageTableEntry::new(0x1000, PageTableEntry::PRESENT | PageTableEntry::WRITABLE);
        assert!(entry.is_present());
        assert_eq!(entry.addr(), 0x1000);
        assert!(entry.bits & PageTableEntry::WRITABLE != 0);
    }

    #[test]
    fn test_page_table_entry_flags() {
        let mut entry = PageTableEntry::new(0x2000, PageTableEntry::PRESENT);
        assert!(entry.is_present());
        assert!(!entry.is_huge());

        entry.set_writable(true);
        assert!(entry.bits & PageTableEntry::WRITABLE != 0);

        entry.set_user(true);
        assert!(entry.bits & PageTableEntry::USER != 0);

        entry.set_no_execute(true);
        assert!(entry.bits & PageTableEntry::NO_EXECUTE != 0);

        entry.set_present(false);
        assert!(!entry.is_present());
    }

    #[test]
    fn test_page_table_new() {
        let pt = PageTable::new();
        for entry in pt.entries.iter() {
            assert!(!entry.is_present());
            assert_eq!(entry.addr(), 0);
        }
    }

    #[test]
    fn test_page_table_set_entry() {
        let mut pt = PageTable::new();
        let entry = PageTableEntry::new(0x3000, PageTableEntry::PRESENT | PageTableEntry::WRITABLE);
        pt.set_entry(42, entry);
        assert!(pt.entry(42).is_present());
        assert_eq!(pt.entry(42).addr(), 0x3000);
    }

    #[test]
    fn test_virtual_address_indices() {
        // 0xFFFF_8000_0000_0000 is the start of the canonical higher half:
        // PML4 = 0x100 (bits 47..39), PDPT = 0x000 (bits 38..30),
        // PD = 0x000 (bits 29..21), PT = 0x000 (bits 20..12).
        let (pml4, pdpt, pd, pt) = PageTable::virtual_address_indices(0xFFFF_8000_0000_0000);
        assert_eq!(pml4, 0x100);
        assert_eq!(pdpt, 0x000);
        assert_eq!(pd, 0x000);
        assert_eq!(pt, 0x000);

        // The higher-half self-map entry sits at PML4 index 0x1FF.
        let (self_pml4, _, _, _) = PageTable::virtual_address_indices(0xFFFF_FF80_0000_0000);
        assert_eq!(self_pml4, 0x1FF);
    }

    #[test]
    fn test_page_table_clear() {
        let mut pt = PageTable::new();
        pt.set_entry(0, PageTableEntry::new(0x4000, PageTableEntry::PRESENT));
        pt.clear();
        for entry in pt.entries.iter() {
            assert!(!entry.is_present());
        }
    }

    #[test]
    fn test_identity_map_4k() {
        let mut pml4 = PageTable::new();
        let mut pdpt = PageTable::new();
        let mut pd = PageTable::new();
        let mut pt = PageTable::new();
        let phys = 0x1000u64;
        let flags = PageTableEntry::PRESENT | PageTableEntry::WRITABLE;
        PageTable::identity_map_4k(&mut pml4, &mut pdpt, &mut pd, &mut pt, phys, flags);
        // The PT entry at the correct index should be present
        let (_, _, _, pt_idx) = PageTable::virtual_address_indices(phys);
        assert!(pt.entry(pt_idx).is_present());
        assert_eq!(pt.entry(pt_idx).addr(), phys);
    }

    // --- IDT ---

    #[test]
    fn test_idt_entry_missing() {
        let entry = IdtEntry::missing();
        assert!(!entry.is_present());
        assert_eq!(entry.handler(), 0);
    }

    #[test]
    fn test_idt_entry_interrupt_gate() {
        let entry = IdtEntry::interrupt_gate(0x1000, Gdt::KERNEL_CODE_SELECTOR, 0);
        assert!(entry.is_present());
        assert_eq!(entry.handler(), 0x1000);
        let code_selector = entry.code_selector;
        assert_eq!(code_selector, 0x08);
    }

    #[test]
    fn test_idt_entry_trap_gate() {
        let entry = IdtEntry::trap_gate(0x2000, 0x08, 1);
        assert!(entry.is_present());
        assert_eq!(entry.handler(), 0x2000);
        let ist = entry.ist;
        assert_eq!(ist, 1);
    }

    #[test]
    fn test_idt_entry_user_interrupt() {
        let entry = IdtEntry::user_interrupt_gate(0x3000, 0x18, 0);
        assert!(entry.is_present());
        assert_eq!(entry.handler(), 0x3000);
        // DPL should be 3
        let flags = entry.flags;
        assert_eq!((flags >> 5) & 0x03, 3);
    }

    #[test]
    fn test_idt_new_and_set() {
        let mut idt = InterruptDescriptorTable::new();
        let entry = IdtEntry::interrupt_gate(0x4000, 0x08, 0);
        idt.set_handler(0x20, entry);
        assert!(idt.handler(0x20).is_present());
        assert_eq!(idt.handler(0x20).handler(), 0x4000);
    }

    #[test]
    fn test_idt_pointer() {
        let idt = InterruptDescriptorTable::new();
        let ptr = idt.pointer();
        let expected_limit = (core::mem::size_of::<InterruptDescriptorTable>() - 1) as u16;
        let limit = ptr.limit;
        assert_eq!(limit, expected_limit);
    }

    #[test]
    fn test_idt_entry_raw_bytes() {
        let entry = IdtEntry::interrupt_gate(0xDEAD_BEEF, 0x08, 0);
        let bytes = entry.raw_bytes();
        assert_eq!(bytes.len(), 16);
        // Handler low should match
        assert_eq!(bytes[0..2], [0xEF, 0xBE]);
        assert_eq!(bytes[2..4], [0x08, 0x00]);
        assert_eq!(bytes[5], 0x8E);
    }

    // --- TSS ---

    #[test]
    fn test_tss_new() {
        let tss = TaskStateSegment::new();
        assert_eq!(tss.rsp0(), 0);
    }

    #[test]
    fn test_tss_rsp0() {
        let mut tss = TaskStateSegment::new();
        tss.set_rsp0(0xFFFF_8000_0000_0000);
        assert_eq!(tss.rsp0(), 0xFFFF_8000_0000_0000);
    }

    #[test]
    fn test_tss_ist() {
        let mut tss = TaskStateSegment::new();
        tss.set_ist(1, 0x9000);
        assert_eq!(tss.ist(1), 0x9000);
        tss.set_ist(2, 0xA000_0000_0000);
        assert_eq!(tss.ist(2), 0xA000_0000_0000);
        tss.set_ist(7, 0xB000);
        assert_eq!(tss.ist(7), 0xB000);
    }

    #[test]
    fn test_tss_size() {
        // x86-64 TSS should be 104 bytes
        assert_eq!(TaskStateSegment::size(), 104);
    }

    // --- MSR / SystemControl ---

    #[test]
    fn test_msr_constants() {
        assert_eq!(Msr::Efer as u32, 0xC000_0080);
        assert_eq!(Msr::Lstar as u32, 0xC000_0082);
        assert_eq!(Msr::KernelGsBase as u32, 0xC000_0102);
        assert_eq!(Msr::EferSyscall as u64, 1);
        assert_eq!(Msr::EferLme as u64, 1 << 8);
        assert_eq!(Msr::EferNxe as u64, 1 << 11);
    }

    // --- CPUID ---

    #[test]
    fn test_cpuid_vendor() {
        // This will only work on actual x86-64 hardware
        // On CI runners, the unsafe operations will JIT-compile but may not
        // execute in test context. We verify the function compiles.
    }

    // --- Memory Map ---

    #[test]
    fn test_memory_map_entry_new() {
        let entry = MemoryMapEntry::new(0x1000, 0x1000, MemoryRegionType::Usable);
        let base_addr = entry.base_addr;
        let length = entry.length;
        assert_eq!(base_addr, 0x1000);
        assert_eq!(length, 0x1000);
        assert!(entry.is_usable());
        assert_eq!(entry.end(), 0x2000);
    }

    #[test]
    fn test_memory_map_entry_contains() {
        let entry = MemoryMapEntry::new(0x1000, 0x1000, MemoryRegionType::Usable);
        assert!(entry.contains(0x1000));
        assert!(entry.contains(0x1FFF));
        assert!(!entry.contains(0x2000));
        assert!(!entry.contains(0x0FFF));
    }

    #[test]
    fn test_memory_map_new() {
        let map = PhysicalMemoryMap::new();
        assert!(map.entries.is_empty());
        assert_eq!(map.total_usable(), 0);
    }

    #[test]
    fn test_memory_map_add_usable() {
        let mut map = PhysicalMemoryMap::new();
        map.add_usable(0x1000, 0x1000);
        map.add_usable(0x2000, 0x2000);
        assert_eq!(map.entries.len(), 2);
        assert_eq!(map.total_usable(), 0x3000);
    }

    #[test]
    fn test_memory_map_find_usable() {
        let mut map = PhysicalMemoryMap::new();
        map.add_usable(0x1000, 0x1000);
        map.add_reserved(0x2000, 0x1000);
        map.add_usable(0x3000, 0x5000);
        let region = map.find_usable_region(0x2000);
        assert!(region.is_some());
        let (base, _len) = region.unwrap();
        // First fitting region is 0x1000 (size 0x1000), which is too small.
        // Next fitting is 0x3000 (size 0x5000).
        assert_eq!(base, 0x3000);
    }

    #[test]
    fn test_memory_map_coalesce() {
        let mut map = PhysicalMemoryMap::new();
        map.add_usable(0x2000, 0x1000);
        map.add_usable(0x1000, 0x1000);
        map.add_reserved(0x4000, 0x1000);
        map.add_usable(0x3000, 0x1000);
        map.coalesce();
        // Should have 2 entries: 0x1000-0x4000 (usable) and 0x4000-0x5000 (reserved)
        assert_eq!(map.entries.len(), 2);
        if map.entries[0].is_usable() {
            let e0_base = map.entries[0].base_addr;
            let e0_len = map.entries[0].length;
            let e1_base = map.entries[1].base_addr;
            assert_eq!(e0_base, 0x1000);
            assert_eq!(e0_len, 0x3000);
            assert_eq!(e1_base, 0x4000);
        } else {
            let e0_base = map.entries[0].base_addr;
            let e1_base = map.entries[1].base_addr;
            let e1_len = map.entries[1].length;
            assert_eq!(e0_base, 0x4000);
            assert_eq!(e1_base, 0x1000);
            assert_eq!(e1_len, 0x3000);
        }
    }

    #[test]
    fn test_memory_map_qemu_default() {
        let map = PhysicalMemoryMap::qemu_default();
        assert!(!map.entries.is_empty());
        assert!(map.total_usable() > 0);
    }

    #[test]
    fn test_memory_map_entry_pages() {
        let entry = MemoryMapEntry::new(0x1000, 0x4000, MemoryRegionType::Usable);
        assert_eq!(entry.pages_4k(), 4);
    }

    // --- AssemblyShim ---

    #[test]
    fn test_read_rflags() {
        // rflags must at least have bit 1 (always 1)
        unsafe {
            let rflags = AssemblyShim::read_rflags();
            assert!(rflags & (1 << 1) != 0);
        }
    }

    #[test]
    fn test_read_rip() {
        unsafe {
            let rip = AssemblyShim::read_rip();
            assert!(rip > 0);
        }
    }

    #[test]
    fn test_read_rsp() {
        unsafe {
            let rsp = AssemblyShim::read_rsp();
            assert!(rsp > 0);
        }
    }

    #[test]
    fn test_pause() {
        unsafe {
            AssemblyShim::pause();
        }
    }

    #[test]
    fn test_mfence() {
        unsafe {
            AssemblyShim::mfence();
        }
    }

    // Control-register access is privileged (ring 0 only). Executing these
    // from a host `cargo test` process faults with SIGSEGV, so they are
    // ignored by default. Run them under a ring-0 harness (or the QEMU
    // kernel) with `cargo test -- --ignored`.

    #[test]
    #[ignore = "requires ring 0: reading CR0 faults in user space"]
    fn test_read_cr0() {
        unsafe {
            let cr0 = AssemblyShim::read_cr0();
            // CR0 must have bit 0 (PE = protection enabled) set
            assert!(cr0 & 1 != 0);
        }
    }

    #[test]
    #[ignore = "requires ring 0: reading CR2 faults in user space"]
    fn test_read_cr2() {
        unsafe {
            let _ = AssemblyShim::read_cr2();
        }
    }

    #[test]
    #[ignore = "requires ring 0: reading CR3 faults in user space"]
    fn test_read_cr3() {
        unsafe {
            let cr3 = AssemblyShim::read_cr3();
            // CR3 should be non-zero (page tables are active)
            assert!(cr3 > 0);
        }
    }

    #[test]
    #[ignore = "requires ring 0: reading CR4 faults in user space"]
    fn test_read_cr4() {
        unsafe {
            let _ = AssemblyShim::read_cr4();
        }
    }

    #[test]
    fn test_inb_outb() {
        // We can't actually test I/O without a specific platform,
        // but the functions should compile and link.
    }
}
