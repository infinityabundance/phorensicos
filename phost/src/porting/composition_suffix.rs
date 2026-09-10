// porting/composition_suffix.rs — Sealed Slice Dispatch Court
//
// The first five compositions derive a **scalar**: `strlen` yields a length used
// as a search bound, and the pair chain shares that bound between two searches.
// The bound narrows how far a search looks, but every stage still reads the same
// buffer from the same origin.
//
// This chain adds the missing shape: a derived value that selects a **buffer**.
//
//   phor:compose:toupper_memchr_suffix:c-locale:index:v1
//
//   input:  haystack, needleA, needleB, n
//   oracle: foreign C-locale `toupper` over the haystack (-> H'),
//           foreign `memchr` of H' for folded needleA, bounded by n (-> i),
//           and only if that matched, foreign `memchr` of H'[i..] for folded
//           needleB, bounded by n - i (-> j), reporting i + j or -1.
//   sealed: fold with the sealed toupper, search with the sealed memchr, **slice
//           the folded haystack at the derived index**, search the suffix with the
//           sealed memchr again.
//
// Two things are new, and the corpus is built to make both observable:
//
//   1. **The slice is load-bearing.** The suffix search starts at `i`, so a needleB
//      that occurs *before* `i` but not after must return -1. A chain that kept the
//      caller's window (the first pass's shape) finds that earlier occurrence and
//      returns a smaller index — a wrong answer, not a rounding difference.
//   2. **A stage is data-dependently skipped.** When needleA is absent there is no
//      suffix and no origin, so needleB's search is never dispatched. That is
//      counted separately (`memchr_b_not_reached_cases`) rather than being dressed
//      up as a "native" run of a stage that never happened.
//
// The observable is the **absolute** index into the folded haystack (`i + j`), not
// the suffix-relative one, so the chain's arithmetic is part of what is verified.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize, encode_index};
use crate::porting::composition::{CompositionTarget, Stage};
use crate::porting::dispatch::{DispatchError, DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The sixth composition: fold, find, **slice**, find again.
pub const COMPOSITION_TOUPPER_MEMCHR_SUFFIX: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_memchr_suffix:c-locale:index:v1",
    locale_contract: "C",
    stages: &[
        "libc:toupper:c-locale:u8:v1",
        "libc:memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needleA, u8 needleB, usize n)",
    output_schema: "i32 absolute index of the first C-locale-folded match of needleB in the folded haystack sliced at needleA's first match, or -1",
    domain_summary: "a slice corpus: needleB before the derived origin (must not match), needleB in the suffix, needleA absent (the second search is never dispatched), needleA at index 0, identical needles, the n-boundary, both exhaustive 0..=255 needle sweeps, and the unsigned edge bytes",
};

// ============================================================================
// Composition corpus
// ============================================================================

/// The sixth composition's corpus.
///
/// Content bytes are distinct (`a` and `b`) so the derived origin `i` and the
/// suffix offset `j` are exactly the indexes under test. The `B` group is the
/// discriminator: `b` sits at index 0 and never in the suffix, so a chain that
/// searched the caller's window instead of the slice would report index 0.
pub fn composition_corpus() -> Vec<TestCase> {
    let mut cases: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: String, hay: &[u8], a: u8, b: u8, n: usize| {
        cases.push(TestCase::new(
            id,
            alloc::vec![
                hay.to_vec(),
                alloc::vec![a],
                alloc::vec![b],
                (n as u64).to_le_bytes().to_vec(),
            ],
        ));
    };

    const A: u8 = 0x61; // 'a' -> 'A'
    const B: u8 = 0x62; // 'b' -> 'B'
    const PAD: u8 = 0xff; // not a letter: `toupper` leaves it unchanged

    // A — the slice is load-bearing: `b` at index 0, `a` at `i`, `b` at `j > i`.
    //     The suffix search must skip the `b` at 0 and report `i + (j - i) = j`.
    for len in 3..=8usize {
        for i in 1..len {
            for j in (i + 1)..len {
                let mut buf = alloc::vec![PAD; len];
                buf[0] = B;
                buf[i] = A;
                buf[j] = B;
                mk(
                    &mut cases,
                    format!("A.slice.{}.{}.{}", len, i, j),
                    &buf,
                    A,
                    B,
                    len,
                );
            }
        }
    }

    // B — `b` before the origin only: the suffix contains no match, so the answer
    //     is -1 even though the window as a whole does match. This is the case a
    //     bound-only chain cannot pass.
    for len in 2..=8usize {
        for i in 1..len {
            let mut buf = alloc::vec![PAD; len];
            buf[0] = B;
            buf[i] = A;
            mk(
                &mut cases,
                format!("B.before.{}.{}", len, i),
                &buf,
                A,
                B,
                len,
            );
        }
    }

    // C — needleA absent: there is no origin, so needleB's search is never
    //     dispatched, and the observable is -1.
    for len in 1..=8usize {
        let mut buf = alloc::vec![PAD; len];
        buf[len - 1] = B;
        mk(&mut cases, format!("C.absent.{}", len), &buf, A, B, len);
    }

    // D — needleA at index 0: the suffix is the whole window.
    for len in 2..=8usize {
        for k in 1..len {
            let mut buf = alloc::vec![PAD; len];
            buf[0] = A;
            buf[k] = B;
            mk(&mut cases, format!("D.zero.{}.{}", len, k), &buf, A, B, len);
        }
    }

    // E — both needles identical: the suffix starts with a match, so `j = 0` and
    //     the answer is the origin itself.
    for len in 1..=8usize {
        for i in 0..len {
            let mut buf = alloc::vec![PAD; len];
            buf[i] = A;
            mk(&mut cases, format!("E.same.{}.{}", len, i), &buf, A, A, len);
        }
    }

    // F — the n-boundary at both ends: the origin at the last byte (suffix length
    //     one), and a suffix match exactly at the last byte.
    for len in 2..=8usize {
        let mut head = alloc::vec![PAD; len];
        head[0] = B;
        head[len - 1] = A;
        mk(
            &mut cases,
            format!("F.origin_last.{}", len),
            &head,
            A,
            B,
            len,
        );

        let mut tail = alloc::vec![PAD; len];
        tail[0] = B;
        tail[len - 2] = A;
        tail[len - 1] = B;
        mk(
            &mut cases,
            format!("F.suffix_last.{}", len),
            &tail,
            A,
            B,
            len,
        );
    }

    // G — exhaustive sweep of needleA. A fixed haystack whose `a` is at 2 and whose
    //     `b` is at 0 and 3, so the sweep exercises the absent path (no suffix), the
    //     slice path, and the unfoldable edge bytes as origins.
    const G_BUF: [u8; 8] = [0x62, 0x7f, 0x61, 0x62, 0x80, 0xff, 0x63, 0x64];
    for needle in 0..=255u16 {
        mk(
            &mut cases,
            format!("G.{:02x}", needle),
            &G_BUF,
            needle as u8,
            B,
            8,
        );
    }

    // H — exhaustive sweep of needleB against a fixed origin at 2. `b` occurs at 0
    //     and never in the suffix, so the sweep pins the slice for every byte value.
    const H_BUF: [u8; 8] = [0x62, 0x78, 0x61, 0x63, 0x7f, 0x80, 0xff, 0x78];
    for needle in 0..=255u16 {
        mk(
            &mut cases,
            format!("H.{:02x}", needle),
            &H_BUF,
            A,
            needle as u8,
            8,
        );
    }

    // I — case folding and the unsigned edge bytes: the needles are supplied in
    //     both cases and the haystack carries both, plus bytes `toupper` cannot fold.
    const I_BUF: [u8; 6] = [0x62, 0x41, 0x7f, 0x80, 0x42, 0x61]; // b A 7f 80 B a
    for (a, b) in [
        (0x61u8, 0x62u8), // a/A -> origin 1, B in the suffix
        (0x41, 0x42),     // A/B -> same origin and match, supplied uppercase
        (0x7f, 0x80),     // unfoldable bytes as origin and needle
        (0x80, 0x7f),     // reversed
        (0x62, 0x61),     // origin at 0 (b), needle 'a' -> suffix ends with A
        (0x00, 0x01),     // NUL is a byte like any other here
    ] {
        mk(
            &mut cases,
            format!("I.{:02x}.{:02x}", a, b),
            &I_BUF,
            a,
            b,
            6,
        );
    }

    cases
}

// ============================================================================
// The chain
// ============================================================================

/// One composed call: the normalized haystack, the derived origin, the suffix
/// offset and the absolute result.
#[derive(Clone, Debug)]
struct ChainOutcome {
    /// The absolute index into the folded haystack, or -1.
    index: Option<i32>,
    /// The derived origin (-1 when needleA is absent).
    origin: Option<i32>,
    /// The suffix-relative offset, when the second search actually ran.
    suffix_index: Option<i32>,
    hay: Stage,
    needle_a: Stage,
    memchr_a: Stage,
    needle_b: Stage,
    /// `None` when the second search was never dispatched (no origin).
    memchr_b: Option<Stage>,
    dispatches: u64,
    hay_norm: Vec<u8>,
    needle_a_norm: Option<u8>,
    needle_b_norm: Option<u8>,
}

/// Run the sealed chain for one case.
///
/// Every stage goes through `NativeDispatcher`; the Rust mirrors are never called.
/// The suffix is produced by **slicing the folded haystack at the sealed memchr's
/// result**, and that slice — not the caller's window — is what the second search
/// receives.
fn run_chain(
    dispatcher: &mut NativeDispatcher,
    hay: &[u8],
    needle_a: u8,
    needle_b: u8,
    n: usize,
    auth: &PortingAuthority,
) -> ChainOutcome {
    let mut out = ChainOutcome {
        index: None,
        origin: None,
        suffix_index: None,
        hay: Stage::Native,
        needle_a: Stage::Native,
        memchr_a: Stage::Native,
        needle_b: Stage::Native,
        memchr_b: None,
        dispatches: 0,
        hay_norm: Vec::with_capacity(hay.len()),
        needle_a_norm: None,
        needle_b_norm: None,
    };

    // Every remaining stage is unreachable once a link fails; mark them all.
    let fail_all = |out: &mut ChainOutcome, stage: Stage| {
        out.hay = stage;
        out.needle_a = stage;
        out.memchr_a = stage;
        out.needle_b = stage;
        out.memchr_b = Some(stage);
    };

    // Stage 1 — fold every haystack byte with the sealed toupper.
    for &b in hay {
        match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![b]], auth) {
            Ok(o) if o.source == DispatchSource::SealedObject => {
                out.dispatches += 1;
                out.hay_norm.push(o.output.first().copied().unwrap_or(b));
            }
            Ok(_) => {
                fail_all(&mut out, Stage::Fallback);
                return out;
            }
            Err(_) => {
                fail_all(&mut out, Stage::Broken);
                return out;
            }
        }
    }

    // Stage 2 — fold needleA.
    match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![needle_a]], auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_a_norm = Some(o.output.first().copied().unwrap_or(needle_a));
        }
        Ok(_) => {
            out.needle_a = Stage::Fallback;
            out.memchr_a = Stage::Fallback;
            out.needle_b = Stage::Fallback;
            out.memchr_b = Some(Stage::Fallback);
            return out;
        }
        Err(_) => {
            out.needle_a = Stage::Broken;
            out.memchr_a = Stage::Broken;
            out.needle_b = Stage::Broken;
            out.memchr_b = Some(Stage::Broken);
            return out;
        }
    }

    // Stage 3 — fold needleB. Folded before the search so the fold's own status is
    // observed even when the suffix search turns out not to be reached.
    match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![needle_b]], auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_b_norm = Some(o.output.first().copied().unwrap_or(needle_b));
        }
        Ok(_) => {
            out.needle_b = Stage::Fallback;
            out.memchr_b = Some(Stage::Fallback);
            return out;
        }
        Err(_) => {
            out.needle_b = Stage::Broken;
            out.memchr_b = Some(Stage::Broken);
            return out;
        }
    }

    // Stage 4 — search the folded window for folded needleA with the sealed memchr.
    // This derives the **origin** for the next stage.
    let folded_a = out.needle_a_norm.unwrap_or(needle_a);
    let window = n.min(out.hay_norm.len());
    let args_a = alloc::vec![
        out.hay_norm.clone(),
        alloc::vec![folded_a],
        (window as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_MEMCHR, &args_a, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.origin = Some(decode_index(&o.output));
        }
        Ok(_) => {
            out.memchr_a = Stage::Fallback;
            out.needle_b = Stage::Fallback;
            out.memchr_b = Some(Stage::Fallback);
            return out;
        }
        Err(_) => {
            out.memchr_a = Stage::Broken;
            out.needle_b = Stage::Broken;
            out.memchr_b = Some(Stage::Broken);
            return out;
        }
    }

    let origin = out.origin.unwrap_or(-1);
    if origin < 0 {
        // No origin: the suffix does not exist, so the second search is not
        // dispatched at all. The observable is -1.
        out.index = Some(-1);
        return out;
    }

    // Stage 5 — **the slice**. A derived index selects the buffer the next stage
    // reads; the suffix ends where the caller's window ended (`window - origin`).
    let origin_u = origin as usize;
    let suffix_len = window.saturating_sub(origin_u);
    if suffix_len == 0 {
        // Defensive: a memchr result is always inside the window, so this cannot
        // happen for a correct sealed memchr. Fail closed rather than slice.
        out.memchr_b = Some(Stage::Broken);
        return out;
    }
    let suffix = out.hay_norm[origin_u..origin_u + suffix_len].to_vec();

    // Stage 6 — search the SUFFIX for folded needleB, bounded by the suffix length.
    let folded_b = out.needle_b_norm.unwrap_or(needle_b);
    let args_b = alloc::vec![
        suffix,
        alloc::vec![folded_b],
        (suffix_len as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_MEMCHR, &args_b, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            let j = decode_index(&o.output);
            out.memchr_b = Some(Stage::Native);
            out.suffix_index = Some(j);
            out.index = Some(if j < 0 { -1 } else { origin + j });
        }
        Ok(_) => out.memchr_b = Some(Stage::Fallback),
        Err(_) => out.memchr_b = Some(Stage::Broken),
    }

    out
}

/// Run this composition as a **stage of another chain** (or from a call site):
/// `(haystack, needleA, needleB, n) -> i32 absolute index` (little-endian).
///
/// Every stage that ran must have been served by a sealed object; a fallback or a
/// broken seal inside the chain is a broken seal, never a silent foreign fallback.
pub fn run_chain_encoded(
    dispatcher: &mut NativeDispatcher,
    args: &[Vec<u8>],
    auth: &PortingAuthority,
) -> Result<Vec<u8>, DispatchError> {
    let hay = args.first().cloned().unwrap_or_default();
    let needle_a = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
    let needle_b = args.get(2).and_then(|a| a.first()).copied().unwrap_or(0);
    let n = args.get(3).map(|x| decode_usize(x)).unwrap_or(0);
    let out = run_chain(dispatcher, &hay, needle_a, needle_b, n, auth);

    if out.hay != Stage::Native
        || out.needle_a != Stage::Native
        || out.memchr_a != Stage::Native
        || out.needle_b != Stage::Native
        || out.memchr_b != Some(Stage::Native)
    {
        return Err(DispatchError::SealBroken(format!(
            "{}: a stage was not served by a sealed object",
            COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id
        )));
    }
    match out.index {
        Some(i) => Ok(encode_index(i)),
        None => Err(DispatchError::SealBroken(format!(
            "{}: the chain produced no observable",
            COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id
        ))),
    }
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuffixVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed objects the chain actually dispatched to.
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    pub cases_run: u64,
    /// Cases where the haystack fold was served by the sealed object.
    pub toupper_hay_native_cases: u64,
    /// Cases where the needleA fold was served by the sealed object.
    pub toupper_needle_a_native_cases: u64,
    /// Cases where the origin-searching memchr was served by the sealed object.
    pub memchr_a_native_cases: u64,
    /// Cases where the needleB fold was served by the sealed object.
    pub toupper_needle_b_native_cases: u64,
    /// Cases where the suffix search ran and was served by the sealed object.
    pub memchr_b_native_cases: u64,
    /// Cases where needleA was absent, so no suffix existed and the suffix search
    /// was never dispatched. Counted here — not as a native run of a stage that
    /// did not happen.
    pub memchr_b_not_reached_cases: u64,
    /// Cases where a stage fell back to the foreign implementation.
    pub fallback_cases: u64,
    /// Cases where a sealed entry failed verification.
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the whole chain per case: the stage statuses, the folded
    /// haystack, the **derived origin**, the suffix offset and the final index.
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl SuffixVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.toupper_hay_native_cases == self.cases_run
            && self.toupper_needle_a_native_cases == self.cases_run
            && self.memchr_a_native_cases == self.cases_run
            && self.toupper_needle_b_native_cases == self.cases_run
            // Every case either ran the suffix search natively, or had no origin at
            // all. No case may leave a dispatched stage unaccounted for.
            && self.memchr_b_native_cases + self.memchr_b_not_reached_cases == self.cases_run
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.toupper_object_hash.is_empty()
            && !self.memchr_object_hash.is_empty()
            && !self.chain_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};cases_run={};toupper_hay_native_cases={};toupper_needle_a_native_cases={};memchr_a_native_cases={};toupper_needle_b_native_cases={};memchr_b_native_cases={};memchr_b_not_reached_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.toupper_object_hash,
            self.toupper_elf_symbol,
            self.memchr_object_hash,
            self.memchr_elf_symbol,
            self.cases_run,
            self.toupper_hay_native_cases,
            self.toupper_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.toupper_needle_b_native_cases,
            self.memchr_b_native_cases,
            self.memchr_b_not_reached_cases,
            self.fallback_cases,
            self.broken_seal_cases,
            self.cases_passed,
            self.cases_failed,
            self.dispatches_run,
            self.oracle_hash,
            self.chain_hash,
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[Mismatch]) -> String {
        let stages: Vec<String> = self
            .stages
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect();
        let body: Vec<String> = mismatches.iter().map(|m| m.to_json()).collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"toupper_hay_native_cases\": {},\n  \"toupper_needle_a_native_cases\": {},\n  \"memchr_a_native_cases\": {},\n  \"toupper_needle_b_native_cases\": {},\n  \"memchr_b_native_cases\": {},\n  \"memchr_b_not_reached_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            self.toupper_object_hash,
            json_escape(&self.toupper_elf_symbol),
            self.memchr_object_hash,
            json_escape(&self.memchr_elf_symbol),
            self.cases_run,
            self.toupper_hay_native_cases,
            self.toupper_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.toupper_needle_b_native_cases,
            self.memchr_b_native_cases,
            self.memchr_b_not_reached_cases,
            self.fallback_cases,
            self.broken_seal_cases,
            self.cases_passed,
            self.cases_failed,
            self.dispatches_run,
            self.oracle_hash,
            self.chain_hash,
            self.verdict.as_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

/// One chain-hash row (kept as a named struct so the hash input is readable).
#[derive(Clone, Debug)]
struct SuffixRow {
    case_id: String,
    hay: Stage,
    needle_a: Stage,
    memchr_a: Stage,
    needle_b: Stage,
    memchr_b: Option<Stage>,
    hay_norm: Vec<u8>,
    needle_a_norm: Option<u8>,
    needle_b_norm: Option<u8>,
    origin: Option<i32>,
    suffix_index: Option<i32>,
    index: Option<i32>,
}

fn push_byte(buf: &mut String, b: Option<u8>) {
    match b {
        Some(v) => buf.push_str(&format!("{:02x}", v)),
        None => buf.push('-'),
    }
}

fn push_index(buf: &mut String, i: Option<i32>) {
    match i {
        Some(v) => buf.push_str(&v.to_string()),
        None => buf.push('-'),
    }
}

/// SHA-256 over the whole chain: per case, the stage statuses, the folded
/// haystack, the **derived origin**, the suffix offset and the final index. A
/// chain that skipped the slice but coincided on the answer still differs here,
/// because the origin and the suffix offset are hashed.
fn chain_hash(rows: &[SuffixRow]) -> String {
    let mut buf = String::new();
    for row in rows {
        buf.push_str(&row.case_id);
        buf.push(';');
        buf.push_str(row.hay.as_str());
        buf.push(';');
        buf.push_str(row.needle_a.as_str());
        buf.push(';');
        buf.push_str(row.memchr_a.as_str());
        buf.push(';');
        buf.push_str(row.needle_b.as_str());
        buf.push(';');
        match row.memchr_b {
            Some(s) => buf.push_str(s.as_str()),
            None => buf.push('-'),
        }
        buf.push(';');
        buf.push_str(&hex::encode(&row.hay_norm));
        buf.push(';');
        push_byte(&mut buf, row.needle_a_norm);
        buf.push(';');
        push_byte(&mut buf, row.needle_b_norm);
        buf.push(';');
        push_index(&mut buf, row.origin);
        buf.push(';');
        push_index(&mut buf, row.suffix_index);
        buf.push(';');
        push_index(&mut buf, row.index);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

// ============================================================================
// Court
// ============================================================================

/// Replay the composition corpus through the sealed chain.
///
/// Returns the verdict plus every mismatch. `index` must contain the sealed entries
/// for every stage (the toupper and memchr ports).
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (SuffixVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut toupper_hay_native: u64 = 0;
    let mut toupper_needle_a_native: u64 = 0;
    let mut memchr_a_native: u64 = 0;
    let mut toupper_needle_b_native: u64 = 0;
    let mut memchr_b_native: u64 = 0;
    let mut memchr_b_not_reached: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows: Vec<SuffixRow> = Vec::with_capacity(traces.len());
    let mut toupper_object_hash = String::new();
    let mut toupper_elf_symbol = String::new();
    let mut memchr_object_hash = String::new();
    let mut memchr_elf_symbol = String::new();

    for t in traces {
        let args = t.input_args();
        let hay = args.first().cloned().unwrap_or_default();
        let needle_a = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
        let needle_b = args.get(2).and_then(|a| a.first()).copied().unwrap_or(0);
        let n = args.get(3).map(|x| decode_usize(x)).unwrap_or(0);

        let outcome = run_chain(&mut dispatcher, &hay, needle_a, needle_b, n, auth);
        dispatches += outcome.dispatches;

        for (native, stage) in [
            (&mut toupper_hay_native, outcome.hay),
            (&mut toupper_needle_a_native, outcome.needle_a),
            (&mut memchr_a_native, outcome.memchr_a),
            (&mut toupper_needle_b_native, outcome.needle_b),
        ] {
            if stage == Stage::Native {
                *native += 1;
            }
        }
        match outcome.memchr_b {
            None => memchr_b_not_reached += 1,
            Some(Stage::Native) => memchr_b_native += 1,
            // A fallback or a broken seal in the second search is counted below.
            Some(_) => {}
        }

        let ran = [
            outcome.hay,
            outcome.needle_a,
            outcome.memchr_a,
            outcome.needle_b,
        ];
        let b_stage = outcome.memchr_b;
        let any_fallback =
            ran.iter().any(|s| *s == Stage::Fallback) || b_stage == Some(Stage::Fallback);
        let any_broken = ran.iter().any(|s| *s == Stage::Broken) || b_stage == Some(Stage::Broken);
        if any_fallback {
            fallback += 1;
        }
        if any_broken {
            broken += 1;
        }

        // Record the concrete sealed objects that served this case.
        if toupper_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_TOUPPER.id) {
                toupper_object_hash = h;
                toupper_elf_symbol = s;
            }
        }
        if memchr_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_MEMCHR.id) {
                memchr_object_hash = h;
                memchr_elf_symbol = s;
            }
        }

        let actual_hex = match outcome.index {
            Some(i) => hex::encode(encode_index(i)),
            None => String::new(),
        };

        let all_native = ran.iter().all(|s| *s == Stage::Native)
            && matches!(b_stage, None | Some(Stage::Native));
        if all_native && actual_hex == t.output_hex && t.status == "ok" {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(Mismatch {
                case_id: t.case_id.clone(),
                expected_output_hex: t.output_hex.clone(),
                actual_output_hex: if any_broken {
                    String::from("!broken seal")
                } else if any_fallback {
                    String::from("!foreign fallback")
                } else {
                    actual_hex
                },
            });
        }

        rows.push(SuffixRow {
            case_id: t.case_id.clone(),
            hay: outcome.hay,
            needle_a: outcome.needle_a,
            memchr_a: outcome.memchr_a,
            needle_b: outcome.needle_b,
            memchr_b: outcome.memchr_b,
            hay_norm: outcome.hay_norm.clone(),
            needle_a_norm: outcome.needle_a_norm,
            needle_b_norm: outcome.needle_b_norm,
            origin: outcome.origin,
            suffix_index: outcome.suffix_index,
            index: outcome.index,
        });
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0
        && fallback == 0
        && broken == 0
        && toupper_hay_native == cases_run
        && toupper_needle_a_native == cases_run
        && memchr_a_native == cases_run
        && toupper_needle_b_native == cases_run
        && memchr_b_native + memchr_b_not_reached == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        SuffixVerdict {
            target: COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id.to_string(),
            stages: COMPOSITION_TOUPPER_MEMCHR_SUFFIX
                .stages
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            toupper_object_hash,
            toupper_elf_symbol,
            memchr_object_hash,
            memchr_elf_symbol,
            cases_run,
            toupper_hay_native_cases: toupper_hay_native,
            toupper_needle_a_native_cases: toupper_needle_a_native,
            memchr_a_native_cases: memchr_a_native,
            toupper_needle_b_native_cases: toupper_needle_b_native,
            memchr_b_native_cases: memchr_b_native,
            memchr_b_not_reached_cases: memchr_b_not_reached,
            fallback_cases: fallback,
            broken_seal_cases: broken,
            cases_passed: passed,
            cases_failed: failed,
            dispatches_run: dispatches,
            oracle_hash: combined_oracle_hash(traces),
            chain_hash: chain_hash(&rows),
            verdict,
        },
        mismatches,
    )
}

// ============================================================================
// One composed call (the CLI / runtime path)
// ============================================================================

/// One composed call, as reported to the CLI.
#[derive(Clone, Debug)]
pub struct CompositionCall {
    pub index: Option<i32>,
    pub origin: Option<i32>,
    pub suffix_index: Option<i32>,
    pub hay_norm: Vec<u8>,
    pub needle_a_norm: Option<u8>,
    pub needle_b_norm: Option<u8>,
    pub hay_stage: &'static str,
    pub memchr_a_stage: &'static str,
    pub memchr_b_stage: &'static str,
    pub dispatches: u64,
}

/// Run this composition for one call through a fresh dispatcher over `index`.
pub fn run_composition_call(
    index: &SealedPortIndex,
    hay: &[u8],
    needle_a: u8,
    needle_b: u8,
    n: usize,
    auth: &PortingAuthority,
) -> CompositionCall {
    let mut dispatcher = NativeDispatcher::new(index.clone());
    let o = run_chain(&mut dispatcher, hay, needle_a, needle_b, n, auth);
    CompositionCall {
        index: o.index,
        origin: o.origin,
        suffix_index: o.suffix_index,
        hay_norm: o.hay_norm,
        needle_a_norm: o.needle_a_norm,
        needle_b_norm: o.needle_b_norm,
        hay_stage: o.hay.as_str(),
        memchr_a_stage: o.memchr_a.as_str(),
        // "-" when there was no origin, so the second search never ran.
        memchr_b_stage: o.memchr_b.map(|s| s.as_str()).unwrap_or("-"),
        dispatches: o.dispatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::dialect_cage;
    use crate::porting::promotion::TrustState;
    use crate::porting::store;
    use crate::porting::target::LIBC_MEMCHR as MEMCHR;
    use crate::porting::target::LIBC_TOUPPER as TOUPPER;
    use crate::porting::{SealedArtifact, SealedPortEntry};

    fn traces_for(cases: &[TestCase]) -> Vec<OracleTrace> {
        cases
            .iter()
            .map(|c| {
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &[0u8; 8],
                    "ok",
                    &["compute"],
                )
            })
            .collect()
    }

    fn case(id: &str, hay: &[u8], a: u8, b: u8, n: usize) -> TestCase {
        TestCase::new(
            id.to_string(),
            alloc::vec![
                hay.to_vec(),
                alloc::vec![a],
                alloc::vec![b],
                (n as u64).to_le_bytes().to_vec()
            ],
        )
    }

    /// C-locale `toupper`, as the corpus and oracle assume it.
    fn fold(b: u8) -> u8 {
        if (b'a'..=b'z').contains(&b) {
            b - 32
        } else {
            b
        }
    }

    /// First occurrence of `needle` in the first `n` bytes — what a chain that
    /// searched the caller's window (ignoring the slice) would find.
    fn window_search(hay: &[u8], needle: u8, n: usize) -> Option<usize> {
        hay[..n.min(hay.len())].iter().position(|&b| b == needle)
    }

    #[test]
    fn test_corpus_is_deterministic_and_well_formed() {
        let a = composition_corpus();
        let b = composition_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 688);

        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len(), "case ids are unique");

        for c in &a {
            assert_eq!(c.args.len(), 4, "case {}", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {}", c.case_id);
            assert_eq!(c.args[2].len(), 1, "case {}", c.case_id);
            let n = u64::from_le_bytes(c.args[3].as_slice().try_into().unwrap()) as usize;
            assert!(n >= 1 && n <= c.args[0].len(), "case {}", c.case_id);
            assert!(c.args[0].len() <= 8, "case {}", c.case_id);
        }

        assert_eq!(ids.iter().filter(|id| id.starts_with("A.")).count(), 56);
        assert_eq!(ids.iter().filter(|id| id.starts_with("B.")).count(), 28);
        assert_eq!(ids.iter().filter(|id| id.starts_with("G.")).count(), 256);
        assert_eq!(ids.iter().filter(|id| id.starts_with("H.")).count(), 256);
        assert!(ids.contains(&"B.before.4.1"));
    }

    /// The oracle slices the folded buffer at the derived origin and reports an
    /// absolute index — hand-computed, so the oracle itself is pinned.
    #[test]
    fn test_oracle_slices_the_buffer_and_reports_an_absolute_index() {
        let cases = alloc::vec![
            // fold "BAXB": origin 1, suffix "AXB", 'b' -> offset 2 -> 3
            case("slice", b"baxb", 0x61, 0x62, 4),
            // fold "BXXA": origin 3, suffix "A", no 'B' -> -1 (the 'b' at 0 is skipped)
            case("before_only", b"bxxa", 0x61, 0x62, 4),
            // fold "BXXB": needleA absent -> -1
            case("no_origin", b"bxxb", 0x61, 0x62, 4),
            // fold "AXXB": origin 0, suffix is the whole window -> 3
            case("origin_zero", b"axxb", 0x61, 0x62, 4),
            // fold "BBAB": origin 2, suffix "AB", 'B' at 1 -> 3
            case("repeated", b"bbab", 0x61, 0x62, 4),
            // the needle is supplied uppercase; the fold is not one-sided
            case("folded_needle", b"baxb", 0x41, 0x42, 4),
        ];
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        let got: Vec<String> = traces.iter().map(|t| t.output_hex.clone()).collect();
        assert_eq!(
            got,
            alloc::vec!["03000000", "ffffffff", "ffffffff", "03000000", "03000000", "03000000",]
        );
    }

    /// The corpus must contain cases where a chain that searched the caller's window
    /// (a bound-only shape) would disagree with the sliced answer. Without these the
    /// court could pass a chain that never slices.
    #[test]
    fn test_corpus_contains_cases_a_bound_only_chain_would_fail() {
        let cases = composition_corpus();
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();

        let mut differ = 0usize;
        let mut before_origin_differ = 0usize;
        for (c, t) in cases.iter().zip(traces.iter()) {
            let folded: Vec<u8> = c.args[0].iter().map(|&b| fold(b)).collect();
            let nb = fold(c.args[2][0]);
            let n = decode_usize(&c.args[3]).min(folded.len());
            let window = window_search(&folded, nb, n)
                .map(|i| i as i32)
                .unwrap_or(-1);
            let expected = decode_index(&hex::decode(&t.output_hex).unwrap());
            if window != expected {
                differ += 1;
                if c.case_id.starts_with("B.") {
                    before_origin_differ += 1;
                }
            }
        }
        assert!(differ >= 80, "only {} cases discriminate", differ);
        assert_eq!(
            before_origin_differ, 28,
            "the 'needleB only before the origin' group"
        );
    }

    #[test]
    fn test_empty_corpus_is_inconclusive() {
        let (v, _) =
            run_composition_court(&[], &SealedPortIndex::new(), &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconclusive);
        assert!(!v.is_sealed_eligible());
    }

    #[test]
    fn test_composition_without_sealed_ports_is_inconsistent() {
        let cases = composition_corpus();
        let traces = traces_for(&cases);
        let (v, _) = run_composition_court(
            &traces,
            &SealedPortIndex::new(),
            &PortingAuthority::granted(),
        );
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.toupper_hay_native_cases, 0);
        assert_eq!(v.memchr_a_native_cases, 0);
        assert_eq!(v.memchr_b_native_cases, 0);
        assert_eq!(v.memchr_b_not_reached_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    #[test]
    fn test_composition_broken_seal_is_not_a_fallback() {
        let mut index = SealedPortIndex::new();
        for target in [TOUPPER, MEMCHR] {
            index.insert(SealedPortEntry {
                target: target.id.to_string(),
                trust: TrustState::Sealed,
                artifact: SealedArtifact::leaf_object(
                    String::from("deadbeef"),
                    String::from("/nonexistent/candidate.o"),
                ),
                oracle_hash: String::new(),
                candidate_behavior_hash: String::new(),
                candidate_source_hash: String::new(),
                sealed_package: String::new(),
            });
        }

        let cases = composition_corpus();
        let traces = traces_for(&cases);
        let (v, _) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.broken_seal_cases, v.cases_run);
        assert_eq!(v.fallback_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    #[test]
    fn test_composition_without_capability_falls_back() {
        let cases = composition_corpus();
        let traces = traces_for(&cases);
        let (v, _) =
            run_composition_court(&traces, &SealedPortIndex::new(), &PortingAuthority::none());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
    }

    /// End to end against the committed store: the slice is load-bearing, so a
    /// needleB that occurs before the origin does not match, and the answer is the
    /// absolute index rather than the suffix offset.
    #[test]
    fn test_the_slice_is_load_bearing_against_the_committed_store() {
        let index = store::load_default().expect("committed store loads");
        let auth = PortingAuthority::granted();

        // Both of these have needleB *before* the derived origin, so a window search
        // would return 0 while the sliced search returns 3.
        for (hay, origin) in [(&b"bbab"[..], 2), (&b"baxb"[..], 1)] {
            let c = run_composition_call(&index, hay, 0x61, 0x62, 4, &auth);
            assert_eq!(c.origin, Some(origin), "haystack {:?}", hay);
            assert_eq!(c.index, Some(3), "haystack {:?}", hay);
            assert_eq!(
                c.origin.unwrap() + c.suffix_index.expect("the suffix search ran"),
                3
            );
        }

        // The window answer for the first case is 0, not 3: the chain really sliced.
        assert_eq!(window_search(&[0x42, 0x42, 0x41, 0x42], 0x42, 4), Some(0));

        // No origin: the suffix search never ran, and that is reported as such.
        let c = run_composition_call(&index, b"bxxb", 0x61, 0x62, 4, &auth);
        assert_eq!(c.origin, Some(-1));
        assert_eq!(c.suffix_index, None);
        assert_eq!(c.index, Some(-1));
        assert_eq!(c.memchr_b_stage, "-");
    }

    /// The second search is data-dependent: the corpus must exercise both the run
    /// and the not-reached path, and the court must account for both.
    #[test]
    fn test_the_suffix_search_is_data_dependent() {
        let index = store::load_default().expect("committed store loads");
        let cases = composition_corpus();
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        let (v, mismatches) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert!(mismatches.is_empty(), "{:?}", mismatches.first());
        assert!(v.is_sealed_eligible());
        assert_eq!(v.cases_run, cases.len() as u64);
        assert_eq!(v.memchr_a_native_cases, v.cases_run);
        assert!(v.memchr_b_native_cases > 0, "the suffix search never ran");
        assert!(
            v.memchr_b_not_reached_cases > 0,
            "no case exercised the absent-origin path"
        );
        assert_eq!(
            v.memchr_b_native_cases + v.memchr_b_not_reached_cases,
            v.cases_run
        );
        assert_eq!(v.fallback_cases, 0);
        assert_eq!(v.broken_seal_cases, 0);
    }

    /// The chain hash must cover the derived origin and the suffix offset, not just
    /// the final index.
    #[test]
    fn test_chain_hash_covers_the_origin_and_the_suffix_offset() {
        let base = || SuffixRow {
            case_id: String::from("c"),
            hay: Stage::Native,
            needle_a: Stage::Native,
            memchr_a: Stage::Native,
            needle_b: Stage::Native,
            memchr_b: Some(Stage::Native),
            hay_norm: alloc::vec![0x42, 0x41, 0x42],
            needle_a_norm: Some(0x41),
            needle_b_norm: Some(0x42),
            origin: Some(1),
            suffix_index: Some(1),
            index: Some(2),
        };
        let h = chain_hash(&[base()]);
        assert_eq!(h.len(), 64);

        let mut changed = base();
        changed.origin = Some(0);
        assert_ne!(chain_hash(&[changed]), h, "origin is not hashed");

        let mut changed = base();
        changed.suffix_index = Some(0);
        assert_ne!(chain_hash(&[changed]), h, "suffix offset is not hashed");

        let mut changed = base();
        changed.memchr_b = None;
        assert_ne!(chain_hash(&[changed]), h, "not-reached is not hashed");
    }
}
