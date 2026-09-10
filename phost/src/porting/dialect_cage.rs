// porting/dialect_cage.rs — Foreign observation (the dialect cage)
//
// The cage is the clean-room boundary. It runs the *foreign* implementation as
// an observed black box and records input/output/status/effects — it never
// reads, copies, or embeds foreign source. For the first milestone the foreign
// surface is host libc `toupper`, reached through a single narrow FFI shim.
//
// This module is std-only: observing a foreign implementation requires the
// foreign runtime to be present. Kernel-side replay against already-sealed
// traces is a later phase and does not need this shim.

use alloc::string::String;
use alloc::vec::Vec;

use core::ffi::c_int;

use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::{PortTarget, TestCase};
use crate::porting::{PortError, PortingAuthority};

// Foreign implementation under observation. In the "C" locale (the initial
// locale for a process that never calls setlocale), `toupper` folds ASCII
// `a`..=`z` and leaves every other byte unchanged, which is exactly what the
// exhaustive 0..=255 court verifies.
extern "C" {
    fn toupper(c: c_int) -> c_int;
}

/// Observe `cases` against `target` through the cage.
///
/// Requires the `PORTING` capability: there is no ambient authority to observe
/// a foreign implementation.
pub fn observe_target(
    target: &PortTarget,
    cases: &[TestCase],
    auth: &PortingAuthority,
) -> Result<Vec<OracleTrace>, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    match target.symbol {
        "toupper" => Ok(cases
            .iter()
            .map(|case| {
                let input_byte = *case.input.first().unwrap_or(&0);
                let out = unsafe { toupper(input_byte as c_int) };
                OracleTrace::new(
                    target.symbol,
                    &case.case_id,
                    &case.input,
                    &[out as u8],
                    "ok",
                    &["compute"],
                )
            })
            .collect()),
        other => Err(PortError::UnsupportedTarget(String::from(other))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target;

    #[test]
    fn test_cage_observes_c_locale_toupper() {
        let target = target::LIBC_TOUPPER;
        let cases = alloc::vec![
            TestCase::byte(b'a'),
            TestCase::byte(b'z'),
            TestCase::byte(b'A'),
            TestCase::byte(b'0'),
            TestCase::byte(0xe9),
        ];
        let traces = observe_target(&target, &cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "41"); // a -> A
        assert_eq!(traces[1].output_hex, "5a"); // z -> Z
        assert_eq!(traces[2].output_hex, "41"); // A -> A
        assert_eq!(traces[3].output_hex, "30"); // 0 -> 0
        assert_eq!(traces[4].output_hex, "e9"); // non-ASCII untouched (C locale)
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_rejects_unknown_symbol() {
        let unknown = PortTarget {
            dialect: "libc",
            symbol: "not_a_real_symbol",
            version: "v0",
            input_schema: "u8",
            output_schema: "u8",
        };
        let err = observe_target(&unknown, &[TestCase::byte(0)], &PortingAuthority::granted());
        assert_eq!(
            err,
            Err(PortError::UnsupportedTarget(String::from(
                "not_a_real_symbol"
            )))
        );
    }
}
