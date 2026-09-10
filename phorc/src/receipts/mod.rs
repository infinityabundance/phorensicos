// Phorc Receipts — Byte attribution and compilation receipts
// Every emitted byte knows why it exists.

pub use crate::ir::FunctionReceipt;
use crate::ir::{AbiManifest, ByteAttribution};

/// The complete output of a compilation
#[derive(Debug)]
pub struct CompileOutput {
    /// ELF64 object file bytes
    pub object_bytes: Vec<u8>,
    /// Per-function receipts
    pub receipts: Vec<FunctionReceipt>,
    /// Byte-level provenance map
    pub byte_map: Vec<ByteAttribution>,
    /// Relocation entries
    pub relocations: Vec<RelocationEntry>,
    /// ABI manifest
    pub abi_manifest: AbiManifest,
    /// Residual records from compilation
    pub residuals: Vec<CompileResidual>,
}

#[derive(Debug, Clone)]
pub struct RelocationEntry {
    pub offset: u64,
    pub kind: RelocationKind,
    pub target: String,
    pub addend: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocationKind {
    Absolute64,
    Relative32,
    Relative64,
    GOT64,
    PLT32,
}

/// A residual record from the compilation process itself
#[derive(Debug, Clone)]
pub struct CompileResidual {
    pub stage: String,
    pub input_hash: [u8; 32],
    pub output_hash: [u8; 32],
    pub timestamp: u64,
    pub function_count: usize,
    pub total_bytes: u64,
}

impl CompileOutput {
    /// Serialize receipts to JSON
    pub fn receipts_to_json(&self) -> String {
        let mut s = String::from("{\n");
        s.push_str(&format!("  \"function_count\": {},\n", self.receipts.len()));
        s.push_str("  \"functions\": [\n");
        for (i, r) in self.receipts.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"name\": \"{}\", \"offset\": {}, \"len\": {}, \"verified\": {}}}",
                r.name, r.byte_offset, r.byte_len, r.verified
            ));
            if i < self.receipts.len() - 1 {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("  ],\n");

        // Byte attribution metrics
        let attr_count = self.byte_map.len();
        let text_bytes: u64 = self.byte_map.iter().map(|b| b.len).sum();
        let obj_bytes = self.object_bytes.len() as u64;
        s.push_str(&format!("  \"byte_attribution_count\": {},\n", attr_count));
        s.push_str(&format!("  \"emitted_text_bytes\": {},\n", text_bytes));
        s.push_str(&format!("  \"object_bytes\": {},\n", obj_bytes));

        // Residual records (includes boot receipt when --emit-kernel is used)
        s.push_str(&format!("  \"residuals\": [\n"));
        for (i, r) in self.residuals.iter().enumerate() {
            let in_hash = r
                .input_hash
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join("");
            let out_hash = r
                .output_hash
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join("");
            s.push_str(&format!(
                "    {{\"stage\": \"{}\", \"input_hash\": \"{}\", \"output_hash\": \"{}\", \"ts\": {}, \"fns\": {}, \"bytes\": {}}}",
                r.stage, in_hash, out_hash, r.timestamp, r.function_count, r.total_bytes
            ));
            if i < self.residuals.len() - 1 {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("  ],\n");

        // Kernel boot metadata (extracted from _start receipt if present)
        let boot_info = self.receipts.iter().find(|r| r.name == "_start");
        if let Some(start) = boot_info {
            let entry_vma = 0x100000u64;
            s.push_str(&format!("  \"boot_entry_vma\": {},\n", entry_vma));
            s.push_str(&format!("  \"boot_size\": {},\n", start.byte_len));
            s.push_str(&format!("  \"boot_verified\": {}\n", start.verified));
        } else {
            s.push_str(&format!("  \"boot_entry\": null\n"));
        }
        s.push_str("}\n");
        s
    }
}

/// Compute a SHA-256 hash for evidence-grade receipt tracking.
/// Used for compilation residuals, boot receipts, and ELF artifact hashing.
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result: [u8; 32] = hasher.finalize().into();
    result
}
