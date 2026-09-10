// ELF64 object file emission for Phorc
// Uses the `object` crate to write relocatable ELF64 object files
// with proper symbol offsets, name mangling, and relocations.

use crate::ir::{PhirType, Constant};
use crate::receipts::RelocationEntry;
use std::collections::HashMap;

/// Write an ELF64 object file.
/// If `entry_addr` is Some(addr), uses ELFCLASS32 for Multiboot v1 compatibility
/// and adds a PT_NOTE section with PVH note.
pub fn write_elf64_object(
    source_name: &str,
    func_sections: &[(String, Vec<u8>, Vec<crate::ir::ByteAttribution>, u64)],
    globals: &[(String, PhirType, Option<Constant>)],
    relocations: &mut Vec<RelocationEntry>,
    entry_addr: Option<u64>,
) -> Vec<u8> {
    use object::write::*;
    use object::*;

    // Multiboot v1 requires EM_386 even if the kernel contains 64-bit code.
    // The startup code transitions to 64-bit before executing 64-bit instructions.
    let arch = if entry_addr.is_some() { Architecture::I386 } else { Architecture::X86_64 };
    let mut obj = write::Object::new(BinaryFormat::Elf, arch, Endianness::Little);

    obj.add_file_symbol(source_name.as_bytes().to_vec());
    obj.set_mangling(Mangling::None);

    // Add .text section
    let text_section_id = obj.add_section(
        segment_text().into(), ".text".into(), SectionKind::Text,
    );
    obj.section_symbol(text_section_id);

    // Add .data section
    let _data_section_id = obj.add_section(
        segment_data().into(), ".data".into(), SectionKind::Data,
    );

    // Add .bss section
    let bss_section_id = obj.add_section(
        segment_data().into(), ".bss".into(), SectionKind::UninitializedData,
    );

    // Add .note section with PVH note if entry_addr is specified (for QEMU -kernel boot)
    if let Some(entry_addr_val) = entry_addr {
        let note_section_id = obj.add_section(
            b"PT_NOTE".to_vec(), ".note".into(), SectionKind::Note,
        );
        // Build PVH note: Elf64_Nhdr + "PVH\0" + entry_addr(uint64_le)
        // Elf64_Nhdr: n_namesz(4) + n_descsz(4) + n_type(4) = 12 bytes
        let mut note_data = Vec::new();
        note_data.extend_from_slice(&4u32.to_le_bytes());  // n_namesz = 4
        note_data.extend_from_slice(&8u32.to_le_bytes());  // n_descsz = 8
        note_data.extend_from_slice(&0x6474u32.to_le_bytes()); // n_type = NT_PVH_ENTRY
        note_data.extend_from_slice(b"PVH\0");            // name
        note_data.extend_from_slice(&entry_addr_val.to_le_bytes()); // descriptor: entry address
        obj.section_symbol(note_section_id);
        obj.append_section_data(note_section_id, &note_data, 8);
    }

    // First pass: collect all mangled names and their symbol IDs
    let mut mangled_to_symid: HashMap<String, SymbolId> = HashMap::new();

    // Add function symbols and code to .text (single pass)
    // Names are already mangled with module prefix by codegen/mod.rs
    for (name, code, _attribs, _size) in func_sections {
        let offset = obj.append_section_data(text_section_id, code, 16);
        let sym_id = obj.add_symbol(write::Symbol {
            name: name.as_bytes().to_vec(),
            value: offset,
            size: code.len() as u64,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: write::SymbolSection::Section(text_section_id),
            flags: SymbolFlags::None,
        });
        mangled_to_symid.insert(name.clone(), sym_id);
    }

    // Add global data symbols
    for (name, _ty, _init) in globals {
        let offset = obj.append_section_data(bss_section_id, &[0u8; 8], 8);
        let sym_id = obj.add_symbol(write::Symbol {
            name: name.as_bytes().to_vec(),
            value: offset,
            size: 8,
            kind: SymbolKind::Data,
            scope: SymbolScope::Linkage,
            weak: false,
            section: write::SymbolSection::Section(bss_section_id),
            flags: SymbolFlags::None,
        });
        mangled_to_symid.insert(name.clone(), sym_id);
    }

    // Add relocations — look up target symbols by mangled name
    // x86-64 ELF relocation constants:
    const R_X86_64_PC32: u32 = 2;  // S + A - P (relative)
    const R_X86_64_64: u32 = 1;    // S + A (absolute)

    for reloc in relocations.iter() {
        let target_name = &reloc.target;
        let target_sym = mangled_to_symid.get(target_name).copied().unwrap_or_else(|| {
            // Create undefined symbol for external references
            obj.add_symbol(write::Symbol {
                name: target_name.as_bytes().to_vec(),
                value: 0, size: 0,
                kind: SymbolKind::Text,
                scope: SymbolScope::Dynamic,
                weak: false,
                section: write::SymbolSection::Undefined,
                flags: SymbolFlags::None,
            })
        });
        let r_type = match reloc.kind {
            crate::receipts::RelocationKind::Relative32 => R_X86_64_PC32,
            crate::receipts::RelocationKind::Absolute64 => R_X86_64_64,
            _ => R_X86_64_PC32,
        };
        obj.add_relocation(
            text_section_id,
            write::Relocation {
                offset: reloc.offset,
                symbol: target_sym,
                addend: reloc.addend,
                flags: write::RelocationFlags::Elf { r_type },
            },
        ).unwrap();
    }

    // Write the object file to bytes
    let bytes = obj.write().unwrap();
    bytes
}

fn segment_text() -> &'static [u8] {
    b".text"
}

fn segment_data() -> &'static [u8] {
    b".data"
}
