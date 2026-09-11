// porting/abi.rs — the per-target ABI extension boundary
//
// Phase 1 of the autonomous porting foundry. The generic execution court
// (`exec.rs`) loads a sealed ELF64 object, locates the entry symbol and calls it
// through an explicit SysV integer ABI. *How* a target's byte-slice arguments map
// onto that register convention is target-specific mechanism, so it lives here —
// one adapter per target, named and registered in `registry.rs` — instead of a
// chain of `if target.id == …` inside the generic workflow.
//
// Each adapter is a leaf function with no target-id branching: it uses the
// target's own id only to label a malformed-argument error. The ABI itself is
// described in the target's `PortSpec` (`AbiSpec { symbol, packing }`); these
// adapters implement that description.
//
// The real bodies are x86-64 Linux. On any other target each adapter fails
// closed rather than guessing a calling convention.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::porting::candidate::{decode_usize, encode_index, encode_sign, encode_usize};
use crate::porting::exec::ExecError;
use crate::porting::target::PortTarget;

/// The signature every ABI adapter implements.
pub type AbiFn = fn(&PortTarget, *const u8, &[Vec<u8>]) -> Result<Vec<u8>, ExecError>;

/// Pack up to 8 bytes **little-endian** — the machine word you get by loading the
/// first 8 bytes of the buffer on x86-64. Byte `i` occupies bits `8*i`, the
/// convention the compiled `phor_memchr_index` reads.
pub fn pack_le_prefix(buf: &[u8]) -> u64 {
    let mut w = [0u8; 8];
    let n = buf.len().min(8);
    w[..n].copy_from_slice(&buf[..n]);
    u64::from_le_bytes(w)
}

/// Pack up to 8 bytes big-endian into a u64 (byte 0 in the most-significant
/// position), matching the compiled `phor_memcmp_sign` contract. Each byte is
/// placed at its left-aligned position so the packing is independent of the
/// buffer length and never shifts by 64.
pub fn pack_be(buf: &[u8]) -> u64 {
    let mut w: u64 = 0;
    for (i, &byte) in buf.iter().take(8).enumerate() {
        w |= (byte as u64) << (8 * (7 - i));
    }
    w
}

const X86_64_LINUX: bool = cfg!(all(target_os = "linux", target_arch = "x86_64"));

macro_rules! unavailable {
    ($target:expr, $entry:expr, $args:expr) => {{
        let _ = ($entry, $args);
        Err(ExecError::UnsupportedTarget($target.id.to_string()))
    }};
}

/// `toupper`: `(u64 byte) -> u64` folded byte in the low 8 bits.
pub fn toupper(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    let b = args
        .first()
        .and_then(|a| a.first())
        .copied()
        .ok_or_else(|| ExecError::MalformedArgs(target.id.to_string()))?;
    let raw = unsafe { crate::porting::exec::invoke(entry, b as u64, 0, 0) };
    Ok(alloc::vec![(raw & 0xff) as u8])
}

/// `memcmp`: `(u64 wa, u64 wb, u64 n) -> u64` sign; each compared prefix is
/// packed big-endian.
pub fn memcmp(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    if args.len() < 3 {
        return Err(ExecError::MalformedArgs(target.id.to_string()));
    }
    let (a, b) = (&args[0], &args[1]);
    let n = decode_usize(&args[2]);
    // The candidate packs each *compared prefix* big-endian into a u64, so the
    // contract bounds the compared length. `memcmp` examines exactly `n` bytes, so
    // bytes past `n` are irrelevant and are not packed.
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
    let raw = unsafe { crate::porting::exec::invoke(entry, wa, wb, n as u64) } as u32 as i32;
    Ok(encode_sign(raw))
}

/// `memchr`: `(u64 wh, u64 needle, u64 n) -> u64` index or `-1`; the searched
/// prefix is packed little-endian.
pub fn memchr(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    if args.len() < 3 {
        return Err(ExecError::MalformedArgs(target.id.to_string()));
    }
    let hay = &args[0];
    let needle = *args[1]
        .first()
        .ok_or_else(|| ExecError::MalformedArgs(target.id.to_string()))?;
    let n = decode_usize(&args[2]);
    if n > 8 || hay.len() < n {
        return Err(ExecError::MalformedArgs(format!(
            "{}: searched length {} exceeds the 8-byte packed-word contract or the haystack",
            target.id, n
        )));
    }
    let wh = pack_le_prefix(&hay[..n]);
    let raw =
        unsafe { crate::porting::exec::invoke(entry, wh, needle as u64, n as u64) } as u32 as i32;
    Ok(encode_index(raw))
}

/// `strlen`: `(u64 w, u64 n) -> u64` length; the buffer is packed little-endian.
pub fn strlen(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    if args.len() < 2 {
        return Err(ExecError::MalformedArgs(target.id.to_string()));
    }
    let buf = &args[0];
    let n = decode_usize(&args[1]);
    if n > 8 || buf.len() < n {
        return Err(ExecError::MalformedArgs(format!(
            "{}: scanned length {} exceeds the 8-byte packed-word contract or the buffer",
            target.id, n
        )));
    }
    let w = pack_le_prefix(&buf[..n]);
    let len = unsafe { crate::porting::exec::invoke(entry, w, n as u64, 0) };
    Ok(encode_usize(len as usize))
}

/// `strrchr`: `(u64 w, u64 needle) -> u64` last in-string index or `-1`; the
/// buffer is packed little-endian and zero-extended.
pub fn strrchr(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    if args.len() < 3 {
        return Err(ExecError::MalformedArgs(target.id.to_string()));
    }
    let buf = &args[0];
    let needle = *args[1]
        .first()
        .ok_or_else(|| ExecError::MalformedArgs(target.id.to_string()))?;
    let n = decode_usize(&args[2]);
    if n > 8 || buf.len() < n {
        return Err(ExecError::MalformedArgs(format!(
            "{}: scanned length {} exceeds the 8-byte packed-word contract or the buffer",
            target.id, n
        )));
    }
    // The ABI precondition: a NUL terminator lies within the packed window, so the
    // word encodes the whole C string and the candidate never reads on.
    if !buf[..n].contains(&0) {
        return Err(ExecError::MalformedArgs(format!(
            "{}: no NUL terminator within the {}-byte bound",
            target.id, n
        )));
    }
    let w = pack_le_prefix(&buf[..n]);
    let raw = unsafe { crate::porting::exec::invoke(entry, w, needle as u64, 0) } as u32 as i32;
    Ok(encode_index(raw))
}

/// `strspn`: `(u64 ws, u64 wa, u64 n) -> u64` span; the string and accept set each
/// travel as their own little-endian packed word.
pub fn strspn(
    target: &PortTarget,
    entry: *const u8,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, ExecError> {
    if !X86_64_LINUX {
        return unavailable!(target, entry, args);
    }
    if args.len() < 3 {
        return Err(ExecError::MalformedArgs(target.id.to_string()));
    }
    let s = &args[0];
    let accept = &args[1];
    let n = decode_usize(&args[2]);
    if n > 8 || s.len() < n {
        return Err(ExecError::MalformedArgs(format!(
            "{}: scanned length {} exceeds the 8-byte packed-word contract or the string",
            target.id, n
        )));
    }
    // The ABI precondition: a NUL terminator lies within the packed window.
    if !s[..n].contains(&0) {
        return Err(ExecError::MalformedArgs(format!(
            "{}: no NUL terminator within the {}-byte bound",
            target.id, n
        )));
    }
    // The accept set travels as its own packed word. A C string set cannot contain
    // NUL, and an interior NUL would silently change the set, so it is rejected
    // rather than normalized.
    if accept.len() > 8 || accept.contains(&0) {
        return Err(ExecError::MalformedArgs(format!(
            "{}: the accept set must be at most 8 NUL-free bytes",
            target.id
        )));
    }
    let ws = pack_le_prefix(&s[..n]);
    let wa = pack_le_prefix(accept);
    let span = unsafe { crate::porting::exec::invoke(entry, ws, wa, n as u64) };
    Ok(encode_usize(span as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_be_left_aligns_bytes() {
        assert_eq!(pack_be(&[]), 0);
        assert_eq!(pack_be(&[0x61]), 0x6100_0000_0000_0000);
        assert_eq!(pack_be(&[0x01, 0x02]), 0x0102_0000_0000_0000);
        assert_eq!(pack_be(&[1, 2, 3, 4, 5, 6, 7, 8]), 0x0102_0304_0506_0708);
    }

    #[test]
    fn test_pack_le_prefix_loads_the_low_bytes() {
        assert_eq!(pack_le_prefix(&[]), 0);
        assert_eq!(pack_le_prefix(&[0x61]), 0x61);
        assert_eq!(pack_le_prefix(&[0x01, 0x02]), 0x0201);
        // Bytes past the eighth are never packed.
        assert_eq!(
            pack_le_prefix(&[1, 2, 3, 4, 5, 6, 7, 8, 9]),
            0x0807_0605_0403_0201
        );
    }
}
