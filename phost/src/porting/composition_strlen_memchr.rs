// porting/composition_strlen_memchr.rs — Sealed Composition Dispatch Court (second chain)
//
// The first composition (`toupper ∘ memchr`) proved sealed artifacts can be chained
// by the runtime. This second composition proves the machinery **generalizes**: the
// chain is three stages, and the middle stage's *result* is not merely an
// intermediate — it is consumed as the **argument** of the next stage.
//
//   phor:compose:toupper_strlen_memchr:c-locale:index:v1
//
//   input:  haystack, needle, n        (n = bound: a NUL lies within haystack[..n])
//   oracle: foreign C-locale `toupper` over the haystack,
//           then foreign `strlen` of the folded haystack -> L,
//           then foreign C-locale `toupper` of the needle,
//           then foreign `memchr` over the folded haystack with bound L
//   sealed: dispatch the sealed toupper once per haystack byte,
//           dispatch the sealed strlen once -> L (a *derived bound*),
//           dispatch the sealed toupper once for the needle,
//           dispatch the sealed memchr once with n = L
//
// The interpretation is case-insensitive "search this C string": the string's
// length is established by the sealed `strlen` rather than supplied by the caller,
// and the sealed `memchr` searches exactly that measured prefix. No foreign call
// happens in the sealed path and no Rust mirror is consulted.
//
// The composition adds no new trusted code: its implementation *is* the three
// already-sealed objects. The court records per-stage native/fallback/broken-seal
// accounting and hashes the whole chain — including the derived length — not just
// the final index.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize};
use crate::porting::composition::{CompositionTarget, Stage};
use crate::porting::dispatch::{DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_STRLEN, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The second composition: `toupper` (over the haystack and the needle), then
/// `strlen` to derive the search bound, then `memchr`.
///
/// The id is `phor:compose:...`, not `libc:...`: this is a Phorensic composition
/// over already-sealed ports, not a single foreign API surface.
pub const COMPOSITION_TOUPPER_STRLEN_MEMCHR: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_strlen_memchr:c-locale:index:v1",
    locale_contract: "C",
    stages: &[
        "libc:toupper:c-locale:u8:v1",
        "libc:strlen:c-locale:u64:v1",
        "libc:memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needle, usize n) — a buffer whose NUL terminator lies within the first n bytes",
    output_schema: "i32 index of the first match after C-locale uppercasing, within the string length, or -1",
    domain_summary: "a NUL-terminated-string corpus: folded first match at every in-string index (needle supplied in both cases), absent needles, needles that occur only after the terminator (excluded by the derived bound), the terminator as needle, unfoldable edge bytes, and an exhaustive 0..=255 needle sweep",
};

// ============================================================================
// Composition corpus
// ============================================================================

/// The second composition's corpus.
///
/// Every case is a **C string**: a NUL terminator inside `haystack[..n]`, which is
/// what makes the sealed `strlen` stage legal and gives the derived bound its
/// meaning. Cases deliberately place needle-like bytes *after* the terminator: the
/// derived bound must exclude them, so a chain that used `n` instead of the sealed
/// `strlen` result would fail.
pub fn composition_corpus() -> Vec<TestCase> {
    let mut cases: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: String, hay: &[u8], needle: u8, n: usize| {
        cases.push(TestCase::new(
            id,
            alloc::vec![
                hay.to_vec(),
                alloc::vec![needle],
                (n as u64).to_le_bytes().to_vec(),
            ],
        ));
    };

    // Distinct lowercase content bytes; C-locale `toupper` folds them to uppercase.
    const LOWER: [u8; 7] = [0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67]; // a..g
    const UPPER_PAD: u8 = 0xff; // not a letter: `toupper` leaves it unchanged

    // A — the empty string (terminator at index 0): the derived bound is 0, so no
    //     needle can match, not even NUL.
    for n in 1..=8usize {
        let mut buf = alloc::vec![0x61u8; n];
        buf[0] = 0x00;
        mk(&mut cases, format!("A.empty.n{}.nul", n), &buf, 0x00, n);
        mk(&mut cases, format!("A.empty.n{}.a", n), &buf, 0x61, n);
    }

    // B — a folded match at every in-string index. The content bytes are distinct,
    //     so the first (and only) match is exactly the index tested. Each index is
    //     tested with the needle in both cases: whichever case it is supplied in,
    //     the C-locale fold makes both sides equal.
    for len in 1..=7usize {
        let mut buf = alloc::vec![UPPER_PAD; 8];
        buf[..len].copy_from_slice(&LOWER[..len]);
        buf[len] = 0x00;
        for i in 0..len {
            let lo = LOWER[i];
            let up = LOWER[i] - 0x20;
            mk(&mut cases, format!("B.{}.{}.lo", len, i), &buf, lo, 8);
            mk(&mut cases, format!("B.{}.{}.up", len, i), &buf, up, 8);
        }
    }

    // C — an absent needle: the string is all 'x', the needle folds to 'Q'.
    for len in 0..=7usize {
        let mut buf = alloc::vec![0x78u8; len + 1]; // 'x' * len + NUL
        buf[len] = 0x00;
        mk(
            &mut cases,
            format!("C.absent.{}", len),
            &buf,
            0x71,
            buf.len(),
        );
    }

    // D — the needle occurs ONLY after the terminator. The derived bound must
    //     exclude the tail, so the folded 'a's after the NUL are never searched.
    for len in 0..=4usize {
        let mut buf = alloc::vec![0x00u8; len + 4];
        for i in 0..len {
            buf[i] = [0x62u8, 0x63, 0x64, 0x65][i]; // b..e (no 'a')
        }
        buf[len] = 0x00;
        buf[len + 1] = 0x61;
        buf[len + 2] = 0x61;
        buf[len + 3] = 0x61;
        mk(&mut cases, format!("D.tail.{}", len), &buf, 0x61, buf.len());
    }

    // E — the terminator itself as the needle: the derived bound excludes the NUL,
    //     so `memchr(folded, 0, L)` is always absent.
    for len in 0..=4usize {
        let mut buf = alloc::vec![0x61u8; len + 3];
        buf[len] = 0x00;
        buf[len + 1] = 0x61;
        buf[len + 2] = 0x61;
        mk(&mut cases, format!("E.nul.{}", len), &buf, 0x00, buf.len());
    }

    // F — exhaustive needle sweep. The haystack folds to "AB\0AB\0\x7f\x80" and the
    //     derived bound is 2, so the search covers exactly [A, B]: a1/A1 match at 0,
    //     a2/B2 (and only those) at 1, and every tail byte is unreachable.
    const F_BUF: [u8; 8] = [0x61, 0x62, 0x00, 0x61, 0x62, 0x00, 0x7f, 0x80];
    for needle in 0..=255u16 {
        mk(
            &mut cases,
            format!("F.{:02x}", needle),
            &F_BUF,
            needle as u8,
            8,
        );
    }

    // G — unfoldable edge bytes before the terminator: 0x7f/0x80 are not letters,
    //     so the fold stage must leave them alone.
    const G_BUF: [u8; 5] = [0x7f, 0x80, 0x00, 0x7f, 0x80];
    for &b in &[0x7fu8, 0x80, 0x00, 0x61] {
        mk(&mut cases, format!("G.{:02x}", b), &G_BUF, b, 5);
    }

    cases
}

// ============================================================================
// The chain
// ============================================================================

/// One composed call: the normalized intermediates, the derived bound and the
/// final index.
#[derive(Clone, Debug)]
struct ChainOutcome {
    index: Option<i32>,
    hay: Stage,
    strlen: Stage,
    needle: Stage,
    memchr: Stage,
    dispatches: u64,
    hay_norm: Vec<u8>,
    derived_len: Option<usize>,
    needle_norm: Option<u8>,
}

/// Run the sealed chain for one case.
///
/// Every stage goes through `NativeDispatcher`; the Rust mirrors are never called.
/// A fallback or a broken seal short-circuits the remaining stages (recorded as not
/// native). Stage 2's output — the sealed `strlen` length — becomes stage 4's `n`.
fn run_chain(
    dispatcher: &mut NativeDispatcher,
    hay: &[u8],
    needle: u8,
    n: usize,
    auth: &PortingAuthority,
) -> ChainOutcome {
    let mut out = ChainOutcome {
        index: None,
        hay: Stage::Native,
        strlen: Stage::Native,
        needle: Stage::Native,
        memchr: Stage::Native,
        dispatches: 0,
        hay_norm: Vec::with_capacity(hay.len()),
        derived_len: None,
        needle_norm: None,
    };

    // Stage 1 — uppercase every haystack byte with the sealed toupper.
    for &b in hay {
        match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![b]], auth) {
            Ok(o) if o.source == DispatchSource::SealedObject => {
                out.dispatches += 1;
                out.hay_norm.push(o.output.first().copied().unwrap_or(b));
            }
            Ok(_) => {
                out.hay = Stage::Fallback;
                out.strlen = Stage::Fallback;
                out.needle = Stage::Fallback;
                out.memchr = Stage::Fallback;
                return out;
            }
            Err(_) => {
                out.hay = Stage::Broken;
                out.strlen = Stage::Broken;
                out.needle = Stage::Broken;
                out.memchr = Stage::Broken;
                return out;
            }
        }
    }

    // Stage 2 — measure the folded string with the sealed strlen. This is the
    // derived bound the search stage consumes.
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
            out.needle = Stage::Fallback;
            out.memchr = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.strlen = Stage::Broken;
            out.needle = Stage::Broken;
            out.memchr = Stage::Broken;
            return out;
        }
    }

    // Stage 3 — uppercase the needle with the sealed toupper.
    match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![needle]], auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_norm = Some(o.output.first().copied().unwrap_or(needle));
        }
        Ok(_) => {
            out.needle = Stage::Fallback;
            out.memchr = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.needle = Stage::Broken;
            out.memchr = Stage::Broken;
            return out;
        }
    }

    // Stage 4 — search the folded string with the sealed memchr, bounded by the
    // length stage 2 derived (never by the caller's `n`).
    let folded_needle = out.needle_norm.unwrap_or(needle);
    let derived = out.derived_len.unwrap_or(0);
    let args = alloc::vec![
        out.hay_norm.clone(),
        alloc::vec![folded_needle],
        (derived as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch(&LIBC_MEMCHR, &args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.index = Some(decode_index(&o.output));
        }
        Ok(_) => out.memchr = Stage::Fallback,
        Err(_) => out.memchr = Stage::Broken,
    }

    out
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrlenMemchrVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed object the toupper stage actually dispatched to.
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    /// The sealed object the strlen stage actually dispatched to.
    pub strlen_object_hash: String,
    pub strlen_elf_symbol: String,
    /// The sealed object the memchr stage actually dispatched to.
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    pub cases_run: u64,
    /// Cases where the haystack-uppercasing stage was served by the sealed object.
    pub toupper_hay_native_cases: u64,
    /// Cases where the length-deriving stage was served by the sealed object.
    pub strlen_native_cases: u64,
    /// Cases where the needle-uppercasing stage was served by the sealed object.
    pub toupper_needle_native_cases: u64,
    /// Cases where the search stage was served by the sealed object.
    pub memchr_native_cases: u64,
    /// Cases where any stage fell back to the foreign implementation.
    pub fallback_cases: u64,
    /// Cases where any stage hit a broken seal (terminal, never a fallback).
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    /// Total sealed-object dispatches performed by the chain.
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the whole chain per case (stage status + normalized
    /// intermediates + the derived bound + final index), in corpus order.
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl StrlenMemchrVerdict {
    /// Every stage was sealed-native for every case, and every case matched the
    /// oracle.
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.toupper_hay_native_cases == self.cases_run
            && self.strlen_native_cases == self.cases_run
            && self.toupper_needle_native_cases == self.cases_run
            && self.memchr_native_cases == self.cases_run
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
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};strlen_object_hash={};strlen_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};cases_run={};toupper_hay_native_cases={};strlen_native_cases={};toupper_needle_native_cases={};memchr_native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
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
            self.toupper_needle_native_cases,
            self.memchr_native_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"strlen_object_hash\": \"{}\",\n  \"strlen_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"toupper_hay_native_cases\": {},\n  \"strlen_native_cases\": {},\n  \"toupper_needle_native_cases\": {},\n  \"memchr_native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
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
            self.toupper_needle_native_cases,
            self.memchr_native_cases,
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

/// SHA-256 over the whole chain: per case, the stage statuses, the normalized
/// intermediates, the **derived bound** and the final index. A fallback, a
/// different fold, or a different derived length changes the hash even if the
/// final index coincides — so the chain hash proves the *chain*, not just the
/// answer.
#[allow(clippy::type_complexity)]
fn chain_hash(
    rows: &[(
        String,
        Stage,
        Stage,
        Stage,
        Stage,
        Vec<u8>,
        Option<usize>,
        Option<u8>,
        Option<i32>,
    )],
) -> String {
    let mut buf = String::new();
    for (case_id, hay, strlen, needle, memchr, hay_norm, derived, needle_norm, index) in rows {
        buf.push_str(case_id);
        buf.push(';');
        buf.push_str(hay.as_str());
        buf.push(';');
        buf.push_str(strlen.as_str());
        buf.push(';');
        buf.push_str(needle.as_str());
        buf.push(';');
        buf.push_str(memchr.as_str());
        buf.push(';');
        buf.push_str(&hex::encode(hay_norm));
        buf.push(';');
        match derived {
            Some(l) => buf.push_str(&l.to_string()),
            None => buf.push('-'),
        }
        buf.push(';');
        match needle_norm {
            Some(b) => buf.push_str(&hex::encode([*b])),
            None => buf.push('-'),
        }
        buf.push(';');
        match index {
            Some(i) => buf.push_str(&i.to_string()),
            None => buf.push('-'),
        }
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// Replay the composition corpus through the sealed three-stage chain.
///
/// Returns the verdict plus every mismatch. `index` must contain the sealed entries
/// for every stage (the toupper, strlen and memchr ports).
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (StrlenMemchrVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut toupper_hay_native: u64 = 0;
    let mut strlen_native: u64 = 0;
    let mut toupper_needle_native: u64 = 0;
    let mut memchr_native: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows = Vec::with_capacity(traces.len());
    let mut toupper_object_hash = String::new();
    let mut toupper_elf_symbol = String::new();
    let mut strlen_object_hash = String::new();
    let mut strlen_elf_symbol = String::new();
    let mut memchr_object_hash = String::new();
    let mut memchr_elf_symbol = String::new();

    for t in traces {
        let args = t.input_args();
        let hay = args.first().cloned().unwrap_or_default();
        let needle = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
        let n = args.get(2).map(|x| decode_usize(x)).unwrap_or(0);

        let outcome = run_chain(&mut dispatcher, &hay, needle, n, auth);
        dispatches += outcome.dispatches;

        if outcome.hay == Stage::Native {
            toupper_hay_native += 1;
        }
        if outcome.strlen == Stage::Native {
            strlen_native += 1;
        }
        if outcome.needle == Stage::Native {
            toupper_needle_native += 1;
        }
        if outcome.memchr == Stage::Native {
            memchr_native += 1;
        }
        if outcome.hay == Stage::Fallback
            || outcome.strlen == Stage::Fallback
            || outcome.needle == Stage::Fallback
            || outcome.memchr == Stage::Fallback
        {
            fallback += 1;
        }
        if outcome.hay == Stage::Broken
            || outcome.strlen == Stage::Broken
            || outcome.needle == Stage::Broken
            || outcome.memchr == Stage::Broken
        {
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

        let actual_hex = outcome
            .index
            .map(|i| hex::encode(i.to_le_bytes()))
            .unwrap_or_default();

        let all_native = outcome.hay == Stage::Native
            && outcome.strlen == Stage::Native
            && outcome.needle == Stage::Native
            && outcome.memchr == Stage::Native;
        let any_broken = outcome.hay == Stage::Broken
            || outcome.strlen == Stage::Broken
            || outcome.needle == Stage::Broken
            || outcome.memchr == Stage::Broken;
        let any_fallback = outcome.hay == Stage::Fallback
            || outcome.strlen == Stage::Fallback
            || outcome.needle == Stage::Fallback
            || outcome.memchr == Stage::Fallback;
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

        rows.push((
            t.case_id.clone(),
            outcome.hay,
            outcome.strlen,
            outcome.needle,
            outcome.memchr,
            outcome.hay_norm.clone(),
            outcome.derived_len,
            outcome.needle_norm,
            outcome.index,
        ));
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0
        && fallback == 0
        && broken == 0
        && toupper_hay_native == cases_run
        && strlen_native == cases_run
        && toupper_needle_native == cases_run
        && memchr_native == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        StrlenMemchrVerdict {
            target: COMPOSITION_TOUPPER_STRLEN_MEMCHR.id.to_string(),
            stages: COMPOSITION_TOUPPER_STRLEN_MEMCHR
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
            toupper_needle_native_cases: toupper_needle_native,
            memchr_native_cases: memchr_native,
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
    pub index: Option<i32>,
    pub hay_norm: Vec<u8>,
    /// The bound the sealed `strlen` stage derived (the memchr stage's `n`).
    pub derived_len: Option<usize>,
    pub needle_norm: Option<u8>,
    pub hay_stage: &'static str,
    pub strlen_stage: &'static str,
    pub needle_stage: &'static str,
    pub memchr_stage: &'static str,
    pub dispatches: u64,
}

/// Run the sealed chain once, for a call-site demo.
pub fn run_composition_call(
    index: &SealedPortIndex,
    hay: &[u8],
    needle: u8,
    n: usize,
    auth: &PortingAuthority,
) -> CompositionCall {
    let mut dispatcher = NativeDispatcher::new(index.clone());
    let o = run_chain(&mut dispatcher, hay, needle, n, auth);
    CompositionCall {
        index: o.index,
        hay_norm: o.hay_norm,
        derived_len: o.derived_len,
        needle_norm: o.needle_norm,
        hay_stage: o.hay.as_str(),
        strlen_stage: o.strlen.as_str(),
        needle_stage: o.needle.as_str(),
        memchr_stage: o.memchr.as_str(),
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
                    COMPOSITION_TOUPPER_STRLEN_MEMCHR.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &[0u8; 4],
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
        assert_eq!(a.len(), 350);

        // Case ids are unique.
        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        // Every case is 3 args, a 1-byte needle, an 8-byte bound; the bound is
        // within the packed-word contract and the haystack carries a NUL inside it
        // (the strlen stage's precondition).
        for c in &a {
            assert_eq!(c.args.len(), 3, "case {}", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {}", c.case_id);
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

        // The groups that make the chain (not just the answer) load-bearing exist.
        assert!(ids.contains(&"A.empty.n1.nul"));
        assert!(ids.contains(&"B.3.1.up"));
        assert!(ids.contains(&"D.tail.4"));
        assert!(ids.contains(&"E.nul.2"));
        assert_eq!(ids.iter().filter(|id| id.starts_with("F.")).count(), 256);
        assert!(ids.contains(&"G.7f"));
    }

    #[test]
    fn test_empty_corpus_is_inconclusive() {
        let (v, _) =
            run_composition_court(&[], &SealedPortIndex::new(), &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconclusive);
        assert!(!v.is_sealed_eligible());
    }

    /// With an empty store every stage falls back; the court must be inconsistent
    /// with zero native cases at every stage.
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
        assert_eq!(v.strlen_native_cases, 0);
        assert_eq!(v.toupper_needle_native_cases, 0);
        assert_eq!(v.memchr_native_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    /// A sealed entry whose object does not verify must be counted as a broken
    /// seal, never as a fallback.
    #[test]
    fn test_composition_broken_seal_is_not_a_fallback() {
        use crate::porting::promotion::TrustState;
        use crate::porting::SealedPortEntry;

        let mut index = SealedPortIndex::new();
        for target in [TOUPPER, STRLEN, MEMCHR] {
            index.insert(SealedPortEntry {
                target: target.id.to_string(),
                trust: TrustState::Sealed,
                oracle_hash: String::new(),
                candidate_behavior_hash: String::new(),
                candidate_source_hash: String::new(),
                candidate_object_hash: String::from("deadbeef"),
                candidate_object_path: String::from("/nonexistent/candidate.o"),
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

    /// Without authority the store is invisible: every stage falls back.
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

    /// The chain hash must cover the derived bound, not just the final index.
    #[test]
    fn test_chain_hash_covers_the_derived_bound() {
        let rows_a = alloc::vec![(
            String::from("c"),
            Stage::Native,
            Stage::Native,
            Stage::Native,
            Stage::Native,
            alloc::vec![0x41, 0x42],
            Some(2usize),
            Some(0x41),
            Some(0),
        )];
        let mut rows_b = rows_a.clone();
        rows_b[0].6 = Some(3);
        assert_ne!(chain_hash(&rows_a), chain_hash(&rows_b));

        // Status changes are covered too.
        let mut rows_c = rows_a.clone();
        rows_c[0].1 = Stage::Fallback;
        assert_ne!(chain_hash(&rows_a), chain_hash(&rows_c));
    }
}
