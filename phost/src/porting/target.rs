// porting/target.rs — Port targets
//
// A port target is a single foreign API surface a court is run against:
// its dialect, symbol, version, and the input/output schemas the cage observes.
// The first milestone target is libc `toupper` (byte-in/byte-out).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A foreign API surface to be ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortTarget {
    pub dialect: &'static str,
    pub symbol: &'static str,
    pub version: &'static str,
    pub input_schema: &'static str,
    pub output_schema: &'static str,
}

/// libc `toupper` — the first JIT-porting court target.
pub const LIBC_TOUPPER: PortTarget = PortTarget {
    dialect: "libc",
    symbol: "toupper",
    version: "host-observed-v1",
    input_schema: "u8 (single byte, 0..=255)",
    output_schema: "u8 (single byte)",
};

/// One input case for a target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub case_id: String,
    pub input: Vec<u8>,
}

impl TestCase {
    /// A single-byte case; `case_id` is the lowercase-hex byte value.
    pub fn byte(value: u8) -> Self {
        Self {
            case_id: format!("0x{:02x}", value),
            input: alloc::vec![value],
        }
    }
}

/// The complete input domain for a byte-in/byte-out target: `0..=255`, in order.
///
/// The domain is small enough that the court runs it exhaustively rather than
/// sampling — there are no hand-picked cases in the verdict path.
pub fn byte_domain_cases() -> Vec<TestCase> {
    (0u16..=255).map(|v| TestCase::byte(v as u8)).collect()
}

/// Resolve a symbol name to a known port target.
pub fn resolve_target(symbol: &str) -> Option<PortTarget> {
    match symbol {
        "toupper" => Some(LIBC_TOUPPER),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_known_and_unknown() {
        assert_eq!(resolve_target("toupper"), Some(LIBC_TOUPPER));
        assert_eq!(resolve_target("memcmp"), None);
    }

    #[test]
    fn test_byte_domain_is_complete_and_ordered() {
        let cases = byte_domain_cases();
        assert_eq!(cases.len(), 256);
        for (i, c) in cases.iter().enumerate() {
            assert_eq!(c.case_id, format!("0x{:02x}", i));
            assert_eq!(c.input, alloc::vec![i as u8]);
        }
    }
}
