// porting/dialect_cage.rs — Foreign observation (the dialect cage)
//
// The cage is the clean-room boundary. It runs the *foreign* implementation as
// an observed black box and records input/output/status/locale/effects — it never
// reads, copies, or embeds foreign source. Foreign functions are reached through
// a single narrow FFI shim (libc `toupper`, `memcmp`, `memchr`, `strlen`,
// `strrchr`).
//
// This module is std-only: observing a foreign implementation requires the
// foreign runtime to be present. Kernel-side replay against already-sealed
// traces is a later phase and does not need this shim.

use alloc::string::String;
use alloc::vec::Vec;

use core::ffi::{c_char, c_int, c_void};

use crate::porting::candidate::{decode_usize, encode_index, encode_sign, encode_usize};
use crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR;
use crate::porting::composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR;
use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::{
    PortTarget, TestCase, LIBC_MEMCHR, LIBC_MEMCMP, LIBC_STRLEN, LIBC_STRRCHR, LIBC_TOUPPER,
};
use crate::porting::{PortError, PortingAuthority};

// Foreign implementations under observation. In the "C" locale (the initial
// locale for a process that never calls setlocale), `toupper` folds ASCII
// `a`..=`z` and leaves every other byte unchanged; `memcmp` compares unsigned
// bytes; `strlen` counts bytes up to the first NUL; `strrchr` finds the last
// occurrence of a byte in a NUL-terminated string. All are recorded with
// `locale_contract` so these courts can never be confused with locale-aware ones.
extern "C" {
    fn toupper(c: c_int) -> c_int;
    fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void;
    fn strlen(s: *const c_char) -> usize;
    fn strrchr(s: *const c_char, c: c_int) -> *mut c_char;
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

    if target.id == LIBC_TOUPPER.id {
        return Ok(cases
            .iter()
            .map(|case| {
                let input_byte = case
                    .args
                    .first()
                    .and_then(|a| a.first())
                    .copied()
                    .unwrap_or(0);
                let out = unsafe { toupper(input_byte as c_int) };
                OracleTrace::new(
                    target,
                    &case.case_id,
                    &case.args,
                    &[out as u8],
                    "ok",
                    &["compute"],
                )
            })
            .collect());
    }

    if target.id == LIBC_MEMCMP.id {
        return Ok(cases
            .iter()
            .map(|case| {
                let a = case.args.first().cloned().unwrap_or_default();
                let b = case.args.get(1).cloned().unwrap_or_default();
                let n = case.args.get(2).map(|x| decode_usize(x)).unwrap_or(0);
                // n is bounded by both buffers in the corpus; clamp defensively so
                // a malformed case can never read past its slice.
                let n = n.min(a.len()).min(b.len());
                let raw =
                    unsafe { memcmp(a.as_ptr() as *const c_void, b.as_ptr() as *const c_void, n) };
                let output = encode_sign(raw);
                OracleTrace::new(
                    target,
                    &case.case_id,
                    &case.args,
                    &output,
                    "ok",
                    &["compute"],
                )
            })
            .collect());
    }

    if target.id == LIBC_MEMCHR.id {
        return Ok(cases
            .iter()
            .map(|case| {
                let hay = case.args.first().cloned().unwrap_or_default();
                let needle = case
                    .args
                    .get(1)
                    .and_then(|a| a.first())
                    .copied()
                    .unwrap_or(0);
                let n = case.args.get(2).map(|x| decode_usize(x)).unwrap_or(0);
                // n is bounded by the haystack in the corpus; clamp defensively so
                // a malformed case can never read past its slice.
                let n = n.min(hay.len());
                let base = hay.as_ptr();
                let found = unsafe { memchr(base as *const c_void, needle as c_int, n) };
                // Normalize the pointer result to the index, or -1 when absent.
                // A pointer value is not portable behavior; the index is.
                let idx: i32 = if found.is_null() {
                    -1
                } else {
                    (found as usize - base as usize) as i32
                };
                OracleTrace::new(
                    target,
                    &case.case_id,
                    &case.args,
                    &encode_index(idx),
                    "ok",
                    &["compute"],
                )
            })
            .collect());
    }

    if target.id == LIBC_STRLEN.id {
        return Ok(cases
            .iter()
            .map(|case| {
                let buf = case.args.first().cloned().unwrap_or_default();
                // The ABI precondition is that a NUL terminator lies within the
                // first n bytes. libc `strlen` takes no bound, so observe the WHOLE
                // buffer: a terminator outside the window stays visible and is a
                // real mismatch, not a silently clamped observation.
                let mut c_buf = buf.clone();
                if !c_buf.contains(&0) {
                    // Defensive only: every corpus case has a terminator inside its
                    // bound, so this never fires for a sealed case. It exists so a
                    // hand-written case can never make the cage read out of bounds.
                    c_buf.push(0x00);
                }
                let len = unsafe { strlen(c_buf.as_ptr() as *const c_char) };
                OracleTrace::new(
                    target,
                    &case.case_id,
                    &case.args,
                    &encode_usize(len),
                    "ok",
                    &["compute"],
                )
            })
            .collect());
    }

    if target.id == LIBC_STRRCHR.id {
        return Ok(cases
            .iter()
            .map(|case| {
                let buf = case.args.first().cloned().unwrap_or_default();
                let needle = case
                    .args
                    .get(1)
                    .and_then(|a| a.first())
                    .copied()
                    .unwrap_or(0);
                // libc `strrchr` takes a C string, so observe the WHOLE buffer up
                // to its terminator (the ABI bound `n` only guarantees the
                // terminator is within the window; it is not a search bound).
                let mut c_buf = buf.clone();
                if !c_buf.contains(&0) {
                    // Defensive only: every corpus case has a terminator inside its
                    // bound, so this never fires for a sealed case.
                    c_buf.push(0x00);
                }
                let base = c_buf.as_ptr();
                let found = unsafe { strrchr(base as *const c_char, needle as c_int) };
                // Normalize the pointer result to the index, or -1 when absent. A
                // pointer value is not portable behavior; the index is.
                let idx: i32 = if found.is_null() {
                    -1
                } else {
                    (found as usize - base as usize) as i32
                };
                OracleTrace::new(
                    target,
                    &case.case_id,
                    &case.args,
                    &encode_index(idx),
                    "ok",
                    &["compute"],
                )
            })
            .collect());
    }

    Err(PortError::UnsupportedTarget(String::from(target.id)))
}

/// Observe the composition `toupper ∘ memchr` through the **foreign** runtime:
/// uppercase the haystack with libc `toupper` (C locale), uppercase the needle
/// with libc `toupper`, then run libc `memchr` over the normalized haystack.
///
/// This is the oracle the sealed composition must reproduce *without* any
/// foreign calls in the sealed path.
pub fn observe_composition(
    cases: &[TestCase],
    auth: &PortingAuthority,
) -> Result<Vec<OracleTrace>, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    Ok(cases
        .iter()
        .map(|case| {
            let hay = case.args.first().cloned().unwrap_or_default();
            let needle = case
                .args
                .get(1)
                .and_then(|a| a.first())
                .copied()
                .unwrap_or(0);
            let n = case.args.get(2).map(|x| decode_usize(x)).unwrap_or(0);
            let n = n.min(hay.len());

            // Stage 1+2: foreign C-locale `toupper` over the haystack and needle.
            let upper: Vec<u8> = hay
                .iter()
                .map(|&b| unsafe { toupper(b as c_int) as u8 })
                .collect();
            let upper_needle = unsafe { toupper(needle as c_int) as u8 };

            // Stage 3: foreign `memchr` over the normalized haystack.
            let base = upper.as_ptr();
            let found = unsafe { memchr(base as *const c_void, upper_needle as c_int, n) };
            let idx: i32 = if found.is_null() {
                -1
            } else {
                (found as usize - base as usize) as i32
            };

            OracleTrace::for_target_id(
                COMPOSITION_TOUPPER_MEMCHR.id,
                COMPOSITION_TOUPPER_MEMCHR.locale_contract,
                &case.case_id,
                &case.args,
                &encode_index(idx),
                "ok",
                &["compute"],
            )
        })
        .collect())
}

/// Observe the composition `toupper ∘ strlen ∘ memchr` through the **foreign**
/// runtime: uppercase the haystack with libc `toupper` (C locale), measure it with
/// libc `strlen` to derive `L`, uppercase the needle with libc `toupper`, then run
/// libc `memchr` over the folded haystack bounded by `L`.
///
/// The middle stage is the point: the oracle's search bound is the foreign
/// `strlen` result, not the case's `n`. The sealed chain must derive the same bound
/// from the sealed `strlen` object.
pub fn observe_composition_strlen_memchr(
    cases: &[TestCase],
    auth: &PortingAuthority,
) -> Result<Vec<OracleTrace>, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    Ok(cases
        .iter()
        .map(|case| {
            let hay = case.args.first().cloned().unwrap_or_default();
            let needle = case
                .args
                .get(1)
                .and_then(|a| a.first())
                .copied()
                .unwrap_or(0);

            // Stage 1: foreign C-locale `toupper` over the haystack.
            let mut folded: Vec<u8> = hay
                .iter()
                .map(|&b| unsafe { toupper(b as c_int) as u8 })
                .collect();
            // libc `strlen` needs a C string; every corpus case has a terminator
            // inside its bound, so this is defensive only.
            if !folded.contains(&0) {
                folded.push(0x00);
            }

            // Stage 2: foreign `strlen` of the folded haystack -> the derived bound.
            let derived = unsafe { strlen(folded.as_ptr() as *const c_char) };

            // Stage 3: foreign C-locale `toupper` of the needle.
            let folded_needle = unsafe { toupper(needle as c_int) as u8 };

            // Stage 4: foreign `memchr` over the folded haystack, bounded by the
            // derived length (never by the case's `n`).
            let base = folded.as_ptr();
            let found = unsafe { memchr(base as *const c_void, folded_needle as c_int, derived) };
            let idx: i32 = if found.is_null() {
                -1
            } else {
                (found as usize - base as usize) as i32
            };

            OracleTrace::for_target_id(
                COMPOSITION_TOUPPER_STRLEN_MEMCHR.id,
                COMPOSITION_TOUPPER_STRLEN_MEMCHR.locale_contract,
                &case.case_id,
                &case.args,
                &encode_index(idx),
                "ok",
                &["compute"],
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::memcmp_corpus;

    #[test]
    fn test_cage_observes_c_locale_toupper() {
        let target = LIBC_TOUPPER;
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
        assert!(traces.iter().all(|t| t.locale_contract == "C"));
    }

    #[test]
    fn test_cage_observes_unsigned_memcmp_ordering() {
        let target = LIBC_MEMCMP;
        let cases = alloc::vec![
            TestCase::new(
                "eq",
                alloc::vec![
                    b"abc".to_vec(),
                    b"abc".to_vec(),
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "lt",
                alloc::vec![
                    b"abc".to_vec(),
                    b"abd".to_vec(),
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "gt",
                alloc::vec![
                    b"abd".to_vec(),
                    b"abc".to_vec(),
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "n0",
                alloc::vec![
                    b"abc".to_vec(),
                    b"zzz".to_vec(),
                    0u64.to_le_bytes().to_vec()
                ]
            ),
            // Unsigned boundary: 0x80 > 0x7f.
            TestCase::new(
                "edge",
                alloc::vec![
                    alloc::vec![0x80],
                    alloc::vec![0x7f],
                    1u64.to_le_bytes().to_vec()
                ]
            ),
        ];
        let traces = observe_target(&target, &cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "00000000"); // equal
        assert_eq!(traces[1].output_hex, "ffffffff"); // less
        assert_eq!(traces[2].output_hex, "01000000"); // greater
        assert_eq!(traces[3].output_hex, "00000000"); // n = 0
        assert_eq!(traces[4].output_hex, "01000000"); // 0x80 > 0x7f (unsigned)
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_reports_memchr_index_not_pointer() {
        let target = LIBC_MEMCHR;
        let cases = alloc::vec![
            TestCase::new(
                "at0",
                alloc::vec![
                    b"abc".to_vec(),
                    alloc::vec![b'a'],
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "at2",
                alloc::vec![
                    b"abc".to_vec(),
                    alloc::vec![b'c'],
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "absent",
                alloc::vec![
                    b"abc".to_vec(),
                    alloc::vec![b'z'],
                    3u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "n0",
                alloc::vec![
                    b"abc".to_vec(),
                    alloc::vec![b'a'],
                    0u64.to_le_bytes().to_vec()
                ]
            ),
            TestCase::new(
                "edge",
                alloc::vec![
                    alloc::vec![0x00, 0x80],
                    alloc::vec![0x80],
                    2u64.to_le_bytes().to_vec()
                ]
            ),
        ];
        let traces = observe_target(&target, &cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "00000000"); // index 0
        assert_eq!(traces[1].output_hex, "02000000"); // index 2
        assert_eq!(traces[2].output_hex, "ffffffff"); // -1 (absent)
        assert_eq!(traces[3].output_hex, "ffffffff"); // n = 0
        assert_eq!(traces[4].output_hex, "01000000"); // unsigned 0x80 found at 1
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_observes_whole_memchr_corpus() {
        let traces = observe_target(
            &LIBC_MEMCHR,
            &crate::porting::target::memchr_corpus(),
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert!(!traces.is_empty());
        assert!(traces.iter().all(|t| t.is_intact()));
        assert!(traces.iter().all(|t| t.output_hex.len() == 8));
    }

    #[test]
    fn test_cage_reports_strlen_length_not_pointer() {
        let target = LIBC_STRLEN;
        let cases = alloc::vec![
            TestCase::new(
                "empty",
                alloc::vec![alloc::vec![0x00], 1u64.to_le_bytes().to_vec()],
            ),
            TestCase::new(
                "three",
                alloc::vec![
                    alloc::vec![0x61, 0x62, 0x63, 0x00],
                    4u64.to_le_bytes().to_vec(),
                ],
            ),
            TestCase::new(
                "first-nul-wins",
                alloc::vec![
                    alloc::vec![0x61, 0x00, 0x62, 0x00],
                    4u64.to_le_bytes().to_vec(),
                ],
            ),
            TestCase::new(
                "high-bytes-do-not-terminate",
                alloc::vec![
                    alloc::vec![0x7f, 0x80, 0xff, 0x00],
                    4u64.to_le_bytes().to_vec(),
                ],
            ),
        ];
        let traces = observe_target(&target, &cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "0000000000000000"); // length 0
        assert_eq!(traces[1].output_hex, "0300000000000000"); // length 3
        assert_eq!(traces[2].output_hex, "0100000000000000"); // first NUL wins -> 1
        assert_eq!(traces[3].output_hex, "0300000000000000"); // only 0x00 terminates
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_observes_whole_strlen_corpus() {
        let traces = observe_target(
            &LIBC_STRLEN,
            &crate::porting::target::strlen_corpus(),
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(traces.len(), 308);
        assert!(traces.iter().all(|t| t.is_intact()));
        assert!(traces.iter().all(|t| t.output_hex.len() == 16));
    }

    #[test]
    fn test_cage_reports_strrchr_last_index() {
        let target = LIBC_STRRCHR;
        let c = |b: &[u8]| b.to_vec();
        let cases = alloc::vec![
            // last of two 'b's
            TestCase::new(
                "last-b",
                alloc::vec![
                    c(b"ababc\0"),
                    alloc::vec![b'b'],
                    6u64.to_le_bytes().to_vec()
                ],
            ),
            // absent
            TestCase::new(
                "absent",
                alloc::vec![c(b"abc\0"), alloc::vec![b'z'], 4u64.to_le_bytes().to_vec()],
            ),
            // NUL needle -> terminator index (the length)
            TestCase::new(
                "term",
                alloc::vec![c(b"abc\0"), alloc::vec![0x00], 4u64.to_le_bytes().to_vec()],
            ),
            // only after the terminator -> absent
            TestCase::new(
                "tail-only",
                alloc::vec![
                    c(b"BC\0AAA"),
                    alloc::vec![b'A'],
                    6u64.to_le_bytes().to_vec(),
                ],
            ),
            // unsigned 0x80 at index 1, with a copy after the terminator
            TestCase::new(
                "edge",
                alloc::vec![
                    c(&[0x7f, 0x80, 0x00, 0x7f, 0x80]),
                    alloc::vec![0x80],
                    5u64.to_le_bytes().to_vec(),
                ],
            ),
        ];
        let traces = observe_target(&target, &cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "03000000"); // last 'b' at 3
        assert_eq!(traces[1].output_hex, "ffffffff"); // absent
        assert_eq!(traces[2].output_hex, "03000000"); // terminator at 3
        assert_eq!(traces[3].output_hex, "ffffffff"); // tail ignored
        assert_eq!(traces[4].output_hex, "01000000"); // 0x80 at 1, tail ignored
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_observes_whole_strrchr_corpus() {
        let traces = observe_target(
            &LIBC_STRRCHR,
            &crate::porting::target::strrchr_corpus(),
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(traces.len(), 336);
        assert!(traces.iter().all(|t| t.is_intact()));
        assert!(traces.iter().all(|t| t.output_hex.len() == 8));
    }

    #[test]
    fn test_cage_folded_search_derives_the_bound_from_strlen() {
        let mk = |id: &str, hay: &[u8], needle: u8, n: usize| {
            TestCase::new(
                id,
                alloc::vec![
                    hay.to_vec(),
                    alloc::vec![needle],
                    (n as u64).to_le_bytes().to_vec(),
                ],
            )
        };
        let cases = alloc::vec![
            // folded 'A' at index 0 of "aB"
            mk("fold", b"aB\0", 0x41, 4),
            // needle only after the terminator: the derived bound excludes it
            mk("tail", &[0x62, 0x00, 0x61, 0x61], 0x41, 4),
            // empty string: the derived bound is 0
            mk("empty", b"\0aaaa", 0x41, 5),
        ];
        let traces =
            observe_composition_strlen_memchr(&cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(traces[0].output_hex, "00000000"); // folded 'A' found at 0
        assert_eq!(traces[1].output_hex, "ffffffff"); // tail is outside the string
        assert_eq!(traces[2].output_hex, "ffffffff"); // no searchable byte
        assert!(traces
            .iter()
            .all(|t| t.target == COMPOSITION_TOUPPER_STRLEN_MEMCHR.id));
        assert!(traces.iter().all(|t| t.is_intact()));
    }

    #[test]
    fn test_cage_observes_whole_strlen_memchr_corpus() {
        let traces = observe_composition_strlen_memchr(
            &crate::porting::composition_strlen_memchr::composition_corpus(),
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(traces.len(), 350);
        assert!(traces.iter().all(|t| t.is_intact()));
        assert!(traces.iter().all(|t| t.output_hex.len() == 8));
    }

    #[test]
    fn test_cage_rejects_unknown_symbol() {
        let unknown = PortTarget {
            id: "libc:not_a_real_symbol:c-locale:u8:v1",
            dialect: "libc",
            symbol: "not_a_real_symbol",
            version: "v0",
            locale_contract: "C",
            input_schema: "u8",
            output_schema: "u8",
            domain_summary: "none",
            candidate_source: "none",
            abi_symbol: "not_a_real_symbol",
        };
        let err = observe_target(&unknown, &[TestCase::byte(0)], &PortingAuthority::granted());
        assert_eq!(
            err,
            Err(PortError::UnsupportedTarget(String::from(
                "libc:not_a_real_symbol:c-locale:u8:v1"
            )))
        );
    }

    #[test]
    fn test_cage_observes_whole_memcmp_corpus() {
        let traces =
            observe_target(&LIBC_MEMCMP, &memcmp_corpus(), &PortingAuthority::granted()).unwrap();
        assert_eq!(traces.len(), 312);
        assert!(traces.iter().all(|t| t.is_intact()));
        assert!(traces.iter().all(|t| t.output_hex.len() == 8));
    }
}
