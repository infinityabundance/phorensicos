// porting/target.rs — Port targets
//
// A port target is a single foreign API surface a court is run against. Its
// identity is qualified (`dialect:symbol:locale:contract:version`) so behavior
// that depends on locale or ABI can never be silently conflated later.
//
// Six surfaces are sealed in the bootstrap store; a seventh record is the
// corrected successor of `strspn` (see the provenance correction below).
//   - libc:toupper:c-locale:u8:v1     exhaustive byte domain, 256 cases
//   - libc:memcmp:c-locale:sign:v1    bounded deterministic corpus, ordering (312)
//   - libc:memchr:c-locale:index:v1   bounded deterministic corpus, first-match index (482)
//   - libc:strlen:c-locale:u64:v1     bounded deterministic corpus, NUL-terminated length (308)
//   - libc:strrchr:c-locale:index:v1  bounded deterministic corpus, last-match index (336)
//   - posix:strspn:c-locale:u64:v1    bounded deterministic corpus, set-membership span (558)
//   - libc:strspn:c-locale:u64:v1     the corrected successor of the above
//
// The dialect names the *specification the contract is drawn from*. `strspn` is
// an ISO C surface (C89 and later), so its corrected identity is `libc:`; the
// historical `posix:` record carries a mistaken provenance claim and is preserved
// rather than rewritten. See docs/DIALECT_QUALIFICATION.md and
// docs/CONTRACT_PROVENANCE_MIGRATION.md.
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

/// libc `strrchr` — bounded deterministic corpus, last-match index contract.
///
/// The observable is the **index of the last occurrence** of the needle in the C
/// string, or `-1` when it is absent. `strrchr` returns a pointer, and a pointer
/// value is not portable behavior, so the court normalizes it to the index — the
/// actual C contract (`result - s`). Unlike `memchr`, the search domain is the
/// *string*: it ends at (and includes) the first NUL, so bytes after the
/// terminator are never matched, and a needle of `0` yields the terminator index
/// (the string's length). `n` is the ABI precondition bound: the terminator lies
/// within the first `n` bytes, which makes the buffer packable into one
/// zero-extended word.
pub const LIBC_STRRCHR: PortTarget = PortTarget {
    id: "libc:strrchr:c-locale:index:v1",
    dialect: "libc",
    symbol: "strrchr",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[] bytes, u8 needle, usize n) — a buffer whose NUL terminator lies within the first n bytes, a needle byte, and the bound",
    output_schema: "i32 index of the last match, or -1 when absent",
    domain_summary: "bounded deterministic corpus: unique and repeated occurrences (last wins), the empty string, needles that occur only after the terminator, buffers longer than the string, the unsigned edge bytes, and an exhaustive 0..=255 needle sweep",
    candidate_source: "examples/jit_port_strrchr.phor",
    // ABI: (w: u64, needle: u64) -> u64 index or -1; the first n bytes are packed
    // LITTLE-ENDIAN (byte i in bits 8*i) and zero-extended. The packed word must
    // contain a NUL terminator (the harness enforces n).
    abi_symbol: "phor_strrchr_index",
};

/// The **historical** record for `strspn`, kept verbatim.
///
/// This record carries a **mistaken provenance claim**: it asserts that ISO C does
/// not specify `strspn`. That is false — `strspn` is specified by ISO C (C89 and
/// every later edition), and POSIX states its `strspn` specification is aligned
/// with and defers to ISO C. The seal built from this record binds the *observed
/// behavior*, which is correct; only the dialect/provenance metadata is wrong.
///
/// The record is **preserved as historical evidence** rather than rewritten. Its
/// successor is [`LIBC_STRSPN`], issued through an explicit contract-provenance
/// correction (`docs/CONTRACT_PROVENANCE_MIGRATION.md`).
///
/// The observable is the **length of the initial segment of `s` consisting only of
/// bytes in `accept`** — a prefix length decided by set membership. `accept` is a C
/// string, so it can never contain NUL; the terminator is therefore always a byte
/// outside the set, which is what ends the span.
///
/// `n` is the ABI precondition bound: the terminator of `s` lies within its first
/// `n` bytes, so the foreign observation never reads past the caller's window and
/// the scan fits one packed word.
pub const POSIX_STRSPN: PortTarget = PortTarget {
    id: "posix:strspn:c-locale:u64:v1",
    dialect: "posix",
    symbol: "strspn",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[] s, u8[] accept, usize n) — a buffer whose NUL terminator lies within the first n bytes, a NUL-free accept set, and the bound",
    output_schema: "u64 span length (index of the first byte not in accept; the terminator is never in accept)",
    domain_summary: "bounded deterministic corpus: the complete (span, n) grid with the whole prefix in the set, empty set and empty string, a stop byte before the terminator, every set size 1..=8, a disjoint set, the unsigned edge bytes, and two exhaustive 0..=255 sweeps (the accepted byte value and the stopping byte value)",
    candidate_source: "examples/jit_port_strspn.phor",
    // ABI: (ws: u64, wa: u64, n: u64) -> u64 span length; the first n bytes of `s`
    // and the whole accept set are packed LITTLE-ENDIAN (byte i in bits 8*i).
    // Unused accept lanes are zero and are inert because membership requires a
    // non-NUL byte.
    abi_symbol: "phor_strspn_len",
};

/// The **successor** record for `strspn`, with corrected contract provenance.
///
/// `strspn` is an ISO C surface (C89 and later), so the dialect is `libc`, not
/// `posix`; POSIX defers to ISO C for it. This successor is issued through an
/// explicit correction of [`POSIX_STRSPN`]'s mistaken provenance: the historical
/// record and its evidence stay intact, and this successor is requalified and
/// resealed on its own (`docs/CONTRACT_PROVENANCE_MIGRATION.md`). The observed
/// implementation is still the host C library — the cage is the boundary — and the
/// seal binds the observed behavior.
pub const LIBC_STRSPN: PortTarget = PortTarget {
    id: "libc:strspn:c-locale:u64:v1",
    dialect: "libc",
    symbol: "strspn",
    version: "host-observed-v1",
    locale_contract: "C",
    input_schema: "(u8[] s, u8[] accept, usize n) — a buffer whose NUL terminator lies within the first n bytes, a NUL-free accept set, and the bound",
    output_schema: "u64 span length (index of the first byte not in accept; the terminator is never in accept)",
    domain_summary: "bounded deterministic corpus: the complete (span, n) grid with the whole prefix in the set, empty set and empty string, a stop byte before the terminator, every set size 1..=8, a disjoint set, the unsigned edge bytes, and two exhaustive 0..=255 sweeps (the accepted byte value and the stopping byte value)",
    candidate_source: "examples/jit_port_strspn.phor",
    abi_symbol: "phor_strspn_len",
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

/// The `strrchr` corpus — bounded, deterministic, exhaustive over its axes.
///
/// Axes: unique occurrences at every in-string index; repeated occurrences so the
/// *last* wins; the empty string; needles that occur only after the terminator
/// (which must never match); needles that occur both before and after it (the
/// in-string occurrence wins); an exhaustive sweep of all 256 needle values
/// against a haystack whose tail repeats bytes from the string; and the unsigned
/// edge bytes `00/01/7f/80/fe/ff` with copies of `0x7f`/`0x80` after the
/// terminator, so the terminator — not the byte value — decides.
///
/// Every case satisfies `n <= len(buf)`, `n <= 8`, `len(buf) <= 8` and has a NUL
/// terminator inside `buf[..n]`, so libc `strrchr` is well defined and the packed
/// word is a faithful encoding of the string.
pub fn strrchr_corpus() -> Vec<TestCase> {
    let mut c: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: &str, buf: &[u8], needle: u8, n: usize| {
        cases.push(TestCase::new(
            id,
            vec![
                buf.to_vec(),
                alloc::vec![needle],
                (n as u64).to_le_bytes().to_vec(),
            ],
        ));
    };

    // Distinct non-NUL content bytes (none is 0x00, 0xff or 0x41-used-as-tail).
    const S: [u8; 7] = [0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47];
    // Alternating pair for the repeated-occurrence group.
    const ALT: [u8; 2] = [0x41, 0x42];

    // A — the empty string (terminator at index 0): a NUL needle yields 0, any
    //     other needle occurs only in the tail and so is absent (-1).
    for n in 1..=8usize {
        let mut buf = alloc::vec![0x41u8; n];
        buf[0] = 0x00;
        mk(&mut c, &format!("A.term.n{}", n), &buf, 0x00, n);
        mk(&mut c, &format!("A.miss.n{}", n), &buf, 0x41, n);
    }

    // B — a single occurrence at index k of a string of length L (k < L),
    //     terminator at L, tail 0xff. Distinct content bytes, so it is unique.
    for l in 1..=7usize {
        for k in 0..l {
            let mut buf = alloc::vec![0xffu8; 8];
            buf[..l].copy_from_slice(&S[..l]);
            buf[l] = 0x00;
            mk(&mut c, &format!("B.{}.{}", l, k), &buf, S[k], 8);
        }
    }

    // C — repeated needle: the LAST in-string occurrence wins.
    // C1 — alternating content.
    for l in 1..=7usize {
        let mut buf = alloc::vec![0xffu8; 8];
        for i in 0..l {
            buf[i] = ALT[i % 2];
        }
        buf[l] = 0x00;
        mk(&mut c, &format!("C.alt.{}.a", l), &buf, 0x41, 8);
        mk(&mut c, &format!("C.alt.{}.b", l), &buf, 0x42, 8);
    }
    // C2 — one repeated byte.
    for l in 1..=7usize {
        let mut buf = alloc::vec![0xffu8; 8];
        for i in 0..l {
            buf[i] = 0x41;
        }
        buf[l] = 0x00;
        mk(&mut c, &format!("C.rep.{}", l), &buf, 0x41, 8);
    }

    // D — the terminator is respected.
    // D1 — the needle occurs ONLY after the terminator: -1.
    for l in 0..=4usize {
        let mut buf = alloc::vec![0x00u8; l + 4];
        for i in 0..l {
            buf[i] = [0x42u8, 0x43, 0x44, 0x45][i];
        }
        buf[l] = 0x00;
        buf[l + 1] = 0x41;
        buf[l + 2] = 0x41;
        buf[l + 3] = 0x41;
        mk(&mut c, &format!("D.tail.{}", l), &buf, 0x41, buf.len());
    }
    // D2 — the needle occurs both before and after the terminator: the last
    //      occurrence inside the string wins (the tail is ignored).
    for p in 0..=2usize {
        let mut buf = alloc::vec![0x00u8; p + 3];
        for i in 0..p {
            buf[i] = [0x42u8, 0x43][i];
        }
        buf[p] = 0x46;
        buf[p + 1] = 0x00;
        buf[p + 2] = 0x46;
        mk(&mut c, &format!("D.both.{}", p), &buf, 0x46, buf.len());
    }

    // E — exhaustive needle sweep against a fixed haystack whose tail repeats
    //     bytes from the string: only in-string occurrences may match.
    //     string = "AB" (terminator at 2); tail = [0x41, 0x42, 0x00, 0x7f, 0x80].
    const E_BUF: [u8; 8] = [0x41, 0x42, 0x00, 0x41, 0x42, 0x00, 0x7f, 0x80];
    for needle in 0..=255u16 {
        mk(
            &mut c,
            &format!("E.{:02x}", needle),
            &E_BUF,
            needle as u8,
            8,
        );
    }

    // F — the unsigned edge bytes at known indices, with copies of 0x7f/0x80
    //     after the terminator so the terminator, not the byte value, decides.
    const F_BUF: [u8; 8] = [0x01, 0x7f, 0x80, 0xfe, 0xff, 0x00, 0x7f, 0x80];
    for &b in &[0x00u8, 0x01, 0x7f, 0x80, 0xfe, 0xff, 0x5a] {
        mk(&mut c, &format!("F.{:02x}", b), &F_BUF, b, 8);
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

/// The `posix:strspn` corpus.
///
/// Every case is `(s, accept, n)` with the terminator of `s` inside the first `n`
/// bytes, `accept` free of NUL, and both buffers at most 8 bytes (one packed word).
pub fn strspn_corpus() -> Vec<TestCase> {
    let mut c: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: &str, s: &[u8], accept: &[u8], n: usize| {
        cases.push(TestCase::new(
            id,
            vec![
                s.to_vec(),
                accept.to_vec(),
                (n as u64).to_le_bytes().to_vec(),
            ],
        ));
    };

    // Distinct non-NUL bytes that span the unsigned boundary at 0x7f/0x80, so a
    // high byte can never be mistaken for a terminator or for padding.
    const FILL: [u8; 8] = [0x01, 0x7f, 0x80, 0xfe, 0xff, 0x41, 0x61, 0x02];
    const ABSENT: u8 = 0x5a; // 'Z', in none of the sets below

    // A — the complete (span, n) grid: the whole prefix is in the set, so the span
    //     is the terminator index. `n` is only a bound; the tail past the
    //     terminator repeats accepted bytes, so a scan that ignored the terminator
    //     would report a longer span.
    for k in 0..=7usize {
        for n in (k + 1)..=8usize {
            let mut s = alloc::vec![0xffu8; n];
            s[..k].copy_from_slice(&FILL[..k]);
            s[k] = 0x00;
            for i in (k + 1)..n {
                // Accepted bytes after the terminator: the span must still stop at k.
                s[i] = FILL[(i - k) % 8];
            }
            mk(&mut c, &format!("A.{}.{}", k, n), &s, &FILL[..k], n);
        }
    }

    // B — the empty set and the empty string: both spans are 0.
    mk(&mut c, "B.empty_set", &[0x61, 0x62, 0x00, 0xff], &[], 4);
    mk(
        &mut c,
        "B.empty_string",
        &[0x00, 0x61, 0x62, 0xff],
        &FILL[..3],
        4,
    );
    mk(&mut c, "B.empty_both", &[0x00, 0xff, 0xff, 0xff], &[], 4);

    // C — a stop byte before the terminator: the span ends at the first byte
    //     outside the set, not at the terminator. `ABSENT` is in no set below.
    for k in 0..=6usize {
        let mut s = alloc::vec![0xffu8; 8];
        s[..k].copy_from_slice(&FILL[..k]);
        s[k] = ABSENT;
        for i in (k + 1)..8 {
            s[i] = if i == 7 { 0x00 } else { FILL[i - k] };
        }
        // A set that excludes ABSENT but includes everything before it.
        mk(&mut c, &format!("C.{}", k), &s, &FILL[..k], 8);
    }

    // D — every set size 1..=8 with the matching prefix: membership is a *set*
    //     test, so a candidate that compared a single byte must fail here.
    for m in 1..=8usize {
        let mut s = alloc::vec![0x00u8; 8];
        s[..m].copy_from_slice(&FILL[..m]);
        s[m.min(7)] = 0x00;
        mk(&mut c, &format!("D.{}", m), &s, &FILL[..m], 8);
    }

    // E — a disjoint set: nothing matches, so the span is 0.
    for m in 1..=4usize {
        let mut s = alloc::vec![0xffu8; 8];
        s[..m].copy_from_slice(&FILL[..m]);
        s[m] = 0x00;
        mk(
            &mut c,
            &format!("E.{}", m),
            &s,
            &[0x30, 0x31, 0x32, 0x33],
            m + 1,
        );
    }

    // F — the terminator is the only stop and sits at the bound: the span equals n.
    for n in 1..=8usize {
        let mut s = alloc::vec![0xffu8; 8];
        for i in 0..(n - 1) {
            s[i] = FILL[i];
        }
        s[n - 1] = 0x00;
        mk(&mut c, &format!("F.bound.{}", n), &s, &FILL, n);
    }

    // G — exhaustive sweep of the accepted byte value: `s = [x, x, NUL]` with the
    //     one-byte set `{x}` has span 2 for every non-NUL x, and span 0 for x = 0
    //     (the set is empty, and the first byte terminates).
    for x in 0..=255u16 {
        let x = x as u8;
        let accept: &[u8] = if x == 0 { &[] } else { &[x] };
        mk(&mut c, &format!("G.{:02x}", x), &[x, x, 0x00], accept, 3);
    }

    // H — exhaustive sweep of the stopping byte value: `s = [x, 'A', NUL]` with the
    //     set `{'A'}` has span 0 for x = 0, span 1 for x not in the set, and span 2
    //     for x = 'A'.
    for x in 0..=255u16 {
        let x = x as u8;
        mk(
            &mut c,
            &format!("H.{:02x}", x),
            &[x, 0x41, 0x00],
            &[0x41],
            3,
        );
    }

    c
}

/// The deterministic case set for a target.
///
/// Registry-driven: the generator is an extension point named by the target's
/// registered extension, so a new target never edits this function.
pub fn cases_for(target: &PortTarget) -> Vec<TestCase> {
    crate::porting::registry::extension_for(target.id)
        .map(|e| (e.cases)())
        .unwrap_or_default()
}

/// Resolve a symbol or qualified target id to a known port target.
///
/// Registry-driven: the name table is the extension registry, not a `match`.
pub fn resolve_target(name: &str) -> Option<PortTarget> {
    crate::porting::registry::extension_by_name(name).map(|e| e.target)
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
        assert_eq!(resolve_target("strrchr"), Some(LIBC_STRRCHR));
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
        assert_eq!(
            resolve_target("libc:strrchr:c-locale:index:v1"),
            Some(LIBC_STRRCHR)
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
        assert_eq!(LIBC_STRRCHR.locale_contract, "C");
    }

    #[test]
    fn test_strrchr_corpus_is_deterministic_and_bounded() {
        let a = strrchr_corpus();
        let b = strrchr_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 336);

        // Case ids are unique.
        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is well formed: 3 args; a 1-byte needle; an 8-byte bound;
        // n <= len(buf) <= 8; and a NUL terminator inside buf[..n], so libc
        // `strrchr` is well defined and the packed word encodes the string.
        for c in &a {
            assert_eq!(c.args.len(), 3, "case {} args", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {} needle", c.case_id);
            let n = u64::from_le_bytes(c.args[2].as_slice().try_into().unwrap()) as usize;
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
    fn test_strrchr_corpus_covers_required_axes() {
        let cases = strrchr_corpus();
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();

        for group in ["A.", "B.", "C.", "D.", "E.", "F."] {
            assert!(
                ids.iter().any(|id| id.starts_with(group)),
                "strrchr corpus is missing group {}",
                group
            );
        }

        // The empty string, a full-length unique occurrence, a repeated needle,
        // and the terminator-respecting groups are present.
        assert!(ids.contains(&"A.term.n1"));
        assert!(ids.contains(&"A.miss.n8"));
        assert!(ids.contains(&"B.7.6"));
        assert!(ids.contains(&"C.alt.7.a"));
        assert!(ids.contains(&"C.rep.7"));
        assert!(ids.contains(&"D.tail.4"));
        assert!(ids.contains(&"D.both.2"));
        // The exhaustive 256-value needle sweep is present, including the
        // unsigned edge bytes.
        assert_eq!(ids.iter().filter(|id| id.starts_with("E.")).count(), 256);
        assert!(ids.contains(&"E.00"));
        assert!(ids.contains(&"E.7f"));
        assert!(ids.contains(&"E.80"));
        assert!(ids.contains(&"F.7f"));
        assert!(ids.contains(&"F.80"));
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
    fn test_strspn_corpus_is_deterministic_and_bounded() {
        let a = strspn_corpus();
        let b = strspn_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 578);

        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len(), "case ids are unique");

        for c in &a {
            assert_eq!(c.args.len(), 3, "case {}", c.case_id);
            let s = &c.args[0];
            let accept = &c.args[1];
            let n = u64::from_le_bytes(c.args[2].as_slice().try_into().unwrap()) as usize;
            assert!(s.len() <= 8, "case {}", c.case_id);
            assert!(accept.len() <= 8, "case {}", c.case_id);
            assert!(n <= 8 && n <= s.len(), "case {}", c.case_id);
            // The precondition: the terminator is inside the bound, so the foreign
            // observation cannot read past the caller's window and the span is
            // always decided within one packed word.
            assert!(
                s[..n].contains(&0),
                "case {} has no terminator inside its bound",
                c.case_id
            );
            // A C string set cannot contain NUL; an interior NUL would silently
            // change the set, so the corpus never encodes one.
            assert!(
                !accept.contains(&0),
                "case {} has a NUL inside the accept set",
                c.case_id
            );
        }
    }

    #[test]
    fn test_strspn_corpus_covers_required_axes() {
        let cases = strspn_corpus();
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();

        for group in ["A.", "B.", "C.", "D.", "E.", "F.", "G.", "H."] {
            assert!(
                ids.iter().any(|id| id.starts_with(group)),
                "strspn corpus is missing group {}",
                group
            );
        }

        // The complete (span, n) grid, with accepted bytes after the terminator.
        assert_eq!(ids.iter().filter(|id| id.starts_with("A.")).count(), 36);
        assert!(ids.contains(&"A.0.1"));
        assert!(ids.contains(&"A.7.8"));
        // The degenerate cases: an empty set and an empty string both span 0.
        assert!(ids.contains(&"B.empty_set"));
        assert!(ids.contains(&"B.empty_string"));
        // Every set size 1..=8, so a single-byte compare cannot pass.
        assert_eq!(ids.iter().filter(|id| id.starts_with("D.")).count(), 8);
        assert!(ids.contains(&"D.8"));
        // Both exhaustive sweeps: the accepted byte and the stopping byte.
        assert_eq!(ids.iter().filter(|id| id.starts_with("G.")).count(), 256);
        assert_eq!(ids.iter().filter(|id| id.starts_with("H.")).count(), 256);
        assert!(ids.contains(&"G.00"));
        assert!(ids.contains(&"G.41"));
        assert!(ids.contains(&"H.7f"));
        assert!(ids.contains(&"H.80"));
        assert!(ids.contains(&"H.ff"));
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
