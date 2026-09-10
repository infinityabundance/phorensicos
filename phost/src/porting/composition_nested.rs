// porting/composition_nested.rs — Sealed Composition Dispatch Court (nested chain)
//
// Chains 1-3 are built from sealed *leaf* objects. This chain is built from a sealed
// **composition**: its fold stage is `phor:compose:toupper_each:c-locale:u8s:v1`,
// looked up in the sealed store by id and dispatched recursively. That makes a
// composition a first-class sealed port — a named, sealed, reusable artifact the
// runtime consumes like any other, instead of fold logic living inside a runner.
//
//   phor:compose:toupper_each_strlen_memchr:c-locale:index:v1
//
//   input:  haystack, needle, n     (n = bound: a NUL lies within haystack[..n])
//   oracle: foreign C-locale `toupper` over the haystack -> H',
//           foreign `strlen` of H' -> L,
//           foreign C-locale `toupper` of the needle -> c',
//           foreign `memchr` of H' for c' bounded by L -> index or -1
//   sealed: dispatch the sealed **composition** toupper_each once over the haystack
//           -> H', dispatch the sealed strlen once -> L,
//           dispatch the same sealed **composition** once over the needle -> c',
//           dispatch the sealed memchr once with n = L
//
// The input domain and the oracle are *deliberately the same* as
// `toupper_strlen_memchr` (see `composition_strlen_memchr`): the two chains compute
// the same observable, so any difference between their evidence is attributable to
// the implementation boundary alone — leaves composed by the runner, versus a sealed
// composition the runner dispatches.
//
// Nothing here re-derives the fold: the outer runner holds no per-byte fold logic at
// all. That is the claim.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize};
use crate::porting::composition::{CompositionTarget, Stage};
use crate::porting::composition_strlen_memchr::composition_corpus;
use crate::porting::composition_toupper_each::COMPOSITION_TOUPPER_EACH;
use crate::porting::dispatch::{DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_MEMCHR, LIBC_STRLEN};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The nested composition: a sealed composition as the fold stage of a search chain.
///
/// Its `stages` list contains a *composition* id, not only leaf ids, which is what
/// makes it nested.
pub const COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_each_strlen_memchr:c-locale:index:v1",
    locale_contract: "C",
    stages: &[
        "phor:compose:toupper_each:c-locale:u8s:v1",
        "libc:strlen:c-locale:u64:v1",
        "libc:memchr:c-locale:index:v1",
    ],
    input_schema: "(u8[] haystack, u8 needle, usize n) — a buffer whose NUL terminator lies within the first n bytes",
    output_schema: "i32 index of the first match after C-locale uppercasing, within the string length, or -1",
    domain_summary: "the same NUL-terminated-string corpus as toupper_strlen_memchr (deliberately shared, so the two chains differ only in the implementation boundary), with the fold provided by the nested sealed composition",
};

/// The nested composition's corpus.
///
/// Deliberately the same input domain as `toupper_strlen_memchr`: identical corpus
/// plus identical oracle semantics means the two chains' evidence is directly
/// comparable.
pub fn nested_corpus() -> Vec<TestCase> {
    composition_corpus()
}

// ============================================================================
// The chain
// ============================================================================

/// One composed call.
#[derive(Clone, Debug)]
struct ChainOutcome {
    index: Option<i32>,
    fold_hay: Stage,
    strlen: Stage,
    fold_needle: Stage,
    memchr: Stage,
    dispatches: u64,
    hay_norm: Vec<u8>,
    derived_len: Option<usize>,
    needle_norm: Option<u8>,
}

/// Run the sealed chain for one case.
///
/// Stage 1 and stage 3 both dispatch the **sealed composition** `toupper_each`
/// through `dispatch_port`; the dispatcher resolves it in the store, checks its seal,
/// and recurses into its chip. The outer runner holds no fold loop.
fn run_chain(
    dispatcher: &mut NativeDispatcher,
    hay: &[u8],
    needle: u8,
    n: usize,
    auth: &PortingAuthority,
) -> ChainOutcome {
    let mut out = ChainOutcome {
        index: None,
        fold_hay: Stage::Native,
        strlen: Stage::Native,
        fold_needle: Stage::Native,
        memchr: Stage::Native,
        dispatches: 0,
        hay_norm: Vec::new(),
        derived_len: None,
        needle_norm: None,
    };

    // Stage 1 — fold the haystack with the sealed composition.
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
            out.fold_hay = Stage::Fallback;
            out.strlen = Stage::Fallback;
            out.fold_needle = Stage::Fallback;
            out.memchr = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.fold_hay = Stage::Broken;
            out.strlen = Stage::Broken;
            out.fold_needle = Stage::Broken;
            out.memchr = Stage::Broken;
            return out;
        }
    }

    // Stage 2 — measure the folded string with the sealed strlen.
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
            out.fold_needle = Stage::Fallback;
            out.memchr = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.strlen = Stage::Broken;
            out.fold_needle = Stage::Broken;
            out.memchr = Stage::Broken;
            return out;
        }
    }

    // Stage 3 — fold the needle with the SAME sealed composition.
    let needle_args = alloc::vec![alloc::vec![needle], 1u64.to_le_bytes().to_vec(),];
    match dispatcher.dispatch_port(COMPOSITION_TOUPPER_EACH.id, &needle_args, auth) {
        Ok(o) if o.source == DispatchSource::SealedObject => {
            out.dispatches += 1;
            out.needle_norm = o.output.first().copied();
        }
        Ok(_) => {
            out.fold_needle = Stage::Fallback;
            out.memchr = Stage::Fallback;
            return out;
        }
        Err(_) => {
            out.fold_needle = Stage::Broken;
            out.memchr = Stage::Broken;
            return out;
        }
    }

    // Stage 4 — search the folded string, bounded by the derived length.
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

/// Run this composition as a **nested stage** of another chain: `(haystack, needle,
/// n) -> index`.
pub fn run_chain_encoded(
    dispatcher: &mut NativeDispatcher,
    args: &[Vec<u8>],
    auth: &PortingAuthority,
) -> Result<Vec<u8>, crate::porting::dispatch::DispatchError> {
    use crate::porting::dispatch::DispatchError;
    let hay = args.first().cloned().unwrap_or_default();
    let needle = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
    let n = args.get(2).map(|x| decode_usize(x)).unwrap_or(0);
    let o = run_chain(dispatcher, &hay, needle, n, auth);
    match o.index {
        Some(i) => Ok(crate::porting::candidate::encode_index(i)),
        None => Err(DispatchError::SealBroken(format!(
            "{}: a nested stage was not served by a sealed object",
            COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id
        ))),
    }
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed composition the fold stages dispatched to, and its chain hash.
    pub fold_composition_id: String,
    pub fold_composition_chain_hash: String,
    /// The sealed objects the remaining stages dispatched to.
    pub strlen_object_hash: String,
    pub strlen_elf_symbol: String,
    pub memchr_object_hash: String,
    pub memchr_elf_symbol: String,
    pub cases_run: u64,
    /// Cases where the haystack fold (the nested composition) was served by the seal.
    pub fold_hay_native_cases: u64,
    pub strlen_native_cases: u64,
    /// Cases where the needle fold (the same nested composition) was served by the seal.
    pub fold_needle_native_cases: u64,
    pub memchr_native_cases: u64,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    /// Dispatches performed by *this* chain (one per nested-port dispatch).
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the outer chain per case (stage statuses, the folded
    /// intermediate, the derived bound and the index).
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl NestedVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.fold_hay_native_cases == self.cases_run
            && self.strlen_native_cases == self.cases_run
            && self.fold_needle_native_cases == self.cases_run
            && self.memchr_native_cases == self.cases_run
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.fold_composition_chain_hash.is_empty()
            && !self.strlen_object_hash.is_empty()
            && !self.memchr_object_hash.is_empty()
            && !self.chain_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};stages={};fold_composition_id={};fold_composition_chain_hash={};strlen_object_hash={};strlen_elf_symbol={};memchr_object_hash={};memchr_elf_symbol={};cases_run={};fold_hay_native_cases={};strlen_native_cases={};fold_needle_native_cases={};memchr_native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.fold_composition_id,
            self.fold_composition_chain_hash,
            self.strlen_object_hash,
            self.strlen_elf_symbol,
            self.memchr_object_hash,
            self.memchr_elf_symbol,
            self.cases_run,
            self.fold_hay_native_cases,
            self.strlen_native_cases,
            self.fold_needle_native_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"fold_composition_id\": \"{}\",\n  \"fold_composition_chain_hash\": \"{}\",\n  \"strlen_object_hash\": \"{}\",\n  \"strlen_elf_symbol\": \"{}\",\n  \"memchr_object_hash\": \"{}\",\n  \"memchr_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"fold_hay_native_cases\": {},\n  \"strlen_native_cases\": {},\n  \"fold_needle_native_cases\": {},\n  \"memchr_native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            json_escape(&self.fold_composition_id),
            self.fold_composition_chain_hash,
            self.strlen_object_hash,
            json_escape(&self.strlen_elf_symbol),
            self.memchr_object_hash,
            json_escape(&self.memchr_elf_symbol),
            self.cases_run,
            self.fold_hay_native_cases,
            self.strlen_native_cases,
            self.fold_needle_native_cases,
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

/// SHA-256 over the outer chain per case: the four stage statuses, the folded
/// intermediate, the derived bound and the index.
fn chain_hash(rows: &[NestedRow]) -> String {
    let mut buf = String::new();
    for row in rows {
        buf.push_str(&row.case_id);
        buf.push(';');
        buf.push_str(row.fold_hay.as_str());
        buf.push(';');
        buf.push_str(row.strlen.as_str());
        buf.push(';');
        buf.push_str(row.fold_needle.as_str());
        buf.push(';');
        buf.push_str(row.memchr.as_str());
        buf.push(';');
        buf.push_str(&hex::encode(&row.hay_norm));
        buf.push(';');
        match row.derived {
            Some(l) => buf.push_str(&l.to_string()),
            None => buf.push('-'),
        }
        buf.push(';');
        match row.needle_norm {
            Some(b) => buf.push_str(&hex::encode([b])),
            None => buf.push('-'),
        }
        buf.push(';');
        match row.index {
            Some(i) => buf.push_str(&i.to_string()),
            None => buf.push('-'),
        }
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// One chain-hash row.
#[derive(Clone, Debug)]
struct NestedRow {
    case_id: String,
    fold_hay: Stage,
    strlen: Stage,
    fold_needle: Stage,
    memchr: Stage,
    hay_norm: Vec<u8>,
    derived: Option<usize>,
    needle_norm: Option<u8>,
    index: Option<i32>,
}

/// Replay the composition corpus through the sealed nested chain.
///
/// `index` must contain the sealed leaf entries **and** the sealed composition entry
/// for `toupper_each`, so the nested dispatch can be resolved.
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (NestedVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut fold_hay_native: u64 = 0;
    let mut strlen_native: u64 = 0;
    let mut fold_needle_native: u64 = 0;
    let mut memchr_native: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows: Vec<NestedRow> = Vec::with_capacity(traces.len());
    let mut strlen_object_hash = String::new();
    let mut strlen_elf_symbol = String::new();
    let mut memchr_object_hash = String::new();
    let mut memchr_elf_symbol = String::new();
    let mut fold_chain_hash = String::new();

    for t in traces {
        let args = t.input_args();
        let hay = args.first().cloned().unwrap_or_default();
        let needle = args.get(1).and_then(|a| a.first()).copied().unwrap_or(0);
        let n = args.get(2).map(|x| decode_usize(x)).unwrap_or(0);

        let outcome = run_chain(&mut dispatcher, &hay, needle, n, auth);
        dispatches += outcome.dispatches;

        if outcome.fold_hay == Stage::Native {
            fold_hay_native += 1;
        }
        if outcome.strlen == Stage::Native {
            strlen_native += 1;
        }
        if outcome.fold_needle == Stage::Native {
            fold_needle_native += 1;
        }
        if outcome.memchr == Stage::Native {
            memchr_native += 1;
        }
        let stages = [
            outcome.fold_hay,
            outcome.strlen,
            outcome.fold_needle,
            outcome.memchr,
        ];
        if stages.iter().any(|s| *s == Stage::Fallback) {
            fallback += 1;
        }
        if stages.iter().any(|s| *s == Stage::Broken) {
            broken += 1;
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
        if fold_chain_hash.is_empty() {
            if let Some((h, _)) = dispatcher.sealed_binding(COMPOSITION_TOUPPER_EACH.id) {
                fold_chain_hash = h;
            }
        }

        let actual_hex = outcome
            .index
            .map(|i| hex::encode(i.to_le_bytes()))
            .unwrap_or_default();
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

        rows.push(NestedRow {
            case_id: t.case_id.clone(),
            fold_hay: outcome.fold_hay,
            strlen: outcome.strlen,
            fold_needle: outcome.fold_needle,
            memchr: outcome.memchr,
            hay_norm: outcome.hay_norm.clone(),
            derived: outcome.derived_len,
            needle_norm: outcome.needle_norm,
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
        && strlen_native == cases_run
        && fold_needle_native == cases_run
        && memchr_native == cases_run
    {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        NestedVerdict {
            target: COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id.to_string(),
            stages: COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR
                .stages
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            fold_composition_id: COMPOSITION_TOUPPER_EACH.id.to_string(),
            fold_composition_chain_hash: fold_chain_hash,
            strlen_object_hash,
            strlen_elf_symbol,
            memchr_object_hash,
            memchr_elf_symbol,
            cases_run,
            fold_hay_native_cases: fold_hay_native,
            strlen_native_cases: strlen_native,
            fold_needle_native_cases: fold_needle_native,
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
    pub derived_len: Option<usize>,
    pub needle_norm: Option<u8>,
    pub fold_hay_stage: &'static str,
    pub strlen_stage: &'static str,
    pub fold_needle_stage: &'static str,
    pub memchr_stage: &'static str,
    pub dispatches: u64,
}

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
        fold_hay_stage: o.fold_hay.as_str(),
        strlen_stage: o.strlen.as_str(),
        fold_needle_stage: o.fold_needle.as_str(),
        memchr_stage: o.memchr.as_str(),
        dispatches: o.dispatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::promotion::TrustState;
    use crate::porting::{SealedArtifact, SealedPortEntry};

    fn traces_for(cases: &[TestCase]) -> Vec<OracleTrace> {
        // Same oracle semantics as toupper_strlen_memchr, stamped with this id.
        let mut out = Vec::with_capacity(cases.len());
        for c in cases {
            let hay = &c.args[0];
            let needle = c.args[1][0];
            let n = decode_usize(&c.args[2]).min(hay.len());
            let folded: Vec<u8> = hay
                .iter()
                .map(|b| {
                    let mut v = *b;
                    if v >= b'a' && v <= b'z' {
                        v -= 0x20;
                    }
                    v
                })
                .collect();
            let derived = folded[..n].iter().position(|&b| b == 0).unwrap_or(n);
            let mut fneedle = needle;
            if fneedle >= b'a' && fneedle <= b'z' {
                fneedle -= 0x20;
            }
            let index: i32 = folded[..derived]
                .iter()
                .position(|&b| b == fneedle)
                .map(|i| i as i32)
                .unwrap_or(-1);
            out.push(OracleTrace::for_target_id(
                COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id,
                "C",
                &c.case_id,
                &c.args,
                &index.to_le_bytes(),
                "ok",
                &["compute"],
            ));
        }
        out
    }

    fn sealed_leaf(id: &str) -> SealedPortEntry {
        SealedPortEntry {
            target: id.to_string(),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::leaf_object(
                String::from("deadbeef"),
                String::from("/nonexistent/candidate.o"),
            ),
            oracle_hash: String::new(),
            candidate_behavior_hash: String::new(),
            candidate_source_hash: String::new(),
            sealed_package: String::new(),
        }
    }

    #[test]
    fn test_corpus_is_shared_with_the_leaf_chain() {
        assert_eq!(nested_corpus(), composition_corpus());
        assert_eq!(nested_corpus().len(), 350);
    }

    #[test]
    fn test_empty_corpus_is_inconclusive() {
        let (v, _) =
            run_composition_court(&[], &SealedPortIndex::new(), &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconclusive);
        assert!(!v.is_sealed_eligible());
    }

    /// Without the sealed *composition* in the store, the fold stages fall back even
    /// though the leaves are present: the nested port is a first-class dependency.
    #[test]
    fn test_without_the_sealed_composition_the_fold_falls_back() {
        let mut index = SealedPortIndex::new();
        for id in [LIBC_STRLEN.id, LIBC_MEMCHR.id] {
            index.insert(sealed_leaf(id));
        }
        let cases = nested_corpus();
        let traces = traces_for(&cases);
        let (v, _) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.fold_hay_native_cases, 0);
        assert_eq!(v.fold_needle_native_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    /// A sealed composition entry whose leaves are *not* sealed is a broken seal: the
    /// recursion refuses rather than silently falling back.
    #[test]
    fn test_nested_composition_without_sealed_leaves_is_a_broken_seal() {
        let mut index = SealedPortIndex::new();
        index.insert(SealedPortEntry {
            target: COMPOSITION_TOUPPER_EACH.id.to_string(),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::composition(
                COMPOSITION_TOUPPER_EACH.id.to_string(),
                String::from("cafe"),
                alloc::vec![crate::porting::target::LIBC_TOUPPER.id.to_string()],
            ),
            oracle_hash: String::new(),
            candidate_behavior_hash: String::new(),
            candidate_source_hash: String::new(),
            sealed_package: String::new(),
        });
        for id in [LIBC_STRLEN.id, LIBC_MEMCHR.id] {
            index.insert(sealed_leaf(id));
        }

        let cases = nested_corpus();
        let traces = traces_for(&cases);
        let (v, _) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.broken_seal_cases, v.cases_run);
        assert_eq!(v.fallback_cases, 0);
    }
}
