// porting/target.rs — Port targets
//
// A port target is a single foreign API surface a court is run against. Its
// identity is qualified (`dialect:symbol:locale:type:version`) so behavior that
// depends on locale or ABI can never be silently conflated later. The first
// milestone target is libc `toupper` in the C locale, byte-in/byte-out.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A foreign API surface to be ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortTarget {
    /// Qualified identity, e.g. `libc:toupper:c-locale:u8:v1`.
    pub id: &'static str,
    pub dialect: &'static str,
    pub symbol: &'static str,
    pub version: &'static str,
    /// The locale the foreign behavior was observed under.
    pub locale_contract: &'static str,
    pub input_schema: &'static str,
    pub output_schema: &'static str,
    /// Clean-room candidate source bound into the seal (repo-relative).
    pub candidate_source: &'static str,
}

/// libc `toupper` in the C locale — the first JIT-porting court target.
///
/// The C locale matters: `toupper` is locale-dependent. This target names the
/// locale explicitly so a future locale-aware target is a *different* id, not a
/// silent redefinition of this one.
pub const LIBC_TOUPPER: PortTarget = PortTarget {
    id: "libc:toupper:c-locale:u8:v1",
    dialect: "libc",
    symbol: "toupper",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "u8 (single byte, 0..=255)",
    output_schema: "u8 (single byte)",
    candidate_source: "examples/jit_port_toupper.phor",
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

/// Resolve a symbol or qualified target id to a known port target.
pub fn resolve_target(name: &str) -> Option<PortTarget> {
    match name {
        "toupper" | "libc:toupper:c-locale:u8:v1" => Some(LIBC_TOUPPER),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_known_and_unknown() {
        assert_eq!(resolve_target("toupper"), Some(LIBC_TOUPPER));
        assert_eq!(
            resolve_target("libc:toupper:c-locale:u8:v1"),
            Some(LIBC_TOUPPER)
        );
        assert_eq!(resolve_target("memcmp"), None);
    }

    #[test]
    fn test_target_id_is_qualified() {
        assert_eq!(LIBC_TOUPPER.id, "libc:toupper:c-locale:u8:v1");
        assert_eq!(LIBC_TOUPPER.locale_contract, "C");
        assert_eq!(LIBC_TOUPPER.symbol, "toupper");
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
