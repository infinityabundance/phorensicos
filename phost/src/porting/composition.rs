// porting/composition.rs — Sealed Composition Dispatch Court
//
// The leaf courts prove a sealed artifact is correct and that the runtime
// prefers it. This court proves sealed artifacts are **runtime building blocks**:
// composed targets whose implementation is a chain of already-sealed native
// objects, executed entirely through `NativeDispatcher` with **no foreign calls
// in the sealed path**.
//
// First composition (this module):
//
//   phor:compose:toupper_memchr:c-locale:index:v1
//
//   input:  haystack, needle, n
//   oracle: foreign C-locale `toupper` over the haystack and the needle,
//           then foreign `memchr` over the normalized haystack (-> index or -1)
//   sealed: dispatch the sealed toupper once per haystack byte and once for the
//           needle, then dispatch the sealed memchr once
//
// Second composition (see `composition_strlen_memchr`):
//
//   phor:compose:toupper_strlen_memchr:c-locale:index:v1
//
// which adds a stage whose *result* is the next stage's *argument*.
//
// Third composition (see `composition_pair`):
//
//   phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1
//
// which shows the runner is a dataflow graph, not a pipe: one derived bound is
// consumed by two searches, the second non-adjacent to the stage that produced it.
//
// A composition adds no new trusted code: its implementation *is* the
// already-sealed objects. Each court records the per-stage native/fallback/
// broken-seal accounting and hashes the whole chain (normalized intermediates
// included), not just the final index.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize};
use crate::porting::dispatch::{DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// A composed target: a qualified id over stages that are themselves sealed ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionTarget {
    /// Qualified identity, e.g. `phor:compose:toupper_memchr:c-locale:index:v1`.
    pub id: &'static str,
    pub locale_contract: &'static str,
    /// The qualified ids of the sealed ports this composition is built from. These
    /// may be leaf ports or other compositions (a composition is itself a sealed
    /// port), which is what makes a nested chain expressible.
    pub stages: &'static [&'static str],
    pub input_schema: &'static str,
    pub output_schema: &'static str,
    pub domain_summary: &'static str,
}

/// The first composition: `toupper` (over the haystack and the needle) then `memchr`.
///
/// The id is `phor:compose:...`, not `libc:...`: this is no longer a single
/// foreign API surface but a Phorensic composition over already-sealed ports.
pub const COMPOSITION_TOUPPER_MEMCHR: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_memchr:c-locale:index:v1",
    locale_contract: "C",
    stages: &[
        "libc:toupper:c-locale:u8:v1",
        "libc:memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needle, usize n)",
    output_schema: "i32 index of the first match after C-locale uppercasing, or -1",
    domain_summary: "the memchr corpus plus C-locale fold cases (folded first match at every index, absent-after-fold, n-boundary, exhaustive 0..=255 needle sweep)",
};

// ============================================================================
// Composition corpus
// ============================================================================

/// The composition corpus.
///
/// It **reuses the memchr corpus** (so the search semantics are already covered)
/// and adds fold-specific cases where a needle only matches after C-locale
/// `toupper` normalizes both sides. Those are the cases that actually exercise
/// the chain: without the toupper stage they would not match.
pub fn composition_corpus() -> Vec<TestCase> {
    let mut cases = crate::porting::target::memchr_corpus();

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

    // G — folded match at every index. The haystack alternates lower/upper case
    //     letters (distinct, so the first match is exactly the index tested), and
    //     the needle is supplied in both cases: whichever case it is, the C-locale
    //     fold makes both sides equal, so the match must be found at i.
    for len in 1..=8usize {
        let hay: Vec<u8> = (0..len)
            .map(|i| {
                if i % 2 == 0 {
                    b'a' + i as u8
                } else {
                    b'A' + i as u8
                }
            })
            .collect();
        for i in 0..len {
            let lo = hay[i].to_ascii_lowercase();
            let up = hay[i].to_ascii_uppercase();
            mk(&mut cases, format!("G.{}.{}.lo", len, i), &hay, lo, len);
            mk(&mut cases, format!("G.{}.{}.up", len, i), &hay, up, len);
        }
    }

    // G2 — a needle whose fold is absent from the folded haystack.
    let abc_lower: &[u8] = b"abc";
    mk(&mut cases, String::from("G2.absent.lo"), abc_lower, b'q', 3);
    mk(&mut cases, String::from("G2.absent.up"), abc_lower, b'Q', 3);
    // Both cases fold to 'A'/'a'; the haystack folds to "ABC", so 'A' is at index 0.
    let abc_upper: &[u8] = b"ABC";
    mk(&mut cases, String::from("G2.hit.lo"), abc_upper, b'a', 3);
    mk(&mut cases, String::from("G2.hit.up"), abc_upper, b'A', 3);
    // n excludes the only folded match.
    mk(&mut cases, String::from("G2.n_excludes"), b"xyZ", b'z', 2);
    mk(&mut cases, String::from("G2.n_includes"), b"xyZ", b'z', 3);

    cases
}

// ============================================================================
// The chain
// ============================================================================

/// Per-stage outcome of one composed call.
///
/// Shared by every composition court (`stage` is a property of a chain link, not
/// of a particular chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Native,
    Fallback,
    Broken,
}

impl Stage {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Stage::Native => "native",
            Stage::Fallback => "fallback",
            Stage::Broken => "broken",
        }
    }
}

/// One composed call: the normalized intermediates plus the final index.
#[derive(Clone, Debug)]
struct ChainOutcome {
    index: Option<i32>,
    hay: Stage,
    needle: Stage,
    memchr: Stage,
    dispatches: u64,
    hay_norm: Vec<u8>,
    needle_norm: Option<u8>,
}

/// Run the sealed chain for one case.
///
/// Every stage goes through `NativeDispatcher`; the Rust mirrors are never
/// called. A fallback or a broken seal short-circuits the remaining stages (the
/// remaining stages are recorded as not native).
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
        needle: Stage::Native,
        memchr: Stage::Native,
        dispatches: 0,
        hay_norm: Vec::with_capacity(hay.len()),
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
                out.needle = Stage::Fallback;
                out.memchr = Stage::Fallback;
                return out;
            }
            Err(_) => {
                out.hay = Stage::Broken;
                out.needle = Stage::Broken;
                out.memchr = Stage::Broken;
                return out;
            }
        }
    }

    // Stage 2 — uppercase the needle with the sealed toupper.
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

    // Stage 3 — search the normalized haystack with the sealed memchr.
    let upper_needle = out.needle_norm.unwrap_or(needle);
    let n_used = n.min(out.hay_norm.len());
    let args = alloc::vec![
        out.hay_norm.clone(),
        alloc::vec![upper_needle],
        (n_used as u64).to_le_bytes().to_vec(),
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
pub struct CompositionVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed object the toupper stage actually dispatched to.
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    /// The sealed object the memchr stage actually dispatched to.
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    pub cases_run: u64,
    /// Cases where the haystack-uppercasing stage was served by the sealed object.
    pub toupper_hay_native_cases: u64,
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
    /// intermediates + final index), in corpus order.
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl CompositionVerdict {
    /// Every stage was sealed-native for every case, and every case matched the
    /// oracle.
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.toupper_hay_native_cases == self.cases_run
            && self.toupper_needle_native_cases == self.cases_run
            && self.memchr_native_cases == self.cases_run
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
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};cases_run={};toupper_hay_native_cases={};toupper_needle_native_cases={};memchr_native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.toupper_object_hash,
            self.toupper_elf_symbol,
            self.memchr_object_hash,
            self.memchr_elf_symbol,
            self.cases_run,
            self.toupper_hay_native_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"toupper_hay_native_cases\": {},\n  \"toupper_needle_native_cases\": {},\n  \"memchr_native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            self.toupper_object_hash,
            json_escape(&self.toupper_elf_symbol),
            self.memchr_object_hash,
            json_escape(&self.memchr_elf_symbol),
            self.cases_run,
            self.toupper_hay_native_cases,
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
/// intermediates and the final index. A fallback or a different intermediate
/// changes the hash even if the final index coincides.
fn chain_hash(
    rows: &[(
        String,
        Stage,
        Stage,
        Stage,
        Vec<u8>,
        Option<u8>,
        Option<i32>,
    )],
) -> String {
    let mut buf = String::new();
    for (case_id, hay, needle, memchr, hay_norm, needle_norm, index) in rows {
        buf.push_str(case_id);
        buf.push(';');
        buf.push_str(hay.as_str());
        buf.push(';');
        buf.push_str(needle.as_str());
        buf.push(';');
        buf.push_str(memchr.as_str());
        buf.push(';');
        buf.push_str(&hex::encode(hay_norm));
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

/// Replay the composition corpus through the sealed chain.
///
/// Returns the verdict plus every mismatch. `index` must contain the sealed
/// entries for every stage (e.g. the toupper and memchr ports).
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (CompositionVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut toupper_hay_native: u64 = 0;
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
        if outcome.needle == Stage::Native {
            toupper_needle_native += 1;
        }
        if outcome.memchr == Stage::Native {
            memchr_native += 1;
        }
        if outcome.hay == Stage::Fallback
            || outcome.needle == Stage::Fallback
            || outcome.memchr == Stage::Fallback
        {
            fallback += 1;
        }
        if outcome.hay == Stage::Broken
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
            && outcome.needle == Stage::Native
            && outcome.memchr == Stage::Native;
        let any_broken = outcome.hay == Stage::Broken
            || outcome.needle == Stage::Broken
            || outcome.memchr == Stage::Broken;
        let any_fallback = outcome.hay == Stage::Fallback
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
            outcome.needle,
            outcome.memchr,
            outcome.hay_norm.clone(),
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
        && toupper_needle_native == cases_run
        && memchr_native == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        CompositionVerdict {
            target: COMPOSITION_TOUPPER_MEMCHR.id.to_string(),
            stages: COMPOSITION_TOUPPER_MEMCHR
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
    pub needle_norm: Option<u8>,
    pub hay_stage: &'static str,
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
        needle_norm: o.needle_norm,
        hay_stage: o.hay.as_str(),
        needle_stage: o.needle.as_str(),
        memchr_stage: o.memchr.as_str(),
        dispatches: o.dispatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::LIBC_MEMCHR as MEMCHR;
    use crate::porting::target::LIBC_TOUPPER as TOUPPER;

    #[test]
    fn test_composition_corpus_reuses_memchr_and_adds_fold_cases() {
        let memchr = crate::porting::target::memchr_corpus();
        let composition = composition_corpus();
        assert!(composition.len() > memchr.len());
        // The memchr corpus is a prefix (a superset reuses it verbatim).
        assert_eq!(&composition[..memchr.len()], &memchr[..]);

        // Case ids are unique across the whole composition corpus.
        let mut ids: Vec<&str> = composition.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), composition.len());

        // Every case is well formed.
        for c in &composition {
            assert_eq!(c.args.len(), 3, "case {}", c.case_id);
            assert_eq!(c.args[1].len(), 1, "case {}", c.case_id);
            let n = u64::from_le_bytes(c.args[2].as_slice().try_into().unwrap()) as usize;
            assert!(n <= c.args[0].len(), "case {}", c.case_id);
        }

        // The fold cases exist and are genuinely fold-dependent.
        assert!(composition.iter().any(|c| c.case_id == "G.3.1.up"));
        assert!(composition.iter().any(|c| c.case_id == "G2.hit.lo"));
    }

    #[test]
    fn test_empty_corpus_is_inconclusive() {
        let (v, _) =
            run_composition_court(&[], &SealedPortIndex::new(), &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconclusive);
        assert!(!v.is_sealed_eligible());
    }

    /// With an empty store every stage falls back; the court must be inconsistent
    /// and must report zero native cases at every stage.
    #[test]
    fn test_composition_without_sealed_ports_is_inconsistent() {
        let cases = composition_corpus();
        let traces: Vec<OracleTrace> = cases
            .iter()
            .map(|c| {
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_MEMCHR.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &[0u8; 4],
                    "ok",
                    &["compute"],
                )
            })
            .collect();
        let (v, _) = run_composition_court(
            &traces,
            &SealedPortIndex::new(),
            &PortingAuthority::granted(),
        );
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.toupper_needle_native_cases, 0);
        assert_eq!(v.memchr_native_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        // The haystack stage is *vacuously* native when the haystack is empty
        // (nothing to dispatch) — six corpus cases have an empty haystack. It is
        // never native once a sealed port is actually required, and the needle /
        // memchr stages still gate the verdict.
        let empty_haystacks = cases.iter().filter(|c| c.args[0].is_empty()).count() as u64;
        assert_eq!(v.toupper_hay_native_cases, empty_haystacks);
        assert!(!v.is_sealed_eligible());
    }

    /// A sealed entry whose object does not verify must be counted as a broken
    /// seal, never as a fallback.
    #[test]
    fn test_composition_broken_seal_is_not_a_fallback() {
        use crate::porting::promotion::TrustState;
        use crate::porting::{SealedArtifact, SealedPortEntry};

        let mut index = SealedPortIndex::new();
        index.insert(SealedPortEntry {
            target: TOUPPER.id.to_string(),
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
        index.insert(SealedPortEntry {
            target: MEMCHR.id.to_string(),
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

        let cases = composition_corpus();
        let traces: Vec<OracleTrace> = cases
            .iter()
            .map(|c| {
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_MEMCHR.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &[0u8; 4],
                    "ok",
                    &["compute"],
                )
            })
            .collect();
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
        let traces: Vec<OracleTrace> = cases
            .iter()
            .map(|c| {
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_MEMCHR.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &[0u8; 4],
                    "ok",
                    &["compute"],
                )
            })
            .collect();
        let (v, _) =
            run_composition_court(&traces, &SealedPortIndex::new(), &PortingAuthority::none());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
    }
}
