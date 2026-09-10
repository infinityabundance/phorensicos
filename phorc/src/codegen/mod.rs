// Phorc Codegen — x86-64 backend
// Translates PHIR → x86-64 instructions → ELF64 object file

pub mod object;
pub mod x86_64;

use crate::ir::{AbiFunction, AbiLinkage, AbiManifest, ByteAttribution, PhirModule};
use crate::receipts::{
    hash_bytes, CompileOutput, CompileResidual, FunctionReceipt, RelocationEntry,
};
use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Known intrinsic type prefixes
// ============================================================================

/// Return true if the callee name looks like an intrinsic method that
/// should get a linker stub rather than remaining an undefined symbol.
fn is_known_intrinsic(callee: &str) -> bool {
    let type_prefixes = [
        "Result_",
        "Option_",
        "Array_",
        "Str_",
        "Slice_",
        "RingBuf_",
        "Word64_",
        "FixedString_",
        "Math_",
    ];
    if type_prefixes.iter().any(|p| callee.starts_with(p)) {
        return true;
    }
    if callee.starts_with("method_") {
        return true;
    }
    matches!(
        callee,
        "Result_Ok"
            | "Result_Err"
            | "Option_Some"
            | "Option_None"
            | "math_sin"
            | "math_cos"
            | "math_sqrt"
    )
}

/// Generate a minimal x86-64 stub function that returns its first argument
/// (System V: RDI → RAX). For zero-argument methods, returns 0.
fn generate_stub(name: &str) -> (Vec<u8>, Vec<ByteAttribution>, u64) {
    use iced_x86::*;

    let mut ins: Vec<Instruction> = Vec::new();

    // Prologue: push rbp; mov rbp, rsp
    ins.push(Instruction::with1(Code::Push_r64, Register::RBP).unwrap());
    ins.push(Instruction::with2(Code::Mov_rm64_r64, Register::RBP, Register::RSP).unwrap());

    // Body: mov rax, rdi (return first arg regardless)
    ins.push(Instruction::with2(Code::Mov_rm64_r64, Register::RAX, Register::RDI).unwrap());

    // Epilogue: mov rsp, rbp; pop rbp; ret
    ins.push(Instruction::with2(Code::Mov_rm64_r64, Register::RSP, Register::RBP).unwrap());
    ins.push(Instruction::with1(Code::Pop_rm64, Register::RBP).unwrap());
    ins.push(Instruction::with(Code::Retnq));

    // Encode
    let block = InstructionBlock::new(&ins, 0);
    let code = match BlockEncoder::encode(64, block, BlockEncoderOptions::NONE) {
        Ok(r) => r.code_buffer,
        Err(_) => vec![0x90u8], // NOP sled fallback
    };
    let size = code.len() as u64;

    let attribs = vec![ByteAttribution {
        section: ".text".to_string(),
        offset: 0,
        len: size,
        source_span: None,
        phir_node: Some(format!("intrinsic_stub:{}", name)),
        lowering_rule: Some("intrinsic_stub".to_string()),
        abi_rule: None,
        instruction: None,
        relocation: None,
        receipt_hash: None,
    }];

    (code, attribs, size)
}

/// Extract a module prefix from the source file path for symbol mangling
fn module_prefix(source_name: &str) -> String {
    let path = Path::new(source_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    // Sanitize: replace non-alphanumeric chars
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Mangle a function name.
/// The PHIR name already carries owner-type qualification (e.g. `BootFlags_empty`)
/// from impl-block lowering, so no additional module prefix is needed.
fn mangle_name(_module: &str, func: &str) -> String {
    format!(
        "_phor_{}",
        func.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
    )
}

/// Compile a PHIR module to an object file
/// If `entry_point` is Some(name), a `_start` function is emitted that
/// calls `_phor_{name}` (for freestanding kernel images).
pub fn compile(module: &PhirModule, entry_point: Option<&str>) -> CompileOutput {
    let mut output = CompileOutput {
        object_bytes: Vec::new(),
        receipts: Vec::new(),
        byte_map: Vec::new(),
        relocations: Vec::new(),
        abi_manifest: AbiManifest {
            functions: Vec::new(),
            globals: Vec::new(),
        },
        residuals: Vec::new(),
    };

    // Derive module prefix for owner-aware symbol mangling
    let mod_prefix = module_prefix(&module.source_name);

    // Emit a concise PHIR dump when PHORC_DEBUG_IR is set (diagnostics).
    if std::env::var("PHORC_DEBUG_IR").is_ok() {
        for f in &module.functions {
            eprintln!(
                "fn {} params={} locals={}",
                f.name,
                f.params.len(),
                f.local_count
            );
            for (bi, b) in f.blocks.iter().enumerate() {
                eprintln!("  block {}:", bi);
                for op in &b.ops {
                    eprintln!("    {:?}", op);
                }
            }
        }
    }

    // Phase 1: Instruction selection & register allocation per function
    // func_sections stores: (mangled_name, code, attribs, size)
    let mut func_sections: Vec<(String, Vec<u8>, Vec<ByteAttribution>, u64)> = Vec::new();
    let mut relocs: Vec<RelocationEntry> = Vec::new();
    let mut text_offset: u64 = 0;
    const ALIGN: u64 = 16;
    let align_up = |x: u64, a: u64| -> u64 { (x + a - 1) / a * a };

    // Phase 1a: Emit entry point FIRST so the Multiboot header is at offset 0
    if let Some(kernel_entry) = entry_point {
        let (code, attribs, size, mut entry_relocs) = x86_64::generate_entry_point(kernel_entry);
        let boot_hash = hash_bytes(&code);

        for r in &mut entry_relocs {
            r.offset += text_offset;
        }
        relocs.extend(entry_relocs);

        // No split: _start points to the beginning of the entry code (offset 0).
        // The Multiboot header is placed AFTER the code (at a known offset < 8K)
        // so the entry point can start executing immediately.
        func_sections.push(("_start".to_string(), code, attribs, size));
        output.receipts.push(FunctionReceipt {
            name: "_start".to_string(),
            source_span: (0, 0),
            byte_offset: text_offset,
            byte_len: size,
            effect_set: Vec::new(),
            verified: true,
        });
        text_offset = align_up(text_offset + size, ALIGN);

        output.abi_manifest.functions.push(AbiFunction {
            name: "_start".to_string(),
            linkage: AbiLinkage::Global,
            calling_convention: "SystemV".to_string(),
            stack_frame_size: 0,
        });

        // Add boot receipt with kernel metadata
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        output.residuals.push(CompileResidual {
            stage: "boot_entry".to_string(),
            input_hash: hash_bytes(kernel_entry.as_bytes()),
            output_hash: boot_hash,
            timestamp,
            function_count: 1,
            total_bytes: size as u64,
        });
    }

    // Phase 1b: User functions (appended after entry point)
    let mut encode_failures: Vec<String> = Vec::new();
    for func in &module.functions {
        let (code, attribs, size, func_relocs, encoded_ok) = x86_64::compile_function(func);
        if !encoded_ok {
            encode_failures.push(func.name.clone());
        }

        // Adjust per-function relocation offsets by the function's text section offset
        for mut r in func_relocs {
            r.offset += text_offset;
            relocs.push(r);
        }

        // Mangle function name (module prefix omitted — PHIR name carries owner qualification)
        let mangled = mangle_name(&mod_prefix, &func.name);
        func_sections.push((mangled.clone(), code, attribs, size));

        // Receipt with real text offset (tracked as we append functions)
        output.receipts.push(FunctionReceipt {
            name: func.name.clone(),
            source_span: (0, 0),
            byte_offset: text_offset,
            byte_len: size,
            effect_set: func.effects.clone(),
            verified: encoded_ok,
        });
        text_offset = align_up(text_offset + size, ALIGN);

        output.abi_manifest.functions.push(AbiFunction {
            name: mangled,
            linkage: AbiLinkage::Global,
            calling_convention: "SystemV".to_string(),
            stack_frame_size: 0,
        });
    }

    // Report any functions that failed to encode. These were emitted as
    // non-executable NOP placeholders; the receipt `verified` flag is false for
    // them. This is deliberately loud: it is the signal that a compiled artifact
    // is not executable (the JIT-porting court gates execution on it).
    if !encode_failures.is_empty() {
        let names = encode_failures.join(", ");
        eprintln!(
            "phorc: warning: {} function(s) failed instruction encoding and were emitted as non-executable placeholders: {}",
            encode_failures.len(),
            names
        );
        output.residuals.push(CompileResidual {
            stage: format!("encode_failed:{}functions", encode_failures.len()),
            input_hash: hash_bytes(module.source_name.as_bytes()),
            output_hash: hash_bytes(names.as_bytes()),
            timestamp: 0,
            function_count: encode_failures.len(),
            total_bytes: 0,
        });
    }

    // Phase 1c: Generate intrinsic stubs for any undefined intrinsic symbols
    // Collect the set of defined symbol names to detect which relocations
    // target functions not defined in this compilation unit.
    let defined_names: HashSet<String> =
        func_sections.iter().map(|(n, _, _, _)| n.clone()).collect();
    let mut intrinsic_names: Vec<String> = Vec::new();
    for reloc in &relocs {
        if defined_names.contains(&reloc.target) {
            continue;
        }
        // Strip _phor_ prefix to get the intrinsic name
        let name = reloc
            .target
            .strip_prefix("_phor_")
            .unwrap_or(&reloc.target)
            .to_string();
        if is_known_intrinsic(&name) && !intrinsic_names.contains(&name) {
            intrinsic_names.push(name);
        }
    }

    for name in &intrinsic_names {
        let mangled = format!("_phor_{}", name);
        let (code, attribs, size) = generate_stub(name);
        func_sections.push((mangled.clone(), code, attribs, size));

        output.receipts.push(FunctionReceipt {
            name: format!("intrinsic:{}", name),
            source_span: (0, 0),
            byte_offset: text_offset,
            byte_len: size,
            effect_set: Vec::new(),
            verified: false,
        });
        text_offset = align_up(text_offset + size, ALIGN);

        output.abi_manifest.functions.push(AbiFunction {
            name: mangled,
            linkage: AbiLinkage::Global,
            calling_convention: "SystemV".to_string(),
            stack_frame_size: 0,
        });
    }

    // Phase 2: Write ELF64 object file
    // Build byte_map with section-level offsets
    let mut sec_offset: u64 = 0;
    for (_mangled, code, attribs, _size) in &func_sections {
        for attr in attribs {
            output.byte_map.push(ByteAttribution {
                offset: sec_offset + attr.offset,
                ..attr.clone()
            });
        }
        sec_offset = align_up(sec_offset + code.len() as u64, 16);
    }

    // Determine entry address for PVH note (kernel loaded at 0x100000 via linker script)
    let entry_addr: Option<u64> = if entry_point.is_some() {
        Some(0x100000u64)
    } else {
        None
    };
    output.object_bytes = object::write_elf64_object(
        &module.source_name,
        &func_sections,
        &module.globals,
        &mut relocs,
        entry_addr,
    );

    output.relocations = relocs;

    // Phase 3: Compute residuals
    output.residuals.push(CompileResidual {
        stage: "codegen".to_string(),
        input_hash: hash_bytes(&[]),
        output_hash: hash_bytes(&output.object_bytes),
        timestamp: 0,
        function_count: module.functions.len(),
        total_bytes: output.object_bytes.len() as u64,
    });

    output
}
