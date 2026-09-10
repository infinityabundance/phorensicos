// porting/composition_pair.rs — Sealed Composition Dispatch Court (third chain)
//
// The first composition (`toupper ∘ memchr`) chained a map stage into a search
// stage. The second (`toupper ∘ strlen ∘ memchr`) showed a stage's *result* can be
// the next stage's *argument*. This third composition shows the dependency pattern
// is not a one-off **pipe**: a single derived value is consumed by **two** stages,
// and the second consumer is deliberately **not adjacent** to the producer.
//
//   phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1
//
//   input:  haystack, needleA, needleB, n   (n = bound: a NUL lies within haystack[..n])
//   oracle: foreign C-locale `toupper` over the haystack,
//           then foreign `strlen` of the folded haystack -> L,
//           then foreign `memchr` of folded needleA bounded by L -> iA,
//           then foreign `memchr` of folded needleB bounded by L -> iB
//   sealed: dispatch sealed toupper once per haystack byte,
//           dispatch sealed strlen once -> L,
//           dispatch sealed toupper for needleA, dispatch sealed memchr (n = L) -> iA,
//           dispatch sealed toupper for needleB, dispatch sealed memchr (n = L) -> iB
//
// Stage 6 consumes `L`, which stage 2 produced — there are three stages (3, 4, 5)
// between them. So the runner is not a pipe with a single live intermediate: it
// holds a derived value and feeds it to every stage that needs it, in any order.
// The observable is the pair `(iA, iB)`, each an index into the measured string or
// `-1`; the runner performs no combining logic of its own, so the composition still
// adds no trusted code beyond the sealed leaves.
//
// Read plainly: a case-insensitive "where does this string first contain A, and
// where does it first contain B", where the string's length is established by the
// sealed `strlen` rather than supplied by the caller.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize, encode_index};
use crate::porting::composition::{CompositionTarget, Stage};
use crate::porting::dispatch::{DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_STRLEN, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The third composition: fold, derive the bound once with `strlen`, then run two
/// `memchr` searches that both consume that one derived bound.
///
/// The id is `phor:compose:...`, not `libc:...`: this is a Phorensic composition
/// over already-sealed ports, not a single foreign API surface.
pub const COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1",
    locale_contract: "C",
    stages: &[
        "libc:toupper:c-locale:u8:v1",
        "libc:strlen:c-locale:u64:v1",
        "libc:memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needleA, u8 needleB, usize n) — a buffer whose NUL terminator lies within the first n bytes",
    output_schema: "two i32 indexes packed little-endian (iA, iB): the first C-locale-folded match of each needle within the string length, or -1",
    domain_summary: "a NUL-terminated-string corpus: every ordered pair of in-string match indexes (both orders), one-present-one-absent, both-needles-identical, occurrences only after the terminator (excluded by the shared derived bound), the unsigned edge bytes, and an exhaustive 0..=255 sweep of needleA against a fixed needleB",
};

// ============================================================================
// Composition corpus
// ============================================================================

/// The third composition's corpus.
///
/// Every case is a C string (a NUL inside `haystack[..n]`) and carries two needles.
/// The two searches must share one derived bound, so cases pin down both indexes
/// independently: distinct match positions in both orders, one-present/one-absent,
/// identical needles, and occurrences that exist only after the terminator.
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

    // Distinct lowercase content bytes; C-locale `toupper` folds them to uppercase.
    const LOWER: [u8; 7] = [0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67]; // a..g
    const PAD: u8 = 0xff; // not a letter: `toupper` leaves it unchanged
    const ABSENT: u8 = 0x7a; // 'z' -> 'Z', never in the folded content

    // LOWER[..len] + NUL + 0xff fill, 8 bytes with the terminator at `len`.
    let padded = |len: usize| -> Vec<u8> {
        let mut buf = alloc::vec![PAD; 8];
        buf[..len].copy_from_slice(&LOWER[..len]);
        buf[len] = 0x00;
        buf
    };

    // A — the empty string: the shared bound is 0, so neither needle can match.
    for n in 1..=8usize {
        let mut buf = alloc::vec![0x61u8; n];
        buf[0] = 0x00;
        mk(&mut cases, format!("A.empty.n{}", n), &buf, 0x61, 0x62, n);
    }

    // B — both needles present, at every ordered pair of distinct indexes. The
    //     content bytes are distinct, so iA and iB are exactly the indexes tested,
    //     and the `.ba` cases cover iA > iB as well as iA < iB.
    for len in 2..=7usize {
        let buf = padded(len);
        for i in 0..len {
            for j in (i + 1)..len {
                mk(
                    &mut cases,
                    format!("B.{}.{}.{}.ab", len, i, j),
                    &buf,
                    LOWER[i],
                    LOWER[j],
                    8,
                );
                mk(
                    &mut cases,
                    format!("B.{}.{}.{}.ba", len, i, j),
                    &buf,
                    LOWER[j],
                    LOWER[i],
                    8,
                );
            }
        }
    }

    // D — one present, one absent: the absent needle's own search must still run
    //     (and return -1) against the same derived bound.
    for len in 1..=7usize {
        let buf = padded(len);
        for i in 0..len {
            mk(
                &mut cases,
                format!("D.{}.{}.a", len, i),
                &buf,
                LOWER[i],
                ABSENT,
                8,
            );
            mk(
                &mut cases,
                format!("D.{}.{}.b", len, i),
                &buf,
                ABSENT,
                LOWER[i],
                8,
            );
        }
    }

    // E — both needles identical: both searches must return the same index.
    for len in 1..=7usize {
        let buf = padded(len);
        for i in 0..len {
            mk(
                &mut cases,
                format!("E.{}.{}", len, i),
                &buf,
                LOWER[i],
                LOWER[i],
                8,
            );
        }
    }

    // F — both needles occur only after the terminator: the shared derived bound
    //     must exclude the tail for both searches.
    for len in 0..=4usize {
        let mut buf = alloc::vec![0x00u8; len + 4];
        for i in 0..len {
            buf[i] = [0x63u8, 0x64, 0x65, 0x66][i]; // c..f (neither needle)
        }
        buf[len] = 0x00;
        buf[len + 1] = 0x61;
        buf[len + 2] = 0x62;
        buf[len + 3] = 0x61;
        mk(
            &mut cases,
            format!("F.tail.{}", len),
            &buf,
            0x61,
            0x62,
            buf.len(),
        );
    }

    // F2 — one needle inside the string, the other only in the tail.
    for len in 0..=4usize {
        let mut buf = alloc::vec![0x00u8; len + 4];
        buf[0] = 0x61;
        for i in 1..=len {
            buf[i] = 0x63;
        }
        buf[len + 1] = 0x00;
        buf[len + 2] = 0x62;
        buf[len + 3] = 0x62;
        mk(
            &mut cases,
            format!("F2.mixed.{}", len),
            &buf,
            0x61,
            0x62,
            buf.len(),
        );
    }

    // G — exhaustive sweep of needleA against a fixed needleB. The haystack folds
    //     to "AB\0AB\0\x7f\x80" and the shared bound is 2, so B is always found at
    //     1 while A's search exercises every byte value.
    const G_BUF: [u8; 8] = [0x61, 0x62, 0x00, 0x61, 0x62, 0x00, 0x7f, 0x80];
    for needle in 0..=255u16 {
        mk(
            &mut cases,
            format!("G.{:02x}", needle),
            &G_BUF,
            needle as u8,
            0x42,
            8,
        );
    }

    // H — unfoldable edge bytes before the terminator, with copies after it.
    const H_BUF: [u8; 5] = [0x7f, 0x80, 0x00, 0x7f, 0x80];
    for (a, b) in [(0x7fu8, 0x80u8), (0x80, 0x7f), (0x00, 0x01), (0x7f, 0x7f)] {
        mk(
            &mut cases,
            format!("H.{:02x}.{:02x}", a, b),
            &H_BUF,
            a,
            b,
            5,
        );
    }

    cases
}

// ============================================================================
// The chain
// ============================================================================

/// One composed call: the normalized intermediates, the shared derived bound and
/// both result indexes.
#[derive(Clone, Debug)]
struct ChainOutcome {
    index_a: Option<i32>,
    index_b: Option<i32>,
    hay: Stage,
    strlen: Stage,
    needle_a: Stage,
    memchr_a: Stage,
    needle_b: Stage,
    memchr_b: Stage,
    dispatches: u64,
    hay_norm: Vec<u8>,
    derived_len: Option<usize>,
    needle_a_norm: Option<u8>,
    needle_b_norm: Option<u8>,
}

/// The encoded observable: `iA` then `iB`, each a 4-byte little-endian index.
fn encode_pair(a: i32, b: i32) -> Vec<u8> {
    let mut out = encode_index(a);
    out.extend_from_slice(&encode_index(b));
    out
}

/// Run the sealed chain for one case.
///
/// Every stage goes through `NativeDispatcher`; the Rust mirrors are never called.
/// A fallback or a broken seal short-circuits the remaining stages (recorded as not
/// native). Stage 2's sealed `strlen` result is consumed by **both** search stages,
/// including stage 6, which is not adjacent to it.
fn run_chain(
    dispatcher: &mut NativeDispatcher,
    hay: &[u8],
    needle_a: u8,
    needle_b: u8,
    n: usize,
    auth: &PortingAuthority,
) -> ChainOutcome {
    let mut out = ChainOutcome {
        index_a: None,
        index_b: None,
        hay: Stage::Native,
        strlen: Stage::Native,
        needle_a: Stage::Native,
        memchr_a: Stage::Native,
        needle_b: Stage::Native,
        memchr_b: Stage::Native,
        dispatches: 0,
        hay_norm: Vec::with_capacity(hay.len()),
        derived_len: None,
        needle_a_norm: None,
        needle_b_norm: None,
    };

    // Every remaining stage is unreachable once a link fails; mark them all.
    let fail_all = |out: &mut ChainOutcome, stage: Stage| {
        out.hay = stage;
        out.strlen = stage;
        out.needle_a = stage;
        out.memchr_a = stage;
        out.needle_b = stage;
        out.memchr_b = stage;
    };

    // Stage 1 — uppercase every haystack byte with the sealed toupper.
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

    // Stage 2 — measure the folded string with the sealed strlen. This single
    // derived bound is consumed twice, by stages 4 and 6.
    let strlen_args = alloc::vec![
        out.hay_norm.clone(),
        (n.min(out.hay_norm.len()) as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_STRLEN, &strlen_args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.derived_len = Some(decode_usize(&o.output));
        }
        Ok(_) => {
            out.strlen = Stage::Fallback;
            out.needle_a = Stage::Fallback;
            out.memchr_a = Stage::Fallback;
            out.needle_b = Stage::Fallback;
            out.memchr_b = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.strlen = Stage::Broken;
            out.needle_a = Stage::Broken;
            out.memchr_a = Stage::Broken;
            out.needle_b = Stage::Broken;
            out.memchr_b = Stage::Broken;
            return out;
        }
    }
    let derived = out.derived_len.unwrap_or(0);

    // Stage 3 — uppercase needleA with the sealed toupper.
    match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![needle_a]], auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_a_norm = Some(o.output.first().copied().unwrap_or(needle_a));
        }
        Ok(_) => {
            out.needle_a = Stage::Fallback;
            out.memchr_a = Stage::Fallback;
            out.needle_b = Stage::Fallback;
            out.memchr_b = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.needle_a = Stage::Broken;
            out.memchr_a = Stage::Broken;
            out.needle_b = Stage::Broken;
            out.memchr_b = Stage::Broken;
            return out;
        }
    }

    // Stage 4 — search the folded string for folded needleA, bounded by the shared
    // derived length (never the caller's `n`).
    let folded_a = out.needle_a_norm.unwrap_or(needle_a);
    let args_a = alloc::vec![
        out.hay_norm.clone(),
        alloc::vec![folded_a],
        (derived as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_MEMCHR, &args_a, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.index_a = Some(decode_index(&o.output));
        }
        Ok(_) => {
            out.memchr_a = Stage::Fallback;
            out.needle_b = Stage::Fallback;
            out.memchr_b = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.memchr_a = Stage::Broken;
            out.needle_b = Stage::Broken;
            out.memchr_b = Stage::Broken;
            return out;
        }
    }

    // Stage 5 — uppercase needleB with the sealed toupper.
    match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![needle_b]], auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_b_norm = Some(o.output.first().copied().unwrap_or(needle_b));
        }
        Ok(_) => {
            out.needle_b = Stage::Fallback;
            out.memchr_b = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.needle_b = Stage::Broken;
            out.memchr_b = Stage::Broken;
            return out;
        }
    }

    // Stage 6 — search for folded needleB, consuming the SAME derived bound that
    // stage 4 used. This dependency is non-adjacent: stages 3, 4 and 5 sit between
    // the producer (stage 2) and this consumer.
    let folded_b = out.needle_b_norm.unwrap_or(needle_b);
    let args_b = alloc::vec![
        out.hay_norm.clone(),
        alloc::vec![folded_b],
        (derived as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_MEMCHR, &args_b, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.index_b = Some(decode_index(&o.output));
        }
        Ok(_) => out.memchr_b = Stage::Fallback,
        Err(_) => out.memchr_b = Stage::Broken,
    }

    out
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed objects the chain actually dispatched to.
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    pub strlen_object_hash: String,
    pub strlen_elf_symbol: String,
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    pub cases_run: u64,
    pub toupper_hay_native_cases: u64,
    pub strlen_native_cases: u64,
    pub toupper_needle_a_native_cases: u64,
    pub memchr_a_native_cases: u64,
    pub toupper_needle_b_native_cases: u64,
    pub memchr_b_native_cases: u64,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the whole chain per case (stage status + normalized
    /// intermediates + the shared derived bound + both final indexes).
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl PairVerdict {
    /// Every stage was sealed-native for every case, and every case matched the
    /// oracle.
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.toupper_hay_native_cases == self.cases_run
            && self.strlen_native_cases == self.cases_run
            && self.toupper_needle_a_native_cases == self.cases_run
            && self.memchr_a_native_cases == self.cases_run
            && self.toupper_needle_b_native_cases == self.cases_run
            && self.memchr_b_native_cases == self.cases_run
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.toupper_object_hash.is_empty()
            && !self.strlen_object_hash.is_empty()
            && !self.memchr_object_hash.is_empty()
            && !self.chain_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};strlen_object_hash={};strlen_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};cases_run={};toupper_hay_native_cases={};strlen_native_cases={};toupper_needle_a_native_cases={};memchr_a_native_cases={};toupper_needle_b_native_cases={};memchr_b_native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.toupper_object_hash,
            self.toupper_elf_symbol,
            self.strlen_object_hash,
            self.strlen_elf_symbol,
            self.memchr_object_hash,
            self.memchr_elf_symbol,
            self.cases_run,
            self.toupper_hay_native_cases,
            self.strlen_native_cases,
            self.toupper_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.toupper_needle_b_native_cases,
            self.memchr_b_native_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"strlen_object_hash\": \"{}\",\n  \"strlen_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"toupper_hay_native_cases\": {},\n  \"strlen_native_cases\": {},\n  \"toupper_needle_a_native_cases\": {},\n  \"memchr_a_native_cases\": {},\n  \"toupper_needle_b_native_cases\": {},\n  \"memchr_b_native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            self.toupper_object_hash,
            json_escape(&self.toupper_elf_symbol),
            self.strlen_object_hash,
            json_escape(&self.strlen_elf_symbol),
            self.memchr_object_hash,
            json_escape(&self.memchr_elf_symbol),
            self.cases_run,
            self.toupper_hay_native_cases,
            self.strlen_native_cases,
            self.toupper_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.toupper_needle_b_native_cases,
            self.memchr_b_native_cases,
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

/// SHA-256 over the whole chain: per case, the six stage statuses, the normalized
/// intermediates, the **shared derived bound** and **both** indexes. A fallback, a
/// different fold, a different derived length, or either index changing all change
/// the hash — so it proves the chain, not just the answer.
#[allow(clippy::type_complexity)]
fn chain_hash(rows: &[PairRow]) -> String {
    let mut buf = String::new();
    for row in rows {
        buf.push_str(&row.case_id);
        buf.push(';');
        buf.push_str(row.hay.as_str());
        buf.push(';');
        buf.push_str(row.strlen.as_str());
        buf.push(';');
        buf.push_str(row.needle_a.as_str());
        buf.push(';');
        buf.push_str(row.memchr_a.as_str());
        buf.push(';');
        buf.push_str(row.needle_b.as_str());
        buf.push(';');
        buf.push_str(row.memchr_b.as_str());
        buf.push(';');
        buf.push_str(&hex::encode(&row.hay_norm));
        buf.push(';');
        match row.derived {
            Some(l) => buf.push_str(&l.to_string()),
            None => buf.push('-'),
        }
        buf.push(';');
        push_byte(&mut buf, row.needle_a_norm);
        buf.push(';');
        push_byte(&mut buf, row.needle_b_norm);
        buf.push(';');
        push_index(&mut buf, row.index_a);
        buf.push(';');
        push_index(&mut buf, row.index_b);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

fn push_byte(buf: &mut String, b: Option<u8>) {
    match b {
        Some(v) => buf.push_str(&hex::encode([v])),
        None => buf.push('-'),
    }
}

fn push_index(buf: &mut String, i: Option<i32>) {
    match i {
        Some(v) => buf.push_str(&v.to_string()),
        None => buf.push('-'),
    }
}

/// One chain-hash row (kept as a named struct so the hash input is readable).
#[derive(Clone, Debug)]
#[allow(clippy::type_complexity)]
struct PairRow {
    case_id: String,
    hay: Stage,
    strlen: Stage,
    needle_a: Stage,
    memchr_a: Stage,
    needle_b: Stage,
    memchr_b: Stage,
    hay_norm: Vec<u8>,
    derived: Option<usize>,
    needle_a_norm: Option<u8>,
    needle_b_norm: Option<u8>,
    index_a: Option<i32>,
    index_b: Option<i32>,
}

/// Replay the composition corpus through the sealed chain.
///
/// Returns the verdict plus every mismatch. `index` must contain the sealed entries
/// for every stage (the toupper, strlen and memchr ports).
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (PairVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut toupper_hay_native: u64 = 0;
    let mut strlen_native: u64 = 0;
    let mut toupper_needle_a_native: u64 = 0;
    let mut memchr_a_native: u64 = 0;
    let mut toupper_needle_b_native: u64 = 0;
    let mut memchr_b_native: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows: Vec<PairRow> = Vec::with_capacity(traces.len());
    let mut toupper_object_hash = String::new();
    let mut toupper_elf_symbol = String::new();
    let mut strlen_object_hash = String::new();
    let mut strlen_elf_symbol = String::new();
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
            (&mut strlen_native, outcome.strlen),
            (&mut toupper_needle_a_native, outcome.needle_a),
            (&mut memchr_a_native, outcome.memchr_a),
            (&mut toupper_needle_b_native, outcome.needle_b),
            (&mut memchr_b_native, outcome.memchr_b),
        ] {
            if stage == Stage::Native {
                *native += 1;
            }
        }

        let stages = [
            outcome.hay,
            outcome.strlen,
            outcome.needle_a,
            outcome.memchr_a,
            outcome.needle_b,
            outcome.memchr_b,
        ];
        if stages.iter().any(|s| *s == Stage::Fallback) {
            fallback += 1;
        }
        if stages.iter().any(|s| *s == Stage::Broken) {
            broken += 1;
        }

        // Record the concrete sealed objects that served this case.
        if toupper_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_TOUPPER.id) {
                toupper_object_hash = h;
                toupper_elf_symbol = s;
            }
        }
        if strlen_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_STRLEN.id) {
                strlen_object_hash = h;
                strlen_elf_symbol = s;
            }
        }
        if memchr_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_MEMCHR.id) {
                memchr_object_hash = h;
                memchr_elf_symbol = s;
            }
        }

        let actual_hex = match (outcome.index_a, outcome.index_b) {
            (Some(a), Some(b)) => hex::encode(encode_pair(a, b)),
            _ => String::new(),
        };

        let all_native = stages.iter().all(|s| *s == Stage::Native);
        let any_broken = stages.iter().any(|s| *s == Stage::Broken);
        let any_fallback = stages.iter().any(|s| *s == Stage::Fallback);
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

        rows.push(PairRow {
            case_id: t.case_id.clone(),
            hay: outcome.hay,
            strlen: outcome.strlen,
            needle_a: outcome.needle_a,
            memchr_a: outcome.memchr_a,
            needle_b: outcome.needle_b,
            memchr_b: outcome.memchr_b,
            hay_norm: outcome.hay_norm.clone(),
            derived: outcome.derived_len,
            needle_a_norm: outcome.needle_a_norm,
            needle_b_norm: outcome.needle_b_norm,
            index_a: outcome.index_a,
            index_b: outcome.index_b,
        });
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0
        && fallback == 0
        && broken == 0
        && toupper_hay_native == cases_run
        && strlen_native == cases_run
        && toupper_needle_a_native == cases_run
        && memchr_a_native == cases_run
        && toupper_needle_b_native == cases_run
        && memchr_b_native == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        PairVerdict {
            target: COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id.to_string(),
            stages: COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR
                .stages
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            toupper_object_hash,
            toupper_elf_symbol,
            strlen_object_hash,
            strlen_elf_symbol,
            memchr_object_hash,
            memchr_elf_symbol,
            cases_run,
            toupper_hay_native_cases: toupper_hay_native,
            strlen_native_cases: strlen_native,
            toupper_needle_a_native_cases: toupper_needle_a_native,
            memchr_a_native_cases: memchr_a_native,
            toupper_needle_b_native_cases: toupper_needle_b_native,
            memchr_b_native_cases: memchr_b_native,
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

/// A single composed call, for the CLI/demo path.
#[derive(Clone, Debug)]
pub struct CompositionCall {
    pub index_a: Option<i32>,
    pub index_b: Option<i32>,
    pub hay_norm: Vec<u8>,
    pub derived_len: Option<usize>,
    pub needle_a_norm: Option<u8>,
    pub needle_b_norm: Option<u8>,
    pub hay_stage: &'static str,
    pub strlen_stage: &'static str,
    pub needle_a_stage: &'static str,
    pub memchr_a_stage: &'static str,
    pub needle_b_stage: &'static str,
    pub memchr_b_stage: &'static str,
    pub dispatches: u64,
}

/// Run the sealed chain once, for a call-site demo.
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
        index_a: o.index_a,
        index_b: o.index_b,
        hay_norm: o.hay_norm,
        derived_len: o.derived_len,
        needle_a_norm: o.needle_a_norm,
        needle_b_norm: o.needle_b_norm,
        hay_stage: o.hay.as_str(),
        strlen_stage: o.strlen.as_str(),
        needle_a_stage: o.needle_a.as_str(),
        memchr_a_stage: o.memchr_a.as_str(),
        needle_b_stage: o.needle_b.as_str(),
        memchr_b_stage: o.memchr_b.as_str(),
        dispatches: o.dispatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::LIBC_MEMCHR as MEMCHR;
    use crate::porting::target::LIBC_STRLEN as STRLEN;
    use crate::porting::target::LIBC_TOUPPER as TOUPPER;

    fn traces_for(cases: &[TestCase]) -> Vec<OracleTrace> {
        cases
            .iter()
            .map(|c| {
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id,
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

    #[test]
    fn test_composition_corpus_is_deterministic_and_well_formed() {
        let a = composition_corpus();
        let b = composition_corpus();
        assert_eq!(a, b);
        assert_eq!(a.len(), 474);

        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is 4 args, two 1-byte needles, an 8-byte bound, and a NUL
        // inside the bound (the shared strlen precondition).
        for c in &a {
            assert_eq!(c.args.len(), 4, "case {}", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {}", c.case_id);
            assert_eq!(c.args[2].len(), 1, "case {}", c.case_id);
            let n = u64::from_le_bytes(c.args[3].as_slice().try_into().unwrap()) as usize;
            assert!(n <= c.args[0].len(), "case {}", c.case_id);
            assert!(n <= 8, "case {}", c.case_id);
            assert!(c.args[0].len() <= 8, "case {}", c.case_id);
            assert!(
                c.args[0][..n].contains(&0x00),
                "case {} has no terminator inside its bound",
                c.case_id
            );
        }

        assert!(ids.contains(&"A.empty.n1"));
        assert!(ids.contains(&"B.7.0.6.ab"));
        assert!(ids.contains(&"B.7.0.6.ba"));
        assert!(ids.contains(&"F.tail.4"));
        assert!(ids.contains(&"F2.mixed.4"));
        assert_eq!(ids.iter().filter(|id| id.starts_with("G.")).count(), 256);
        assert!(ids.contains(&"H.7f.80"));
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
        // Every stage must have a real dispatch for every case (no empty haystacks
        // in this corpus), so no stage is vacuously native.
        assert_eq!(v.toupper_hay_native_cases, 0);
        assert_eq!(v.strlen_native_cases, 0);
        assert_eq!(v.toupper_needle_a_native_cases, 0);
        assert_eq!(v.memchr_a_native_cases, 0);
        assert_eq!(v.toupper_needle_b_native_cases, 0);
        assert_eq!(v.memchr_b_native_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    #[test]
    fn test_composition_broken_seal_is_not_a_fallback() {
        use crate::porting::promotion::TrustState;
        use crate::porting::{SealedArtifact, SealedPortEntry};

        let mut index = SealedPortIndex::new();
        for target in [TOUPPER, STRLEN, MEMCHR] {
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

    /// The chain hash must cover the shared derived bound and both indexes.
    #[test]
    fn test_chain_hash_covers_the_shared_bound_and_both_indexes() {
        let base = || PairRow {
            case_id: String::from("c"),
            hay: Stage::Native,
            strlen: Stage::Native,
            needle_a: Stage::Native,
            memchr_a: Stage::Native,
            needle_b: Stage::Native,
            memchr_b: Stage::Native,
            hay_norm: alloc::vec![0x41, 0x42],
            derived: Some(2),
            needle_a_norm: Some(0x41),
            needle_b_norm: Some(0x42),
            index_a: Some(0),
            index_b: Some(1),
        };
        let a = alloc::vec![base()];
        let h = chain_hash(&a);

        let mut b = a.clone();
        b[0].derived = Some(3);
        assert_ne!(h, chain_hash(&b));

        let mut c = a.clone();
        c[0].index_b = Some(-1);
        assert_ne!(h, chain_hash(&c));

        let mut d = a.clone();
        d[0].needle_b_norm = Some(0x43);
        assert_ne!(h, chain_hash(&d));

        let mut e = a.clone();
        e[0].memchr_b = Stage::Fallback;
        assert_ne!(h, chain_hash(&e));
    }
}
