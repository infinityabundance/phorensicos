// porting/target.rs — Port targets
//
// A port target is a single foreign API surface a court is run against. Its
// identity is qualified (`dialect:symbol:locale:contract:version`) so behavior
// that depends on locale or ABI can never be silently conflated later.
//
// Four targets exist:
//   - libc:toupper:c-locale:u8:v1     exhaustive byte domain, 256 cases
//   - libc:memcmp:c-locale:sign:v1    bounded deterministic corpus, ordering (312)
//   - libc:memchr:c-locale:index:v1   bounded deterministic corpus, first-match index (482)
//   - libc:strlen:c-locale:u64:v1     bounded deterministic corpus, NUL-terminated length (308)
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
    /// The symbol `phorc` emits for the promoted implementation's entry point.
    ///
    /// This is the deliberate ABI boundary the execution court loads and calls.
    /// `phorc` emits it as `_phor_<abi_symbol>`. It must be a leaf function with
    /// no calls and no relocations for the execution court to accept it.
    pub abi_symbol: &'static str,
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
    // ABI: (u64 byte) -> u64 folded byte, result in the low 8 bits.
    abi_symbol: "phor_toupper",
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
    // ABI: (wa: u64, wb: u64, n: u64) -> u64 sign, buffers packed big-endian.
    abi_symbol: "phor_memcmp_sign",
};

/// libc `memchr` — bounded deterministic corpus, index contract.
///
/// The observable is the **index of the first matching byte** (`0..n-1`) or
/// `-1` when the byte is absent. `memchr` returns a pointer, and a pointer value
/// is not portable behavior (it depends on the caller's buffer address), so the
/// court normalizes it to the index — which is the actual C contract
/// (`result - s`), and is what a native caller needs.
pub const LIBC_MEMCHR: PortTarget = PortTarget {
    id: "libc:memchr:c-locale:index:v1",
    dialect: "libc",
    symbol: "memchr",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[], u8 needle, usize n) — haystack, needle byte, searched length",
    output_schema: "i32 index of the first match, or -1 when absent",
    domain_summary: "bounded deterministic corpus: lengths 0..=8, first match at every index, absent needle, repeated needles, n-boundary, edge bytes, exhaustive 0..=255 needle sweep",
    candidate_source: "examples/jit_port_memchr.phor",
    // ABI: (wh: u64, needle: u64, n: u64) -> u64 index or -1; the haystack's
    // first n bytes are packed LITTLE-ENDIAN (byte i in bits 8*i).
    abi_symbol: "phor_memchr_index",
};

/// libc `strlen` — bounded deterministic corpus, NUL-terminated length contract.
///
/// The observable is the **length** of the C string: the index of the first NUL
/// byte. `strlen` takes no length argument, so the ABI carries `n` only as a
/// precondition bound: the terminator must lie within the first `n` bytes, which
/// is what makes the buffer packable into one word and keeps the foreign
/// observation from reading past the caller's window. A case whose terminator
/// falls outside the bound is out of contract and the native candidate fails
/// closed by returning `n`.
pub const LIBC_STRLEN: PortTarget = PortTarget {
    id: "libc:strlen:c-locale:u64:v1",
    dialect: "libc",
    symbol: "strlen",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[] bytes, usize n) — a buffer whose NUL terminator lies within the first n bytes",
    output_schema: "u64 length (index of the first NUL byte)",
    domain_summary: "bounded deterministic corpus: the complete (terminator-index k, scan-bound n) grid 0 <= k < n <= 8, non-NUL filler sweeps, ignored tails, and an exhaustive 0..=255 non-terminator sweep",
    candidate_source: "examples/jit_port_strlen.phor",
    // ABI: (w: u64, n: u64) -> u64 length; the first n bytes are packed
    // LITTLE-ENDIAN (byte i in bits 8*i). Returns n when no terminator is inside
    // the bound (fail closed).
    abi_symbol: "phor_strlen_len",
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

/// The `strlen` corpus — bounded, deterministic, exhaustive over its axes.
///
/// Axes: the complete `(k, n)` grid of terminator index `k` and scan bound `n`
/// with `0 <= k < n <= 8`; non-NUL filler bytes (including the unsigned edges
/// `0x7f`/`0x80`) at every prefix position; tail bytes after the terminator that
/// are themselves NUL (the FIRST NUL must win); buffers longer than the bound
/// (bytes past `n` must be ignored); and an exhaustive sweep of all 256 byte
/// values proving that only `0x00` terminates.
///
/// Every case has exactly one NUL inside the scanned window, so libc `strlen` is
/// well defined and never reads beyond it.
pub fn strlen_corpus() -> Vec<TestCase> {
    let mut c: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: &str, buf: &[u8], n: usize| {
        cases.push(TestCase::new(
            id,
            vec![buf.to_vec(), (n as u64).to_le_bytes().to_vec()],
        ));
    };

    // Non-NUL filler bytes used before a terminator, spanning the unsigned
    // boundary at 0x7f/0x80 so high bytes cannot be mistaken for a terminator.
    const FILL: [u8; 8] = [0x01, 0x7f, 0x80, 0xfe, 0xff, 0x41, 0x61, 0x02];

    // A — the complete (k, n) domain: terminator at index k, scan bound n.
    for k in 0..=7usize {
        for n in (k + 1)..=8usize {
            let mut buf = alloc::vec![0xffu8; n];
            buf[..k].copy_from_slice(&FILL[..k]);
            buf[k] = 0x00;
            // Tail bytes past the terminator alternate 0x00/0xff, so a later NUL
            // is present and the first NUL must win.
            for i in (k + 1)..n {
                buf[i] = if (i - k) % 2 == 1 { 0x00 } else { 0xff };
            }
            mk(&mut c, &format!("A.{}.{}", k, n), &buf, n);
        }
    }

    // B — a non-NUL filler byte at every prefix position: only 0x00 terminates.
    for &e in &[0x01u8, 0x02, 0x41, 0x7f, 0x80, 0xfe, 0xff] {
        mk(
            &mut c,
            &format!("B.{:02x}", e),
            &[e, e, e, e, 0x00, 0xff, 0xff, 0xff],
            8,
        );
    }

    // B2 — the empty string with a high-byte tail after its terminator.
    mk(
        &mut c,
        "B2.empty.high",
        &[0x00, 0x80, 0xff, 0xfe, 0x7f, 0x01, 0x00, 0xff],
        8,
    );

    // C — the scan bound n is only a bound: the buffer extends past it, and the
    //     terminator is inside the bound. Bytes past n must be ignored entirely.
    for k in 0..=7usize {
        let mut buf = alloc::vec![0xffu8; 8];
        buf[..k].copy_from_slice(&FILL[..k]);
        buf[k] = 0x00;
        mk(&mut c, &format!("C.{}", k), &buf, k + 1);
    }

    // D — exhaustive sweep of every byte value: exactly one of them terminates.
    //     [x, 0x00] has length 1 for x != 0 and length 0 for x == 0.
    for x in 0..=255u16 {
        mk(&mut c, &format!("D.{:02x}", x), &[x as u8, 0x00], 2);
    }

    c
}

/// The `memchr` corpus — bounded, deterministic, exhaustive over its axes.
///
/// Axes: lengths `0..=8`; the first match at every index (distinct patterns);
/// repeated needles (first occurrence wins); an absent needle; the `n`-boundary
/// around a match (`n = j` excludes it, `n = j+1` includes it); the unsigned edge
/// bytes `00/01/7f/80/fe/ff`; and an exhaustive sweep of all 256 needle values.
pub fn memchr_corpus() -> Vec<TestCase> {
    let mut c: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: &str, hay: &[u8], needle: u8, n: usize| {
        cases.push(TestCase::new(
            id,
            vec![
                hay.to_vec(),
                alloc::vec![needle],
                (n as u64).to_le_bytes().to_vec(),
            ],
        ));
    };

    // A — n = 0: absent regardless of the haystack and needle.
    mk(&mut c, "A.000", &[], 0x00, 0);
    mk(&mut c, "A.001", &[0xff], 0xff, 0);
    mk(&mut c, "A.002", &[0x61, 0x62], 0x61, 0);
    mk(&mut c, "A.003", &[0x00, 0x01, 0x02], 0x01, 0);

    // B — absent needle (0x5a is not in zero/ones/asc/desc/alt for len <= 8).
    for len in 0..=8usize {
        for p in PATTERNS {
            let v = p.bytes(len);
            mk(&mut c, &format!("B.{}.{}", p.name(), len), &v, 0x5a, len);
        }
    }

    // C — first match at index i, on patterns with distinct bytes (asc/desc),
    //     so the expected index is exactly i.
    for len in 1..=8usize {
        for p in [Pattern::Asc, Pattern::Desc] {
            let v = p.bytes(len);
            for i in 0..len {
                mk(
                    &mut c,
                    &format!("C.{}.{}.{}", p.name(), len, i),
                    &v,
                    v[i],
                    len,
                );
            }
        }
    }

    // C2 — repeated bytes: the FIRST occurrence wins (index 0).
    for len in 1..=8usize {
        mk(
            &mut c,
            &format!("C2.zero.{}", len),
            &Pattern::Zero.bytes(len),
            0x00,
            len,
        );
        mk(
            &mut c,
            &format!("C2.ones.{}", len),
            &Pattern::Ones.bytes(len),
            0xff,
            len,
        );
        mk(
            &mut c,
            &format!("C2.alt.{}", len),
            &Pattern::Alt.bytes(len),
            0x00,
            len,
        );
        mk(
            &mut c,
            &format!("C2.alt2.{}", len),
            &Pattern::Alt.bytes(len),
            0xff,
            1.min(len),
        );
    }

    // D — the n boundary around a match at index j:
    //     n = j excludes it (absent); n = j+1 includes it (index j).
    for len in 1..=8usize {
        let v = Pattern::Asc.bytes(len);
        for j in 0..len {
            if j > 0 {
                mk(&mut c, &format!("D.{}.{}.n{}", len, j, j), &v, v[j], j);
            }
            mk(
                &mut c,
                &format!("D.{}.{}.n{}", len, j, j + 1),
                &v,
                v[j],
                j + 1,
            );
        }
    }

    // E — the unsigned edge bytes plus ASCII, each at a known index.
    const SWEEP: [u8; 8] = [0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff, 0x41, 0x61];
    for &b in SWEEP.iter() {
        mk(&mut c, &format!("E.{:02x}", b), &SWEEP, b, SWEEP.len());
    }
    mk(&mut c, "E.absent", &SWEEP, 0x5a, SWEEP.len());

    // F — exhaustive needle sweep: all 256 needle values against a fixed
    //     haystack containing the unsigned edge bytes and two ASCII letters.
    for needle in 0..=255u16 {
        mk(
            &mut c,
            &format!("F.{:02x}", needle),
            &SWEEP,
            needle as u8,
            SWEEP.len(),
        );
    }

    c
}

/// The deterministic case set for a target.
pub fn cases_for(target: &PortTarget) -> Vec<TestCase> {
    match target.id {
        id if id == LIBC_TOUPPER.id => byte_domain_cases(),
        id if id == LIBC_MEMCMP.id => memcmp_corpus(),
        id if id == LIBC_MEMCHR.id => memchr_corpus(),
        id if id == LIBC_STRLEN.id => strlen_corpus(),
        _ => Vec::new(),
    }
}

/// Resolve a symbol or qualified target id to a known port target.
pub fn resolve_target(name: &str) -> Option<PortTarget> {
    match name {
        "toupper" | "libc:toupper:c-locale:u8:v1" => Some(LIBC_TOUPPER),
        "memcmp" | "libc:memcmp:c-locale:sign:v1" => Some(LIBC_MEMCMP),
        "memchr" | "libc:memchr:c-locale:index:v1" => Some(LIBC_MEMCHR),
        "strlen" | "libc:strlen:c-locale:u64:v1" => Some(LIBC_STRLEN),
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
        assert_eq!(resolve_target("memchr"), Some(LIBC_MEMCHR));
        assert_eq!(resolve_target("strlen"), Some(LIBC_STRLEN));
        assert_eq!(
            resolve_target("libc:memcmp:c-locale:sign:v1"),
            Some(LIBC_MEMCMP)
        );
        assert_eq!(
            resolve_target("libc:memchr:c-locale:index:v1"),
            Some(LIBC_MEMCHR)
        );
        assert_eq!(
            resolve_target("libc:strlen:c-locale:u64:v1"),
            Some(LIBC_STRLEN)
        );
        assert_eq!(resolve_target("strcspn"), None);
    }

    #[test]
    fn test_target_ids_are_qualified() {
        assert_eq!(LIBC_TOUPPER.id, "libc:toupper:c-locale:u8:v1");
        assert_eq!(LIBC_MEMCMP.id, "libc:memcmp:c-locale:sign:v1");
        assert_eq!(LIBC_MEMCHR.id, "libc:memchr:c-locale:index:v1");
        assert_eq!(LIBC_STRLEN.id, "libc:strlen:c-locale:u64:v1");
        assert_eq!(LIBC_MEMCMP.locale_contract, "C");
        assert_eq!(LIBC_MEMCHR.locale_contract, "C");
        assert_eq!(LIBC_STRLEN.locale_contract, "C");
    }

    #[test]
    fn test_strlen_corpus_is_deterministic_and_bounded() {
        let a = strlen_corpus();
        let b = strlen_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 308);

        // Case ids are unique.
        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is well formed: 2 args; an 8-byte scan bound; n <= len(buf);
        // n within the packed-word contract; and a NUL terminator inside the
        // bound (the ABI precondition without which the court is out of contract).
        for c in &a {
            assert_eq!(c.args.len(), 2, "case {} args", c.case_id);
            let n = u64::from_le_bytes(c.args[1].as_slice().try_into().unwrap()) as usize;
            assert!(n <= c.args[0].len(), "case {}", c.case_id);
            assert!(n <= 8, "case {}", c.case_id);
            assert!(c.args[0].len() <= 8, "case {}", c.case_id);
            assert!(
                c.args[0][..n].contains(&0x00),
                "case {} has no terminator inside its bound",
                c.case_id
            );
        }
    }

    #[test]
    fn test_strlen_corpus_covers_required_axes() {
        let cases = strlen_corpus();
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();

        for group in ["A.", "B.", "B2.", "C.", "D."] {
            assert!(
                ids.iter().any(|id| id.starts_with(group)),
                "strlen corpus is missing group {}",
                group
            );
        }

        // The complete (k, n) grid is present: 36 cases with k < n <= 8.
        assert_eq!(ids.iter().filter(|id| id.starts_with("A.")).count(), 36);
        // The empty string and the longest 8-byte-window string are present.
        assert!(ids.contains(&"A.0.1"));
        assert!(ids.contains(&"A.7.8"));
        // The exhaustive 256-value non-terminator sweep is present.
        assert_eq!(ids.iter().filter(|id| id.starts_with("D.")).count(), 256);
        assert!(ids.contains(&"D.00"));
        assert!(ids.contains(&"D.7f"));
        assert!(ids.contains(&"D.80"));
        assert!(ids.contains(&"D.ff"));
    }

    #[test]
    fn test_memchr_corpus_is_deterministic_and_bounded() {
        let a = memchr_corpus();
        let b = memchr_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 482);

        // Case ids are unique.
        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is well formed: 3 args; a 1-byte needle; an 8-byte length;
        // n <= len(haystack); and n within the packed-word contract.
        for c in &a {
            assert_eq!(c.args.len(), 3, "case {} args", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {} needle", c.case_id);
            let n = u64::from_le_bytes(c.args[2].as_slice().try_into().unwrap()) as usize;
            assert!(n <= c.args[0].len(), "case {}", c.case_id);
            assert!(n <= 8, "case {}", c.case_id);
            assert!(c.args[0].len() <= 8, "case {}", c.case_id);
        }
    }

    #[test]
    fn test_memchr_corpus_covers_required_axes() {
        let cases = memchr_corpus();
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();

        for group in ["A.", "B.", "C.", "C2.", "D.", "E.", "F."] {
            assert!(
                ids.iter().any(|id| id.starts_with(group)),
                "memchr corpus is missing group {}",
                group
            );
        }

        // n = 0 is exercised; the absent needle is exercised; the n boundary is
        // exercised; the first match at index 0 on a repeated pattern is
        // exercised; and the exhaustive 256-value needle sweep is present.
        assert!(ids.contains(&"A.000"));
        assert!(ids.contains(&"B.zero.0"));
        assert!(ids.contains(&"D.4.2.n2"));
        assert!(ids.contains(&"C2.ones.3"));
        assert_eq!(ids.iter().filter(|id| id.starts_with("F.")).count(), 256);

        // The exhaustive sweep covers the unsigned edge bytes at known indices.
        for id in ["F.00", "F.01", "F.7f", "F.80", "F.fe", "F.ff"] {
            assert!(ids.contains(&id), "memchr sweep is missing {}", id);
        }
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
