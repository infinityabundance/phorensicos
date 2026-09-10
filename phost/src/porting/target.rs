// porting/target.rs — Port targets
//
// A port target is a single foreign API surface a court is run against. Its
// identity is qualified (`dialect:symbol:locale:contract:version`) so behavior
// that depends on locale or ABI can never be silently conflated later.
//
// Two targets exist:
//   - libc:toupper:c-locale:u8:v1     exhaustive byte domain, 256 cases
//   - libc:memcmp:c-locale:sign:v1    bounded deterministic corpus, ordering
//
// The corpus for a target is deterministic and bounded; it is enumerated in a
// fixed order so the court is reproducible and the verdict does not depend on
// sampling.

use alloc::format;
use alloc::string::String;
use alloc::vec;
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
    /// Short description of the case set (used in the behavior signature).
    pub domain_summary: &'static str,
    /// Clean-room candidate source bound into the seal (repo-relative).
    pub candidate_source: &'static str,
}

/// libc `toupper` in the C locale — exhaustive byte domain.
pub const LIBC_TOUPPER: PortTarget = PortTarget {
    id: "libc:toupper:c-locale:u8:v1",
    dialect: "libc",
    symbol: "toupper",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "u8 (single byte, 0..=255)",
    output_schema: "u8 (single byte)",
    domain_summary: "exhaustive 0..=255",
    candidate_source: "examples/jit_port_toupper.phor",
};

/// libc `memcmp` — bounded deterministic two-buffer corpus, ordering contract.
///
/// The observable is the **sign** of the return value (`<0`, `0`, `>0`). The
/// exact integer is not part of the C contract, so the target id says `sign`.
pub const LIBC_MEMCMP: PortTarget = PortTarget {
    id: "libc:memcmp:c-locale:sign:v1",
    dialect: "libc",
    symbol: "memcmp",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[], u8[], usize) — two buffers and a compared length",
    output_schema: "i32 sign (-1 | 0 | 1)",
    domain_summary: "bounded deterministic corpus: lengths 0..=8, 5 patterns, every mismatch position, n-boundary, unsigned edge bytes",
    candidate_source: "examples/jit_port_memcmp.phor",
};

/// One input case for a target: an ordered list of byte-slice arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub case_id: String,
    pub args: Vec<Vec<u8>>,
}

impl TestCase {
    pub fn new(case_id: impl Into<String>, args: Vec<Vec<u8>>) -> Self {
        Self {
            case_id: case_id.into(),
            args,
        }
    }

    /// A single-byte case; `case_id` is the lowercase-hex byte value.
    pub fn byte(value: u8) -> Self {
        Self {
            case_id: format!("0x{:02x}", value),
            args: vec![vec![value]],
        }
    }
}

/// The complete input domain for a byte-in/byte-out target: `0..=255`, in order.
pub fn byte_domain_cases() -> Vec<TestCase> {
    (0u16..=255).map(|v| TestCase::byte(v as u8)).collect()
}

// ============================================================================
// memcmp corpus — bounded, deterministic, exhaustive over its axes
// ============================================================================
//
// Axes (per the review): lengths 0..=8; patterns zero/ones/ascending/
// descending/alternating; every first-mismatch position; both orderings; the
// n-boundary around a mismatch; and the unsigned edge bytes 00/01/7f/80/fe/ff.

/// Byte patterns used to build corpus buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Zero,
    Ones,
    Asc,
    Desc,
    Alt,
}

const PATTERNS: [Pattern; 5] = [
    Pattern::Zero,
    Pattern::Ones,
    Pattern::Asc,
    Pattern::Desc,
    Pattern::Alt,
];

impl Pattern {
    fn name(&self) -> &'static str {
        match self {
            Pattern::Zero => "zero",
            Pattern::Ones => "ones",
            Pattern::Asc => "asc",
            Pattern::Desc => "desc",
            Pattern::Alt => "alt",
        }
    }

    fn bytes(&self, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| match self {
                Pattern::Zero => 0x00,
                Pattern::Ones => 0xff,
                Pattern::Asc => (i & 0xff) as u8,
                Pattern::Desc => ((len - 1 - i) & 0xff) as u8,
                Pattern::Alt => {
                    if i % 2 == 0 {
                        0x00
                    } else {
                        0xff
                    }
                }
            })
            .collect()
    }
}

/// Unsigned edge bytes, spanning the signed/unsigned boundary at 0x7f/0x80.
const EDGE_BYTES: [u8; 6] = [0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff];

fn mk(cases: &mut Vec<TestCase>, id: &str, a: &[u8], b: &[u8], n: usize) {
    cases.push(TestCase::new(
        id,
        vec![a.to_vec(), b.to_vec(), (n as u64).to_le_bytes().to_vec()],
    ));
}

/// The bounded deterministic memcmp corpus.
///
/// Every case's length argument satisfies `n <= min(a.len(), b.len())`, so no
/// observation reads past a buffer.
pub fn memcmp_corpus() -> Vec<TestCase> {
    let mut c: Vec<TestCase> = Vec::new();

    // A — zero length: n = 0 is Equal regardless of the buffers.
    mk(&mut c, "A.000", &[], &[], 0);
    mk(&mut c, "A.001", &[], &[0xff], 0);
    mk(&mut c, "A.002", &[0xff], &[], 0);
    mk(&mut c, "A.003", &[0x00, 0x01], &[0xff, 0xfe], 0);

    // B — identical buffers: Equal for every length and pattern, n = len.
    for len in 0..=8usize {
        for p in PATTERNS {
            let v = p.bytes(len);
            mk(&mut c, &format!("B.{}.{}", p.name(), len), &v, &v, len);
        }
    }

    // C — first mismatch at index i, n = len, both directions.
    for len in 1..=8usize {
        let base = Pattern::Asc.bytes(len);
        for i in 0..len {
            if base[i] < 0xff {
                let mut b = base.clone();
                b[i] = base[i] + 1; // a < b at i
                mk(&mut c, &format!("C.{}.{}.lt", len, i), &base, &b, len);
            }
            if base[i] > 0x00 {
                let mut b = base.clone();
                b[i] = base[i] - 1; // a > b at i
                mk(&mut c, &format!("C.{}.{}.gt", len, i), &base, &b, len);
            }
        }
    }

    // D — shorter equal prefix, n = the shorter length (Equal).
    for len in 0..=8usize {
        let a = Pattern::Asc.bytes(len);
        let mut b = a.clone();
        b.push(0xAA);
        mk(&mut c, &format!("D.{}.prefix", len), &a, &b, len);
    }

    // E — the n boundary around a mismatch at index j:
    //     n = j excludes it (Equal); n = j+1 includes it (ordering).
    for len in 1..=8usize {
        let base = Pattern::Asc.bytes(len);
        for j in 0..len {
            let mut b = base.clone();
            b[j] = if base[j] < 0xff {
                base[j] + 1
            } else {
                base[j] - 1
            };
            if j > 0 {
                mk(&mut c, &format!("E.{}.{}.n{}", len, j, j), &base, &b, j);
            }
            mk(
                &mut c,
                &format!("E.{}.{}.n{}", len, j, j + 1),
                &base,
                &b,
                j + 1,
            );
        }
    }

    // F — one-byte edge pairs: every unsigned ordering, including 0x7f/0x80.
    for x in EDGE_BYTES {
        for y in EDGE_BYTES {
            mk(&mut c, &format!("F.{:02x}.{:02x}", x, y), &[x], &[y], 1);
        }
    }

    // G — distinct equal-length patterns, n = len.
    for len in 0..=8usize {
        for i in 0..PATTERNS.len() {
            for k in (i + 1)..PATTERNS.len() {
                let a = PATTERNS[i].bytes(len);
                let b = PATTERNS[k].bytes(len);
                mk(
                    &mut c,
                    &format!("G.{}.{}.{}", PATTERNS[i].name(), PATTERNS[k].name(), len),
                    &a,
                    &b,
                    len,
                );
            }
        }
    }

    c
}

/// The deterministic case set for a target.
pub fn cases_for(target: &PortTarget) -> Vec<TestCase> {
    match target.id {
        id if id == LIBC_TOUPPER.id => byte_domain_cases(),
        id if id == LIBC_MEMCMP.id => memcmp_corpus(),
        _ => Vec::new(),
    }
}

/// Resolve a symbol or qualified target id to a known port target.
pub fn resolve_target(name: &str) -> Option<PortTarget> {
    match name {
        "toupper" | "libc:toupper:c-locale:u8:v1" => Some(LIBC_TOUPPER),
        "memcmp" | "libc:memcmp:c-locale:sign:v1" => Some(LIBC_MEMCMP),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_known_and_unknown() {
        assert_eq!(resolve_target("toupper"), Some(LIBC_TOUPPER));
        assert_eq!(resolve_target("memcmp"), Some(LIBC_MEMCMP));
        assert_eq!(
            resolve_target("libc:memcmp:c-locale:sign:v1"),
            Some(LIBC_MEMCMP)
        );
        assert_eq!(resolve_target("strlen"), None);
    }

    #[test]
    fn test_target_ids_are_qualified() {
        assert_eq!(LIBC_TOUPPER.id, "libc:toupper:c-locale:u8:v1");
        assert_eq!(LIBC_MEMCMP.id, "libc:memcmp:c-locale:sign:v1");
        assert_eq!(LIBC_MEMCMP.locale_contract, "C");
    }

    #[test]
    fn test_byte_domain_is_complete_and_ordered() {
        let cases = byte_domain_cases();
        assert_eq!(cases.len(), 256);
        for (i, c) in cases.iter().enumerate() {
            assert_eq!(c.case_id, format!("0x{:02x}", i));
            assert_eq!(c.args, vec![vec![i as u8]]);
        }
    }

    #[test]
    fn test_memcmp_corpus_is_deterministic_and_bounded() {
        let a = memcmp_corpus();
        let b = memcmp_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 312);

        // Case ids are unique.
        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is well formed: 3 args, n <= min(len(a), len(b)).
        for c in &a {
            assert_eq!(c.args.len(), 3, "case {} args", c.case_id);
            let n = u64::from_le_bytes(c.args[2].as_slice().try_into().unwrap()) as usize;
            assert!(
                n <= c.args[0].len().min(c.args[1].len()),
                "case {}",
                c.case_id
            );
        }
    }

    #[test]
    fn test_memcmp_corpus_covers_required_axes() {
        let cases = memcmp_corpus();
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();

        // Zero-length comparison is exercised.
        assert!(ids.iter().any(|id| id.starts_with("A.")));
        // Both orderings at a mismatch are exercised.
        assert!(ids.iter().any(|id| id.ends_with(".lt")));
        assert!(ids.iter().any(|id| id.ends_with(".gt")));
        // The n boundary is exercised.
        assert!(ids.iter().any(|id| id.starts_with("E.")));
        // The 0x7f/0x80 unsigned edge is exercised.
        assert!(ids.iter().any(|id| *id == "F.7f.80"));
        assert!(ids.iter().any(|id| *id == "F.80.7f"));

        // All five patterns appear in the distinct-pattern group (a pattern may
        // only ever be the first or second member of a pair, so match anywhere).
        for p in PATTERNS {
            assert!(ids
                .iter()
                .any(|id| id.starts_with("G.") && id.contains(p.name())));
        }
    }
}
