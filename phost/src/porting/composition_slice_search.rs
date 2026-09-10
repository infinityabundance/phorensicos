// porting/composition_slice_search.rs — Sealed Composition Dispatch Court (seventh chain)
//
// The first six chains proved sealed ports can be charted: a map into a search, a
// derived bound consumed twice, a composition as a reusable port, and a derived value
// that selects a *buffer*. In every one of them the buffer a stage reads is either a
// caller argument or a slice of a **leaf** fold performed by the runner.
//
// This chain adds the last piece of the dataflow vocabulary: a **sealed composition
// consumes a buffer that another sealed composition's output selected**.
//
//   phor:compose:toupper_each_slice_search:c-locale:index:v1
//
//   input:  haystack, needleA, needleB, n
//   oracle: foreign C-locale `toupper` over the haystack -> H',
//           foreign `memchr` of H' for folded needleA, bounded by n -> i,
//           and only if that matched, foreign `memchr` of H'[i..] for folded
//           needleB, bounded by n - i -> j, reporting i + j or -1.
//   sealed: dispatch the sealed **composition** `toupper_each` over the haystack -> H',
//           dispatch the same sealed **composition** over needleA -> a',
//           dispatch the sealed `memchr` over H' for a' -> origin i,
//           **slice the ORIGINAL haystack at i** -> S (unfolded),
//           dispatch the sealed **composition** `toupper_memchr` over S for needleB,
//           bounded by n - i -> j, and report i + j or -1.
//
// Two things make this a genuinely new shape rather than a re-labelling of the sixth
// chain:
//
//   1. **A composition consumes a derived buffer.** The buffer the second half of the
//      chain reads is not a caller argument and not a runner-managed fold: it is the
//      slice selected by the origin the first half derived. It is handed to
//      `toupper_memchr` as that composition's *haystack*, so a composition consumes
//      another composition's output.
//   2. **The consumer must fold the slice itself.** The slice is taken from the
//      *unfolded* haystack, so `toupper_memchr` has real work to do. A chain that
//      handed the raw slice to a bare `memchr` with a folded needle would miss every
//      lowercase match in the suffix — the corpus is built so that happens (the
//      `A.slice.*` group), which is what makes the composition load-bearing rather
//      than decorative.
//
// The input domain and the oracle are *deliberately the same* as
// `toupper_memchr_suffix` (see `composition_suffix`): the two chains compute the same
// observable, so any difference between their evidence is attributable to the
// implementation boundary alone — a runner-driven leaf fold and a leaf slice-search,
// versus a nested composition fold and a nested composition consuming the slice.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize, encode_index};
use crate::porting::composition::{CompositionTarget, Stage, COMPOSITION_TOUPPER_MEMCHR};
use crate::porting::composition_toupper_each::COMPOSITION_TOUPPER_EACH;
use crate::porting::dispatch::{DispatchError, DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The seventh composition: a nested composition produces the buffer, a leaf derives
/// the origin, a nested composition consumes the derived slice.
pub const COMPOSITION_TOUPPER_EACH_SLICE_SEARCH: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_each_slice_search:c-locale:index:v1",
    locale_contract: "C",
    stages: &[
        "phor:compose:toupper_each:c-locale:u8s:v1",
        "libc:memchr:c-locale:index:v1",
        "phor:compose:toupper_memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needleA, u8 needleB, usize n)",
    output_schema: "i32 absolute index of the first C-locale-folded match of needleB in the folded haystack sliced at needleA's first match, or -1",
    domain_summary: "the same slice corpus as toupper_memchr_suffix (deliberately shared, so the two chains differ only in the implementation boundary), with the fold provided by the nested sealed composition toupper_each and the derived slice consumed by the nested sealed composition toupper_memchr",
};

/// The seventh composition's corpus.
///
/// Deliberately the same input domain as `toupper_memchr_suffix`: identical corpus
/// plus identical oracle semantics means the two chains' evidence is directly
/// comparable, exactly as `toupper_each_strlen_memchr` and `toupper_strlen_memchr` are.
pub fn composition_corpus() -> Vec<TestCase> {
    crate::porting::composition_suffix::composition_corpus()
}

// ============================================================================
// The chain
// ============================================================================

/// One composed call: the folded view, the derived origin, the slice handed to the
/// consumer composition, the suffix offset and the absolute result.
#[derive(Clone, Debug)]
struct ChainOutcome {
    /// The absolute index into the folded haystack, or -1.
    index: Option<i32>,
    /// The derived origin (-1 when needleA is absent).
    origin: Option<i32>,
    /// The suffix-relative offset, when the consumer composition actually ran.
    suffix_index: Option<i32>,
    fold_hay: Stage,
    fold_needle_a: Stage,
    memchr_a: Stage,
    /// `None` when the consumer composition was never dispatched (no origin).
    search: Option<Stage>,
    dispatches: u64,
    hay_norm: Vec<u8>,
    needle_a_norm: Option<u8>,
    /// The buffer the derived origin selected — handed to the consumer composition.
    slice: Vec<u8>,
}

/// Run the sealed chain for one case.
///
/// Every stage goes through `NativeDispatcher`; the Rust mirrors are never called.
/// The fold is the sealed **composition** `toupper_each`, and the suffix search is the
/// sealed **composition** `toupper_memchr`, which consumes the slice this runner
/// derived from the origin.
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
        fold_hay: Stage::Native,
        fold_needle_a: Stage::Native,
        memchr_a: Stage::Native,
        search: None,
        dispatches: 0,
        hay_norm: Vec::new(),
        needle_a_norm: None,
        slice: Vec::new(),
    };

    // Every remaining stage is unreachable once a link fails; mark them all.
    let fail_all = |out: &mut ChainOutcome, stage: Stage| {
        out.fold_hay = stage;
        out.fold_needle_a = stage;
        out.memchr_a = stage;
        out.search = Some(stage);
    };

    // Stage 1 — fold the haystack with the sealed composition `toupper_each`.
    let fold_args = alloc::vec![
        hay.to_vec(),
        (n.min(hay.len()) as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch_port(COMPOSITION_TOUPPER_EACH.id, &fold_args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.hay_norm = o.output;
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

    // Stage 2 — fold needleA with the same sealed composition.
    let a_args = alloc::vec![alloc::vec![needle_a], 1u64.to_le_bytes().to_vec()];
    match dispatcher.dispatch_port(COMPOSITION_TOUPPER_EACH.id, &a_args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_a_norm = o.output.first().copied().or(Some(needle_a));
        }
        Ok(_) => {
            out.fold_needle_a = Stage::Fallback;
            out.memchr_a = Stage::Fallback;
            out.search = Some(Stage::Fallback);
            return out;
        }
        Err(_) => {
            out.fold_needle_a = Stage::Broken;
            out.memchr_a = Stage::Broken;
            out.search = Some(Stage::Broken);
            return out;
        }
    }

    // Stage 3 — derive the origin in the folded buffer with the sealed leaf `memchr`.
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
            out.search = Some(Stage::Fallback);
            return out;
        }
        Err(_) => {
            out.memchr_a = Stage::Broken;
            out.search = Some(Stage::Broken);
            return out;
        }
    }

    let origin = out.origin.unwrap_or(-1);
    if origin < 0 {
        // No origin: the slice does not exist, so the consumer composition is not
        // dispatched at all. The observable is -1.
        out.index = Some(-1);
        return out;
    }

    // Stage 4 — **the derived value selects a buffer**. The slice is taken from the
    // ORIGINAL haystack, not from the folded view, so the consumer composition has to
    // fold it itself. A chain that skipped that fold would miss lowercase bytes.
    let origin_u = origin as usize;
    let suffix_len = window.saturating_sub(origin_u);
    if suffix_len == 0 || origin_u + suffix_len > hay.len() {
        // Defensive: a memchr result is always inside the window, so this cannot
        // happen for a correct sealed memchr. Fail closed rather than slice.
        out.search = Some(Stage::Broken);
        return out;
    }
    out.slice = hay[origin_u..origin_u + suffix_len].to_vec();

    // Stage 5 — the sealed composition `toupper_memchr` consumes the derived buffer:
    // it folds the slice and the needle itself, then searches. The runner does no
    // fold of its own here.
    let search_args = alloc::vec![
        out.slice.clone(),
        alloc::vec![needle_b],
        (suffix_len as u64).to_le_bytes().to_vec(),
    ];
    match dispatcher.dispatch_port(COMPOSITION_TOUPPER_MEMCHR.id, &search_args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            let j = decode_index(&o.output);
            out.search = Some(Stage::Native);
            out.suffix_index = Some(j);
            out.index = Some(if j < 0 { -1 } else { origin + j });
        }
        Ok(_) => out.search = Some(Stage::Fallback),
        Err(_) => out.search = Some(Stage::Broken),
    }

    out
}

/// Run this composition as a **stage of another chain** (or from a call site):
/// `(haystack, needleA, needleB, n) -> i32 absolute index` (little-endian).
///
/// Every stage that ran must have been served by a sealed object; a fallback or a
/// broken seal inside the chain is reported as a broken seal, never as a silent
/// foreign fallback — a sealed composition that cannot run natively is unusable.
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

    if out.fold_hay != Stage::Native
        || out.fold_needle_a != Stage::Native
        || out.memchr_a != Stage::Native
        || out.search != Some(Stage::Native)
    {
        return Err(DispatchError::SealBroken(format!(
            "{}: a stage was not served by a sealed object",
            COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id
        )));
    }
    match out.index {
        Some(i) => Ok(encode_index(i)),
        None => Err(DispatchError::SealBroken(format!(
            "{}: the chain produced no observable",
            COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id
        ))),
    }
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceSearchVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed leaf objects the chain ultimately dispatched to (the `toupper` and
    /// `memchr` leaves the nested compositions resolve).
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    /// The sealed **composition** whose buffer the chain sliced.
    pub fold_composition_id: String,
    pub fold_composition_chain_hash: String,
    /// The sealed **composition** that consumed the derived slice.
    pub search_composition_id: String,
    pub search_composition_chain_hash: String,
    pub cases_run: u64,
    /// Cases where the haystack fold (the nested composition) was served natively.
    pub fold_hay_native_cases: u64,
    /// Cases where the needleA fold (the nested composition) was served natively.
    pub fold_needle_a_native_cases: u64,
    /// Cases where the origin-searching leaf `memchr` was served natively.
    pub memchr_a_native_cases: u64,
    /// Cases where the slice-consuming composition ran and was served natively.
    pub search_native_cases: u64,
    /// Cases where needleA was absent, so no slice existed and the consumer
    /// composition was never dispatched. Counted here — not as a native run of a
    /// stage that did not happen.
    pub search_not_reached_cases: u64,
    /// Cases where a stage fell back to the foreign implementation.
    pub fallback_cases: u64,
    /// Cases where a sealed entry failed verification.
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the whole chain per case: the stage statuses, the folded
    /// haystack, the **derived origin**, the **slice handed to the consumer
    /// composition**, the suffix offset and the final index.
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl SliceSearchVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.fold_hay_native_cases == self.cases_run
            && self.fold_needle_a_native_cases == self.cases_run
            && self.memchr_a_native_cases == self.cases_run
            // Every case either ran the consumer composition natively, or had no
            // origin at all. No case may leave a dispatched stage unaccounted for.
            && self.search_native_cases + self.search_not_reached_cases == self.cases_run
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.toupper_object_hash.is_empty()
            && !self.memchr_object_hash.is_empty()
            && !self.fold_composition_chain_hash.is_empty()
            && !self.search_composition_chain_hash.is_empty()
            && !self.chain_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};fold_composition_id={};fold_composition_chain_hash={};search_composition_id={};search_composition_chain_hash={};cases_run={};fold_hay_native_cases={};fold_needle_a_native_cases={};memchr_a_native_cases={};search_native_cases={};search_not_reached_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.toupper_object_hash,
            self.toupper_elf_symbol,
            self.memchr_object_hash,
            self.memchr_elf_symbol,
            self.fold_composition_id,
            self.fold_composition_chain_hash,
            self.search_composition_id,
            self.search_composition_chain_hash,
            self.cases_run,
            self.fold_hay_native_cases,
            self.fold_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.search_native_cases,
            self.search_not_reached_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"fold_composition_id\": \"{}\",\n  \"fold_composition_chain_hash\": \"{}\",\n  \"search_composition_id\": \"{}\",\n  \"search_composition_chain_hash\": \"{}\",\n  \"cases_run\": {},\n  \"fold_hay_native_cases\": {},\n  \"fold_needle_a_native_cases\": {},\n  \"memchr_a_native_cases\": {},\n  \"search_native_cases\": {},\n  \"search_not_reached_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            self.toupper_object_hash,
            json_escape(&self.toupper_elf_symbol),
            self.memchr_object_hash,
            json_escape(&self.memchr_elf_symbol),
            json_escape(&self.fold_composition_id),
            self.fold_composition_chain_hash,
            json_escape(&self.search_composition_id),
            self.search_composition_chain_hash,
            self.cases_run,
            self.fold_hay_native_cases,
            self.fold_needle_a_native_cases,
            self.memchr_a_native_cases,
            self.search_native_cases,
            self.search_not_reached_cases,
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
struct SliceRow {
    case_id: String,
    fold_hay: Stage,
    fold_needle_a: Stage,
    memchr_a: Stage,
    search: Option<Stage>,
    hay_norm: Vec<u8>,
    needle_a_norm: Option<u8>,
    slice: Vec<u8>,
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

/// SHA-256 over the whole chain: per case, the stage statuses, the folded haystack,
/// the **derived origin**, the **slice the consumer composition received**, the
/// suffix offset and the final index. A chain that skipped the slice but coincided
/// on the answer still differs here, because the origin, the slice and the suffix
/// offset are hashed.
fn chain_hash(rows: &[SliceRow]) -> String {
    let mut buf = String::new();
    for row in rows {
        buf.push_str(&row.case_id);
        buf.push(';');
        buf.push_str(row.fold_hay.as_str());
        buf.push(';');
        buf.push_str(row.fold_needle_a.as_str());
        buf.push(';');
        buf.push_str(row.memchr_a.as_str());
        buf.push(';');
        match row.search {
            Some(s) => buf.push_str(s.as_str()),
            None => buf.push('-'),
        }
        buf.push(';');
        buf.push_str(&hex::encode(&row.hay_norm));
        buf.push(';');
        push_byte(&mut buf, row.needle_a_norm);
        buf.push(';');
        buf.push_str(&hex::encode(&row.slice));
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
/// for every leaf (toupper, memchr) and every nested composition (toupper_each,
/// toupper_memchr) the chain dispatches.
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (SliceSearchVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut fold_hay_native: u64 = 0;
    let mut fold_needle_a_native: u64 = 0;
    let mut memchr_a_native: u64 = 0;
    let mut search_native: u64 = 0;
    let mut search_not_reached: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows: Vec<SliceRow> = Vec::with_capacity(traces.len());
    let mut toupper_object_hash = String::new();
    let mut toupper_elf_symbol = String::new();
    let mut memchr_object_hash = String::new();
    let mut memchr_elf_symbol = String::new();
    let mut fold_composition_chain_hash = String::new();
    let mut search_composition_chain_hash = String::new();

    for t in traces {
        let args = t.input_args();
        let hay = args.first().cloned().unwrap_or_default();
        let needle_a = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
        let needle_b = args.get(2).and_then(|a| a.first()).copied().unwrap_or(0);
        let n = args.get(3).map(|x| decode_usize(x)).unwrap_or(0);

        let outcome = run_chain(&mut dispatcher, &hay, needle_a, needle_b, n, auth);
        dispatches += outcome.dispatches;

        for (native, stage) in [
            (&mut fold_hay_native, outcome.fold_hay),
            (&mut fold_needle_a_native, outcome.fold_needle_a),
            (&mut memchr_a_native, outcome.memchr_a),
        ] {
            if stage == Stage::Native {
                *native += 1;
            }
        }
        match outcome.search {
            None => search_not_reached += 1,
            Some(Stage::Native) => search_native += 1,
            // A fallback or a broken seal in the consumer composition is counted below.
            Some(_) => {}
        }

        let ran = [outcome.fold_hay, outcome.fold_needle_a, outcome.memchr_a];
        let s_stage = outcome.search;
        let any_fallback =
            ran.iter().any(|s| *s == Stage::Fallback) || s_stage == Some(Stage::Fallback);
        let any_broken = ran.iter().any(|s| *s == Stage::Broken) || s_stage == Some(Stage::Broken);
        if any_fallback {
            fallback += 1;
        }
        if any_broken {
            broken += 1;
        }

        // Record the concrete sealed artifacts that served these cases: the leaves the
        // nested compositions resolved, and the nested compositions themselves (their
        // chain hashes) — a seal of a seal.
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
        if fold_composition_chain_hash.is_empty() {
            if let Some((h, _)) = dispatcher.sealed_binding(COMPOSITION_TOUPPER_EACH.id) {
                fold_composition_chain_hash = h;
            }
        }
        if search_composition_chain_hash.is_empty() {
            if let Some((h, _)) = dispatcher.sealed_binding(COMPOSITION_TOUPPER_MEMCHR.id) {
                search_composition_chain_hash = h;
            }
        }

        let actual_hex = match outcome.index {
            Some(i) => hex::encode(encode_index(i)),
            None => String::new(),
        };

        let all_native = ran.iter().all(|s| *s == Stage::Native)
            && matches!(s_stage, None | Some(Stage::Native));
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

        rows.push(SliceRow {
            case_id: t.case_id.clone(),
            fold_hay: outcome.fold_hay,
            fold_needle_a: outcome.fold_needle_a,
            memchr_a: outcome.memchr_a,
            search: outcome.search,
            hay_norm: outcome.hay_norm.clone(),
            needle_a_norm: outcome.needle_a_norm,
            slice: outcome.slice.clone(),
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
        && fold_hay_native == cases_run
        && fold_needle_a_native == cases_run
        && memchr_a_native == cases_run
        && search_native + search_not_reached == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        SliceSearchVerdict {
            target: COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id.to_string(),
            stages: COMPOSITION_TOUPPER_EACH_SLICE_SEARCH
                .stages
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            toupper_object_hash,
            toupper_elf_symbol,
            memchr_object_hash,
            memchr_elf_symbol,
            fold_composition_id: COMPOSITION_TOUPPER_EACH.id.to_string(),
            fold_composition_chain_hash,
            search_composition_id: COMPOSITION_TOUPPER_MEMCHR.id.to_string(),
            search_composition_chain_hash,
            cases_run,
            fold_hay_native_cases: fold_hay_native,
            fold_needle_a_native_cases: fold_needle_a_native,
            memchr_a_native_cases: memchr_a_native,
            search_native_cases: search_native,
            search_not_reached_cases: search_not_reached,
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
    /// The buffer the derived origin selected and the consumer composition received.
    pub slice: Vec<u8>,
    pub fold_hay_stage: &'static str,
    pub memchr_a_stage: &'static str,
    pub search_stage: &'static str,
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
        slice: o.slice,
        fold_hay_stage: o.fold_hay.as_str(),
        memchr_a_stage: o.memchr_a.as_str(),
        // "-" when there was no origin, so the consumer composition never ran.
        search_stage: o.search.map(|s| s.as_str()).unwrap_or("-"),
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
                    COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id,
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

    /// The plausible wrong implementation this chain exists to falsify: the derived
    /// slice is handed to a bare search **without** the consumer composition folding
    /// it. It agrees with the oracle only when the suffix happens to contain no
    /// lowercase byte that must fold.
    fn unfolded_slice_search(hay: &[u8], a: u8, b: u8, n: usize) -> i32 {
        let folded: Vec<u8> = hay.iter().map(|&x| fold(x)).collect();
        let window = n.min(folded.len());
        let origin = match window_search(&folded, fold(a), window) {
            Some(i) => i,
            None => return -1,
        };
        let suffix_len = window - origin;
        if suffix_len == 0 {
            return -1;
        }
        match window_search(&hay[origin..origin + suffix_len], fold(b), suffix_len) {
            Some(j) => (origin + j) as i32,
            None => -1,
        }
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
            // the needle is supplied uppercase; the fold is not one-sided
            case("folded_needle", b"baxb", 0x41, 0x42, 4),
        ];
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        let got: Vec<String> = traces.iter().map(|t| t.output_hex.clone()).collect();
        assert_eq!(
            got,
            alloc::vec!["03000000", "ffffffff", "ffffffff", "03000000", "03000000"]
        );
    }

    /// The chain's corpus is deliberately the sixth chain's, so the two must produce
    /// the same oracle over the same cases.
    #[test]
    fn test_corpus_and_oracle_match_the_suffix_chain() {
        let cases = composition_corpus();
        assert_eq!(
            cases,
            crate::porting::composition_suffix::composition_corpus()
        );
        assert_eq!(cases.len(), 688);

        let here =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        let there =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        assert_eq!(here, there);
        assert_eq!(combined_oracle_hash(&here), combined_oracle_hash(&there));
    }

    /// The corpus must contain cases where the plausible wrong implementation
    /// (a search of the *unfolded* slice) disagrees with the oracle. Without these the
    /// court could pass a chain that never folded the derived buffer.
    #[test]
    fn test_corpus_contains_cases_an_unfolded_slice_chain_would_fail() {
        let cases = composition_corpus();
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();

        let mut differ = 0usize;
        let mut slice_differ = 0usize;
        for (c, t) in cases.iter().zip(traces.iter()) {
            let expected = decode_index(&hex::decode(&t.output_hex).unwrap());
            let n = decode_usize(&c.args[3]);
            let wrong = unfolded_slice_search(&c.args[0], c.args[1][0], c.args[2][0], n);
            if wrong != expected {
                differ += 1;
                if c.case_id.starts_with("A.") {
                    slice_differ += 1;
                }
            }
        }
        assert!(
            differ >= 50,
            "only {} cases falsify the unfolded-slice chain",
            differ
        );
        assert!(
            slice_differ >= 50,
            "the A.slice group does not exercise the consumer's own fold"
        );
    }

    /// The bound-only shape (searching the caller's window instead of the slice) must
    /// still be falsified — the sixth chain's discriminator, preserved here.
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
        assert_eq!(v.fold_hay_native_cases, 0);
        assert_eq!(v.memchr_a_native_cases, 0);
        assert_eq!(v.search_native_cases, 0);
        assert_eq!(v.search_not_reached_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    #[test]
    fn test_composition_broken_seal_is_not_a_fallback() {
        let mut index = SealedPortIndex::new();
        // The nested composition entries must be present so the chain actually
        // attempts a dispatch; the leaves they resolve are the broken seals.
        for (id, leaves) in [
            (
                "phor:compose:toupper_each:c-locale:u8s:v1",
                alloc::vec!["libc:toupper:c-locale:u8:v1"],
            ),
            (
                "phor:compose:toupper_memchr:c-locale:index:v1",
                alloc::vec![
                    "libc:toupper:c-locale:u8:v1",
                    "libc:memchr:c-locale:index:v1"
                ],
            ),
        ] {
            index.insert(SealedPortEntry {
                target: id.to_string(),
                trust: TrustState::Sealed,
                artifact: SealedArtifact::composition(
                    id.to_string(),
                    String::from("deadbeef"),
                    leaves.into_iter().map(|s| s.to_string()).collect(),
                ),
                oracle_hash: String::new(),
                candidate_behavior_hash: String::new(),
                candidate_source_hash: String::new(),
                sealed_package: String::new(),
            });
        }
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

    /// The whole point, end to end against the committed store: the nested composition
    /// `toupper_each` produces the buffer, a leaf derives the origin, and the nested
    /// composition `toupper_memchr` consumes the derived slice and folds it itself.
    #[test]
    fn test_the_consumer_composition_folds_the_derived_slice() {
        let index = store::load_default().expect("committed store loads");
        let auth = PortingAuthority::granted();

        // "baxb" folds to "BAXB"; needleA 'a' -> origin 1; the slice handed to the
        // consumer composition is the *unfolded* "axb", which it must fold to "AXB"
        // before finding 'b' -> offset 2 -> absolute 3.
        let c = run_composition_call(&index, b"baxb", 0x61, 0x62, 4, &auth);
        assert_eq!(c.origin, Some(1));
        assert_eq!(
            c.slice,
            b"axb".to_vec(),
            "the consumer received the raw slice"
        );
        assert_eq!(c.suffix_index, Some(2));
        assert_eq!(c.index, Some(3));
        assert_eq!(c.fold_hay_stage, "native");
        assert_eq!(c.search_stage, "native");
        // The wrong implementation (no fold of the slice) would report -1 here.
        assert_eq!(unfolded_slice_search(b"baxb", 0x61, 0x62, 4), -1);

        // No origin: the consumer composition never ran, and that is reported as such.
        let c = run_composition_call(&index, b"bxxb", 0x61, 0x62, 4, &auth);
        assert_eq!(c.origin, Some(-1));
        assert_eq!(c.index, Some(-1));
        assert_eq!(c.search_stage, "-");
    }

    /// The consumer composition is data-dependent: the corpus must exercise both the
    /// run and the not-reached path, and the court must account for both.
    #[test]
    fn test_the_consumer_composition_is_data_dependent() {
        let index = store::load_default().expect("committed store loads");
        let cases = composition_corpus();
        let traces =
            dialect_cage::observe_composition_suffix(&cases, &PortingAuthority::granted()).unwrap();
        let (v, mismatches) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert!(mismatches.is_empty(), "{:?}", mismatches.first());
        assert!(v.is_sealed_eligible());
        assert_eq!(v.cases_run, cases.len() as u64);
        assert_eq!(v.memchr_a_native_cases, v.cases_run);
        assert!(
            v.search_native_cases > 0,
            "the consumer composition never ran"
        );
        assert!(
            v.search_not_reached_cases > 0,
            "no case exercised the absent-origin path"
        );
        assert_eq!(
            v.search_native_cases + v.search_not_reached_cases,
            v.cases_run
        );
        assert_eq!(v.fallback_cases, 0);
        assert_eq!(v.broken_seal_cases, 0);
        // The nested composition seals are recorded: a seal of a seal.
        assert_eq!(v.fold_composition_chain_hash.len(), 64);
        assert_eq!(v.search_composition_chain_hash.len(), 64);
        assert_eq!(
            v.fold_composition_id,
            "phor:compose:toupper_each:c-locale:u8s:v1"
        );
        assert_eq!(
            v.search_composition_id,
            "phor:compose:toupper_memchr:c-locale:index:v1"
        );
    }

    /// The chain hash must cover the derived origin, the slice and the suffix offset,
    /// not just the final index.
    #[test]
    fn test_chain_hash_covers_the_origin_the_slice_and_the_suffix_offset() {
        let base = || SliceRow {
            case_id: String::from("c"),
            fold_hay: Stage::Native,
            fold_needle_a: Stage::Native,
            memchr_a: Stage::Native,
            search: Some(Stage::Native),
            hay_norm: alloc::vec![0x42, 0x41, 0x42],
            needle_a_norm: Some(0x41),
            slice: alloc::vec![0x61, 0x78, 0x62],
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
        changed.slice = alloc::vec![0x61, 0x78];
        assert_ne!(chain_hash(&[changed]), h, "the slice is not hashed");

        let mut changed = base();
        changed.suffix_index = Some(0);
        assert_ne!(chain_hash(&[changed]), h, "suffix offset is not hashed");

        let mut changed = base();
        changed.search = None;
        assert_ne!(chain_hash(&[changed]), h, "not-reached is not hashed");
    }
}
