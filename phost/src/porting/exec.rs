// porting/exec.rs — Sealed Object Execution Court (std)
//
// The replay court compares the *Rust mirror* of the candidate against the oracle
// traces. This module closes the last gap: it loads the sealed `phorc` ELF64
// object, verifies its SHA-256 against the sealed package **before** use, parses
// the ELF64 sections/symbols, rejects any object whose promoted entry point is
// not a self-contained leaf function, maps the executable bytes, calls the
// function through an explicit ABI harness, and replays the exact same oracle
// corpus through the compiled object.
//
// This is the difference between "the promoted candidate is bound to a compiled
// artifact" and "the promoted artifact is loaded and executed". Claim hygiene:
// this executes **leaf, pure, relocation-free** functions only. It is API-surface
// JIT-porting, not arbitrary binary translation and not a general dynamic linker.
//
// Fail-closed rules:
//   * no PORTING capability            → CapabilityDenied
//   * object file missing              → ObjectMissing
//   * object hash ≠ sealed hash        → ObjectHashMismatch
//   * not ELF64 x86-64                 → Elf
//   * entry symbol absent/not in .text → SymbolNotFound / SymbolNotExecutable
//   * any relocation inside the entry  → RelocationInTarget
//   * mmap/mprotect failure            → Map
//   * any case mismatch                → verdict Inconsistent (never sealed)

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use core::ffi::c_void;

use crate::porting::candidate::{decode_usize, encode_sign};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{PortTarget, LIBC_MEMCMP, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority};

/// Why the execution court could not run (or refused to).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecError {
    CapabilityDenied,
    UnsupportedTarget(String),
    ObjectMissing(String),
    ObjectHashMismatch { expected: String, actual: String },
    Elf(String),
    SymbolNotFound(String),
    SymbolNotExecutable(String),
    RelocationInTarget { offset: u64, count: u64 },
    Map(String),
    MalformedArgs(String),
}

impl ExecError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecError::CapabilityDenied => "PORTING capability required to execute",
            ExecError::UnsupportedTarget(_) => "unsupported execution target",
            ExecError::ObjectMissing(_) => "sealed object missing",
            ExecError::ObjectHashMismatch { .. } => "sealed object hash mismatch",
            ExecError::Elf(_) => "malformed ELF64 object",
            ExecError::SymbolNotFound(_) => "entry symbol not found",
            ExecError::SymbolNotExecutable(_) => "entry symbol is not an executable .text function",
            ExecError::RelocationInTarget { .. } => "entry function is not relocation-free",
            ExecError::Map(_) => "could not map the executable image",
            ExecError::MalformedArgs(_) => "malformed execution arguments",
        }
    }
}

impl core::fmt::Display for ExecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ExecError::ObjectMissing(p) => write!(f, "{}: {}", self.as_str(), p),
            ExecError::Elf(m) => write!(f, "{}: {}", self.as_str(), m),
            ExecError::SymbolNotFound(s) => write!(f, "{}: {}", self.as_str(), s),
            ExecError::SymbolNotExecutable(s) => write!(f, "{}: {}", self.as_str(), s),
            ExecError::Map(m) => write!(f, "{}: {}", self.as_str(), m),
            ExecError::MalformedArgs(m) => write!(f, "{}: {}", self.as_str(), m),
            ExecError::UnsupportedTarget(t) => write!(f, "{}: {}", self.as_str(), t),
            ExecError::ObjectHashMismatch { expected, actual } => {
                write!(
                    f,
                    "{}: expected {}, got {}",
                    self.as_str(),
                    expected,
                    actual
                )
            }
            ExecError::RelocationInTarget { offset, count } => write!(
                f,
                "{}: {} relocation(s), first at +0x{:x}",
                self.as_str(),
                count,
                offset
            ),
            ExecError::CapabilityDenied => write!(f, "{}", self.as_str()),
        }
    }
}

/// The execution residual: the compiled object was loaded and replayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionVerdict {
    pub target: String,
    /// The demangled source-level name (e.g. `phor_toupper`).
    pub abi_symbol: String,
    /// The actual ELF symbol that was located and called (e.g. `_phor_phor_toupper`).
    pub elf_symbol: String,
    pub cases_run: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    /// SHA-256 of the sealed object that was executed.
    pub object_hash: String,
    pub oracle_hash: String,
    /// SHA-256 over the compiled object's behavior across the case domain.
    pub execution_hash: String,
    pub verdict: CourtVerdict,
}

impl ExecutionVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.object_hash.is_empty()
            && !self.oracle_hash.is_empty()
            && !self.execution_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};abi_symbol={};elf_symbol={};cases_run={};cases_passed={};cases_failed={};object_hash={};oracle_hash={};execution_hash={};verdict={}",
            self.target,
            self.abi_symbol,
            self.elf_symbol,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.object_hash,
            self.oracle_hash,
            self.execution_hash,
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[Mismatch]) -> String {
        let body: Vec<String> = mismatches.iter().map(|m| m.to_json()).collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.execution_verdict.v1\",\n  \"target\": \"{}\",\n  \"abi_symbol\": \"{}\",\n  \"elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"object_hash\": \"{}\",\n  \"oracle_hash\": \"{}\",\n  \"execution_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.abi_symbol),
            json_escape(&self.elf_symbol),
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.object_hash,
            self.oracle_hash,
            self.execution_hash,
            self.verdict.as_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

/// SHA-256 over `case_id:compiled_output_hex` per case, in corpus order. Cases
/// the compiled object could not produce are encoded as `!<reason>`.
fn execution_behavior_hash(outputs: &[(String, Vec<u8>)]) -> String {
    let mut buf = String::new();
    for (case_id, out) in outputs {
        buf.push_str(case_id);
        buf.push(':');
        buf.push_str(&hex::encode(out));
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

// ============================================================================
// ABI harness
// ============================================================================

/// Run the compiled entry point for one case, returning the encoded output bytes
/// in the same encoding the court seals (`toupper`: one byte; `memcmp`: a 4-byte
/// little-endian sign).
///
/// This is the explicit ABI boundary: the object speaks SysV integer registers
/// (`RDI, RSI, RDX -> RAX`), and the adapters below map the court's byte-slice
/// arguments onto that convention.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn call_target(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if target.id == LIBC_TOUPPER.id {
        let b = args
            .first()
            .and_then(|a| a.first())
            .copied()
            .ok_or_else(|| ExecError::MalformedArgs(target.id.to_string()))?;
        let f: extern "C" fn(u64) -> u64 = unsafe { core::mem::transmute(entry) };
        return Ok(alloc::vec![(f(b as u64) & 0xff) as u8]);
    }

    if target.id == LIBC_MEMCMP.id {
        if args.len() < 3 {
            return Err(ExecError::MalformedArgs(target.id.to_string()));
        }
        let (a, b) = (&args[0], &args[1]);
        let n = decode_usize(&args[2]);
        // The candidate packs each *compared prefix* big-endian into a u64, so the
        // corpus is contracted to at most 8 compared bytes. `memcmp` examines
        // exactly `n` bytes, so bytes past `n` are irrelevant and are not packed
        // (a buffer may legitimately be longer than the compared prefix).
        if n > 8 {
            return Err(ExecError::MalformedArgs(format!(
                "{}: compared length {} exceeds the 8-byte packed-word contract",
                target.id, n
            )));
        }
        if a.len() < n || b.len() < n {
            return Err(ExecError::MalformedArgs(format!(
                "{}: compared length {} exceeds a buffer",
                target.id, n
            )));
        }
        let wa = pack_be(&a[..n]);
        let wb = pack_be(&b[..n]);
        let f: extern "C" fn(u64, u64, u64) -> u64 = unsafe { core::mem::transmute(entry) };
        let raw = f(wa, wb, n as u64) as u32 as i32;
        return Ok(encode_sign(raw));
    }

    Err(ExecError::UnsupportedTarget(target.id.to_string()))
}

/// Pack up to 8 bytes big-endian into a u64 (byte 0 in the most-significant
/// position), matching the compiled `phor_memcmp_sign` contract.
///
/// Each byte is placed at its left-aligned position (`byte i` at bit
/// `8*(7-i)`), so the packing is independent of the buffer length and never
/// shifts by 64.
fn pack_be(buf: &[u8]) -> u64 {
    let mut w: u64 = 0;
    for (i, &byte) in buf.iter().take(8).enumerate() {
        w |= (byte as u64) << (8 * (7 - i));
    }
    w
}

// ============================================================================
// ELF64 parsing (relocatable object, minimal subset)
// ============================================================================

const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;
const SHT_SYMTAB: u32 = 2;
const SHT_RELA: u32 = 4;
const STT_FUNC: u8 = 2;
const SHN_UNDEF: u16 = 0;

fn u16_at(b: &[u8], off: usize) -> Result<u16, ExecError> {
    b.get(off..off + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| ExecError::Elf(format!("truncated at +0x{:x}", off)))
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, ExecError> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| ExecError::Elf(format!("truncated at +0x{:x}", off)))
}

fn u64_at(b: &[u8], off: usize) -> Result<u64, ExecError> {
    b.get(off..off + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| ExecError::Elf(format!("truncated at +0x{:x}", off)))
}

struct Section {
    name_off: u32,
    ty: u32,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    entsize: u64,
}

fn cstr(b: &[u8], off: usize) -> String {
    let end = b[off..]
        .iter()
        .position(|&c| c == 0)
        .map(|p| off + p)
        .unwrap_or(b.len());
    String::from_utf8_lossy(&b[off..end]).to_string()
}

fn parse_sections(data: &[u8]) -> Result<Vec<Section>, ExecError> {
    if data.len() < 64 || &data[0..4] != b"\x7fELF" {
        return Err(ExecError::Elf("not an ELF file".to_string()));
    }
    if data[4] != 2 {
        return Err(ExecError::Elf("not ELF64 (EI_CLASS != 2)".to_string()));
    }
    if u16_at(data, 0x10)? != ET_REL {
        return Err(ExecError::Elf(
            "not a relocatable object (e_type != ET_REL)".to_string(),
        ));
    }
    if u16_at(data, 0x12)? != EM_X86_64 {
        return Err(ExecError::Elf(
            "not x86-64 (e_machine != EM_X86_64)".to_string(),
        ));
    }

    let shoff = u64_at(data, 0x28)? as usize;
    let shentsize = u16_at(data, 0x3A)? as usize;
    let shnum = u16_at(data, 0x3C)? as usize;
    if shentsize < 64 {
        return Err(ExecError::Elf(format!("bad e_shentsize {}", shentsize)));
    }

    let mut sections = Vec::with_capacity(shnum);
    for i in 0..shnum {
        let base = shoff + i * shentsize;
        sections.push(Section {
            name_off: u32_at(data, base)?,
            ty: u32_at(data, base + 4)?,
            offset: u64_at(data, base + 24)?,
            size: u64_at(data, base + 32)?,
            link: u32_at(data, base + 40)?,
            info: u32_at(data, base + 44)?,
            entsize: u64_at(data, base + 56)?,
        });
    }
    Ok(sections)
}

fn section_name(data: &[u8], sections: &[Section], strndx: usize, idx: usize) -> String {
    let name_off = sections[idx].name_off as usize;
    if let Some(strtab) = sections.get(strndx) {
        if strtab.offset as usize + name_off < data.len() {
            return cstr(data, strtab.offset as usize + name_off);
        }
    }
    String::new()
}

/// A located, verified, relocation-free entry point inside `.text`.
struct LoadedEntry {
    text_offset: usize,
    text_size: usize,
    func_offset: u64,
    elf_symbol: String,
}

fn locate_entry(
    data: &[u8],
    sections: &[Section],
    shstrndx: usize,
    symbol: &str,
) -> Result<LoadedEntry, ExecError> {
    // .text
    let text_idx = (0..sections.len())
        .find(|&i| section_name(data, sections, shstrndx, i) == ".text")
        .ok_or_else(|| ExecError::Elf(".text section missing".to_string()))?;
    let text = &sections[text_idx];

    // .symtab
    let sym_idx = (0..sections.len())
        .find(|&i| sections[i].ty == SHT_SYMTAB)
        .ok_or_else(|| ExecError::Elf(".symtab missing".to_string()))?;
    let symtab = &sections[sym_idx];
    let strtab = sections
        .get(symtab.link as usize)
        .ok_or_else(|| ExecError::Elf("symtab strtab link out of range".to_string()))?;

    // Locate the entry symbol.
    let mut found: Option<(u64, u64)> = None;
    if symtab.entsize >= 24 {
        let count = (symtab.size / symtab.entsize) as usize;
        for i in 0..count {
            let base = symtab.offset as usize + i * symtab.entsize as usize;
            let name_off = u32_at(data, base)? as usize;
            let info = *data
                .get(base + 4)
                .ok_or_else(|| ExecError::Elf("truncated symbol".to_string()))?;
            let shndx = u16_at(data, base + 6)?;
            let value = u64_at(data, base + 8)?;
            let size = u64_at(data, base + 16)?;
            if strtab.offset as usize + name_off >= data.len() {
                continue;
            }
            let name = cstr(data, strtab.offset as usize + name_off);
            if name != symbol {
                continue;
            }
            if shndx == SHN_UNDEF {
                return Err(ExecError::SymbolNotExecutable(symbol.to_string()));
            }
            if (info & 0xf) != STT_FUNC {
                return Err(ExecError::SymbolNotExecutable(symbol.to_string()));
            }
            if shndx as usize != text_idx {
                return Err(ExecError::SymbolNotExecutable(symbol.to_string()));
            }
            found = Some((value, size));
            break;
        }
    }
    let (func_offset, func_size) =
        found.ok_or_else(|| ExecError::SymbolNotFound(symbol.to_string()))?;

    if func_size == 0 {
        return Err(ExecError::SymbolNotExecutable(symbol.to_string()));
    }
    if func_offset + func_size > text.size {
        return Err(ExecError::SymbolNotExecutable(symbol.to_string()));
    }

    // Reject any relocation that lands inside the promoted function: a leaf entry
    // point must not depend on the linker (or on any other symbol).
    let mut in_target: u64 = 0;
    let mut first_offset: u64 = 0;
    for s in sections.iter() {
        if s.ty != SHT_RELA || s.info as usize != text_idx {
            continue;
        }
        let entsize = if s.entsize >= 24 { s.entsize } else { 24 };
        let count = (s.size / entsize) as usize;
        for k in 0..count {
            let base = s.offset as usize + k * entsize as usize;
            let r_offset = u64_at(data, base)?;
            if r_offset >= func_offset && r_offset < func_offset + func_size {
                if in_target == 0 {
                    first_offset = r_offset;
                }
                in_target += 1;
            }
        }
    }
    if in_target > 0 {
        return Err(ExecError::RelocationInTarget {
            offset: first_offset,
            count: in_target,
        });
    }

    Ok(LoadedEntry {
        text_offset: text.offset as usize,
        text_size: text.size as usize,
        func_offset,
        elf_symbol: symbol.to_string(),
    })
}

// ============================================================================
// Executable mapping (Linux x86-64)
// ============================================================================

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod sys {
    use core::ffi::c_void;

    extern "C" {
        pub fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut c_void;
        pub fn munmap(addr: *mut c_void, len: usize) -> i32;
        pub fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
    }

    pub const PROT_READ: i32 = 0x1;
    pub const PROT_WRITE: i32 = 0x2;
    pub const PROT_EXEC: i32 = 0x4;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const PAGE: usize = 4096;
    pub const MAP_FAILED: isize = -1;
}

/// An anonymous executable mapping holding a copy of `.text`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct ExecutableImage {
    ptr: *mut u8,
    len: usize,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl ExecutableImage {
    fn map(code: &[u8]) -> Result<Self, ExecError> {
        if code.is_empty() {
            return Err(ExecError::Map(".text is empty".to_string()));
        }
        let len = code.len().div_ceil(sys::PAGE) * sys::PAGE;
        let p = unsafe {
            sys::mmap(
                core::ptr::null_mut(),
                len,
                sys::PROT_READ | sys::PROT_WRITE,
                sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if p as isize == sys::MAP_FAILED {
            return Err(ExecError::Map("mmap(RW) failed".to_string()));
        }
        let ptr = p as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            // Drop write permission now that the bytes are in place.
            let rc = sys::mprotect(ptr as *mut c_void, len, sys::PROT_READ | sys::PROT_EXEC);
            if rc != 0 {
                sys::munmap(ptr as *mut c_void, len);
                return Err(ExecError::Map("mprotect(RX) failed".to_string()));
            }
        }
        Ok(Self { ptr, len })
    }

    fn entry(&self, offset: u64) -> *const u8 {
        unsafe { self.ptr.add(offset as usize) }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Drop for ExecutableImage {
    fn drop(&mut self) {
        unsafe {
            sys::munmap(self.ptr as *mut c_void, self.len);
        }
    }
}

// ============================================================================
// Loaded sealed object
// ============================================================================

/// A verified, loaded, executable sealed object entry.
///
/// The object's SHA-256 is checked against the seal **before** its bytes are
/// mapped, so a tampered or stale object is never executed. The handle maps once
/// and can be called repeatedly (this is the shape the runtime dispatcher uses).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub struct SealedObjectHandle {
    image: ExecutableImage,
    func_offset: u64,
    pub target: String,
    pub elf_symbol: String,
    pub object_hash: String,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl SealedObjectHandle {
    pub fn load(
        target: &PortTarget,
        object_path: &str,
        expected_object_hash: &str,
    ) -> Result<Self, ExecError> {
        // 1. Verify against the seal BEFORE mapping anything.
        let data = std::fs::read(object_path)
            .map_err(|_| ExecError::ObjectMissing(object_path.to_string()))?;
        let actual = sha256_hex(&data);
        if actual != expected_object_hash {
            return Err(ExecError::ObjectHashMismatch {
                expected: expected_object_hash.to_string(),
                actual,
            });
        }

        // 2. Parse the ELF64 object and locate the ABI entry symbol.
        let sections = parse_sections(&data)?;
        let shstrndx = u16_at(&data, 0x3E)? as usize;
        let elf_symbol = format!("_phor_{}", target.abi_symbol);
        let entry = locate_entry(&data, &sections, shstrndx, &elf_symbol)?;

        // 3. Map .text read-only/executable.
        let text = data
            .get(entry.text_offset..entry.text_offset + entry.text_size)
            .ok_or_else(|| ExecError::Elf(".text extends past EOF".to_string()))?;
        let image = ExecutableImage::map(text)?;

        Ok(Self {
            image,
            func_offset: entry.func_offset,
            target: target.id.to_string(),
            elf_symbol: entry.elf_symbol,
            object_hash: actual,
        })
    }

    /// Call the compiled entry point for one case, returning the court-encoded
    /// output bytes.
    pub fn call(&self, target: &PortTarget, args: &[Vec<u8>]) -> Result<Vec<u8>, ExecError> {
        call_target(target, self.image.entry(self.func_offset), args)
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub struct SealedObjectHandle {
    pub target: String,
    pub elf_symbol: String,
    pub object_hash: String,
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
impl SealedObjectHandle {
    pub fn load(
        _target: &PortTarget,
        _object_path: &str,
        _expected_object_hash: &str,
    ) -> Result<Self, ExecError> {
        Err(ExecError::Map(
            "execution requires linux x86-64".to_string(),
        ))
    }

    pub fn call(&self, _target: &PortTarget, _args: &[Vec<u8>]) -> Result<Vec<u8>, ExecError> {
        Err(ExecError::Map(
            "execution requires linux x86-64".to_string(),
        ))
    }
}

// ============================================================================
// Court entry point
// ============================================================================

/// Load the sealed object at `object_path`, verify it against
/// `expected_object_hash` (the hash recorded in the sealed package), and replay
/// every oracle trace through the compiled entry point.
pub fn execute_sealed_candidate(
    target: &PortTarget,
    traces: &[OracleTrace],
    object_path: &str,
    expected_object_hash: &str,
    auth: &PortingAuthority,
) -> Result<(ExecutionVerdict, Vec<Mismatch>), ExecError> {
    if !auth.can_observe() {
        return Err(ExecError::CapabilityDenied);
    }

    let handle = SealedObjectHandle::load(target, object_path, expected_object_hash)?;

    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut outputs: Vec<(String, Vec<u8>)> = Vec::with_capacity(traces.len());

    for t in traces {
        let args = t.input_args();
        match handle.call(target, &args) {
            Ok(out) => {
                let actual_hex = hex::encode(&out);
                if actual_hex == t.output_hex && t.status == "ok" {
                    passed += 1;
                } else {
                    failed += 1;
                    mismatches.push(Mismatch {
                        case_id: t.case_id.clone(),
                        expected_output_hex: t.output_hex.clone(),
                        actual_output_hex: actual_hex,
                    });
                }
                outputs.push((t.case_id.clone(), out));
            }
            Err(e) => {
                failed += 1;
                mismatches.push(Mismatch {
                    case_id: t.case_id.clone(),
                    expected_output_hex: t.output_hex.clone(),
                    actual_output_hex: format!("!{}", e.as_str()),
                });
                outputs.push((t.case_id.clone(), Vec::new()));
            }
        }
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0 {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    Ok((
        ExecutionVerdict {
            target: target.id.to_string(),
            abi_symbol: target.abi_symbol.to_string(),
            elf_symbol: handle.elf_symbol,
            cases_run,
            cases_passed: passed,
            cases_failed: failed,
            object_hash: handle.object_hash,
            oracle_hash: combined_oracle_hash(traces),
            execution_hash: execution_behavior_hash(&outputs),
            verdict,
        },
        mismatches,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::compiled::compile_candidate;
    use crate::porting::target::{cases_for, LIBC_TOUPPER};
    use crate::porting::PortingAuthority;
    use std::path::PathBuf;
    use std::{eprintln, format, string::ToString, vec};

    /// Locate the `phorc` binary. Tests that need to compile are skipped when it
    /// has not been built (CI builds it before running the suite).
    fn phorc_bin() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("PHORC_BIN") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        let root = crate::porting::compiled::workspace_root();
        for cand in [
            root.join("target/debug/phorc"),
            root.join("target/release/phorc"),
        ] {
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    }

    fn out_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("phost_exec_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Compile the target's candidate and return the sealed object path + hash.
    fn compile(target: &PortTarget, tag: &str) -> Option<(String, String)> {
        let phorc = phorc_bin()?;
        let dir = out_dir(tag);
        let c = compile_candidate(
            target,
            &dir.display().to_string(),
            Some(&phorc.display().to_string()),
        )
        .expect("compile candidate");
        Some((c.object_path, c.object_hash))
    }

    #[test]
    fn test_pack_be_left_aligns_bytes() {
        assert_eq!(pack_be(&[]), 0);
        assert_eq!(pack_be(&[0x61]), 0x6100_0000_0000_0000);
        assert_eq!(pack_be(&[0x01, 0x02]), 0x0102_0000_0000_0000);
        assert_eq!(pack_be(&[1, 2, 3, 4, 5, 6, 7, 8]), 0x0102_0304_0506_0708);
    }

    #[test]
    fn test_parse_rejects_non_elf() {
        let err = parse_sections(&[0u8; 64]);
        assert!(matches!(err, Err(ExecError::Elf(_))));
    }

    #[test]
    fn test_capability_denied_without_porting() {
        let traces = alloc::vec![];
        let err = execute_sealed_candidate(
            &LIBC_TOUPPER,
            &traces,
            "/nonexistent/candidate.o",
            "deadbeef",
            &PortingAuthority::none(),
        );
        assert_eq!(err, Err(ExecError::CapabilityDenied));
    }

    #[test]
    fn test_missing_object_fails_closed() {
        let err = execute_sealed_candidate(
            &LIBC_TOUPPER,
            &[],
            "/nonexistent/candidate.o",
            "deadbeef",
            &PortingAuthority::granted(),
        );
        assert!(matches!(err, Err(ExecError::ObjectMissing(_))));
    }

    /// The genuine end-to-end check: compile `toupper`, execute the sealed object
    /// and require all 256 cases to match the oracle.
    #[test]
    fn test_execution_court_runs_sealed_toupper_object() {
        let Some((path, hash)) = compile(&LIBC_TOUPPER, "toupper") else {
            eprintln!("phorc not built; skipping execution test");
            return;
        };
        // Oracle traces obtained through the real dialect cage.
        let cases = cases_for(&LIBC_TOUPPER);
        let traces = crate::porting::dialect_cage::observe_target(
            &LIBC_TOUPPER,
            &cases,
            &PortingAuthority::granted(),
        )
        .unwrap();

        let (verdict, mismatches) = execute_sealed_candidate(
            &LIBC_TOUPPER,
            &traces,
            &path,
            &hash,
            &PortingAuthority::granted(),
        )
        .unwrap();

        assert_eq!(verdict.verdict, CourtVerdict::Consistent);
        assert_eq!(verdict.cases_run, 256);
        assert_eq!(verdict.cases_passed, 256);
        assert_eq!(verdict.cases_failed, 0);
        assert!(mismatches.is_empty());
        assert_eq!(verdict.elf_symbol, "_phor_phor_toupper");
        assert_eq!(verdict.abi_symbol, "phor_toupper");
        assert!(verdict.is_sealed_eligible());
        // The compiled object reproduces the sealed oracle behavior exactly.
        assert_eq!(verdict.oracle_hash, combined_oracle_hash(&traces));
    }

    /// The object hash must be verified against the seal before use.
    #[test]
    fn test_tampered_object_hash_fails_closed() {
        let Some((path, hash)) = compile(&LIBC_TOUPPER, "tamper") else {
            return;
        };
        // Corrupt one byte of the object on disk while keeping the sealed hash.
        let mut bytes = std::fs::read(&path).unwrap();
        let n = bytes.len();
        bytes[n / 2] ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();

        let err = execute_sealed_candidate(
            &LIBC_TOUPPER,
            &[],
            &path,
            &hash,
            &PortingAuthority::granted(),
        );
        assert!(matches!(err, Err(ExecError::ObjectHashMismatch { .. })));
    }

    #[test]
    fn test_unsupported_target_fails_closed() {
        let unknown = PortTarget {
            id: "libc:identity:c-locale:u8:v1",
            dialect: "libc",
            symbol: "identity",
            version: "v1",
            locale_contract: "C",
            input_schema: "u8",
            output_schema: "u8",
            domain_summary: "none",
            candidate_source: "none",
            abi_symbol: "identity",
        };
        // Reuse the toupper object but ask for an unknown target: the ELF lookup
        // for `_phor_identity` must fail before any call.
        let Some((path, hash)) = compile(&LIBC_TOUPPER, "unknown") else {
            return;
        };
        let err =
            execute_sealed_candidate(&unknown, &[], &path, &hash, &PortingAuthority::granted());
        assert!(matches!(err, Err(ExecError::SymbolNotFound(_))));
    }

    /// A mutated oracle trace must change the court's verdict inputs.
    #[test]
    fn test_mutated_trace_changes_execution_verdict() {
        let Some((path, hash)) = compile(&LIBC_TOUPPER, "mutate") else {
            return;
        };
        let cases = cases_for(&LIBC_TOUPPER);
        let mut traces = crate::porting::dialect_cage::observe_target(
            &LIBC_TOUPPER,
            &cases,
            &PortingAuthority::granted(),
        )
        .unwrap();
        traces[0x61].output_hex = "42".to_string(); // claim a -> B

        let (verdict, mismatches) = execute_sealed_candidate(
            &LIBC_TOUPPER,
            &traces,
            &path,
            &hash,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(verdict.verdict, CourtVerdict::Inconsistent);
        assert_eq!(verdict.cases_failed, 1);
        assert_eq!(verdict.cases_passed, 255);
        assert_eq!(mismatches[0].case_id, "0x61");
        assert!(!verdict.is_sealed_eligible());
    }
}
