// porting/composition_toupper_each.rs — Sealed Composition Dispatch Court (map chain)
//
// This composition is the first **buffer-to-buffer** sealed port: it applies the
// sealed `toupper` leaf to every byte of a buffer. Its output is not a scalar but the
// folded bytes themselves, which is what makes it usable as a *stage* of another
// chain: a nested chain can dispatch it once and use the resulting buffer as the
// input of its later stages.
//
//   phor:compose:toupper_each:c-locale:u8s:v1
//
//   input:  bytes, n           (fold the first n bytes)
//   oracle: foreign C-locale `toupper` over each of the first n bytes
//   sealed: dispatch the sealed toupper once per byte
//
// Stage ABI when used as a nested stage: `(u8[] bytes, usize n) -> u8[] folded`.
// A nested chain's dispatcher looks this port up in the sealed store by id, checks
// its seal, and recurses into this chain — so the *fold* becomes a named, sealed,
// reusable artifact instead of inline runner code.
//
// The composition adds no new trusted code: its implementation *is* the sealed
// `toupper` object.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::decode_usize;
use crate::porting::composition::{CompositionTarget, Stage};
use crate::porting::dispatch::{DispatchError, DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{TestCase, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex, PortingAuthority, SealedPortIndex};

/// The map composition: `toupper` applied to every byte of a buffer.
///
/// The id is `phor:compose:...`, not `libc:...`: it is a Phorensic composition over
/// an already-sealed port.
pub const COMPOSITION_TOUPPER_EACH: CompositionTarget = CompositionTarget {
    id: "phor:compose:toupper_each:c-locale:u8s:v1",
    locale_contract: "C",
    stages: &["libc:toupper:c-locale:u8:v1"],
    input_schema: "(u8[] bytes, usize n) — the first n bytes are folded",
    output_schema: "u8[] the folded bytes (n bytes)",
    domain_summary: "a bounded buffer corpus: lowercase runs of every length 1..=8, an exhaustive 0..=255 single-byte sweep, unfoldable edge bytes, and a mixed-case buffer",
};

// ============================================================================
// Composition corpus
// ============================================================================

/// The map composition's corpus: every byte value once, plus multi-byte buffers.
pub fn composition_corpus() -> Vec<TestCase> {
    let mut cases: Vec<TestCase> = Vec::new();

    let mk = |cases: &mut Vec<TestCase>, id: String, buf: &[u8], n: usize| {
        cases.push(TestCase::new(
            id,
            alloc::vec![buf.to_vec(), (n as u64).to_le_bytes().to_vec()],
        ));
    };

    // A — lowercase runs of every length, so the fold has to touch every byte.
    for len in 1..=8usize {
        let buf: Vec<u8> = (0..len).map(|i| 0x61 + (i % 26) as u8).collect();
        mk(&mut cases, format!("A.lower.{}", len), &buf, len);
    }

    // B — exhaustive single-byte sweep: every value passes through toupper once.
    for b in 0..=255u16 {
        mk(&mut cases, format!("B.{:02x}", b), &[b as u8], 1);
    }

    // C — unfoldable edge bytes and a digit, mixed.
    const C_BUF: [u8; 8] = [0x00, 0x7f, 0x80, 0xfe, 0xff, 0x41, 0x61, 0x39];
    mk(&mut cases, String::from("C.edges"), &C_BUF, C_BUF.len());

    // D — mixed case, already uppercase, and non-letters.
    const D_BUF: [u8; 8] = [0x61, 0x5a, 0x42, 0x7a, 0x30, 0x7f, 0x41, 0x62];
    mk(&mut cases, String::from("D.mixed"), &D_BUF, D_BUF.len());

    // E — a prefix shorter than the buffer: only the first n bytes are folded.
    const E_BUF: [u8; 6] = [0x61, 0x62, 0x63, 0x64, 0x65, 0x66];
    mk(&mut cases, String::from("E.prefix.n3"), &E_BUF, 3);

    cases
}

// ============================================================================
// The chain
// ============================================================================

/// One folded call.
#[derive(Clone, Debug)]
struct FoldOutcome {
    folded: Vec<u8>,
    stage: Stage,
    dispatches: u64,
}

/// Fold the first `n` bytes through the sealed toupper.
fn run_chain(
    dispatcher: &mut NativeDispatcher,
    bytes: &[u8],
    _n: usize,
    auth: &PortingAuthority,
) -> FoldOutcome {
    let mut out = FoldOutcome {
        folded: Vec::with_capacity(bytes.len()),
        stage: Stage::Native,
        dispatches: 0,
    };

    for &b in bytes {
        match dispatcher.dispatch(&LIBC_TOUPPER, &alloc::vec![alloc::vec![b]], auth) {
            Ok(o) if o.source == DispatchSource::SealedObject => {
                out.dispatches += 1;
                out.folded.push(o.output.first().copied().unwrap_or(b));
            }
            Ok(_) => {
                out.stage = Stage::Fallback;
                return out;
            }
            Err(_) => {
                out.stage = Stage::Broken;
                return out;
            }
        }
    }

    out
}

/// Run this composition as a **nested stage** of another chain.
///
/// Arguments are `(bytes, n)`; the result is the folded prefix. A fallback or a
/// broken seal inside the folded leaves is a hard error — from the outer chain's
/// perspective a sealed composition that cannot run natively is a broken seal, never
/// a silent foreign fallback.
pub fn run_chain_encoded(
    dispatcher: &mut NativeDispatcher,
    args: &[Vec<u8>],
    auth: &PortingAuthority,
) -> Result<Vec<u8>, DispatchError> {
    let buf = args.first().cloned().unwrap_or_default();
    let n = args
        .get(1)
        .map(|x| decode_usize(x))
        .unwrap_or(0)
        .min(buf.len());
    let out = run_chain(dispatcher, &buf[..n], n, auth);
    match out.stage {
        Stage::Native => Ok(out.folded),
        Stage::Fallback => Err(DispatchError::SealBroken(format!(
            "{}: fold stage fell back to the foreign implementation",
            COMPOSITION_TOUPPER_EACH.id
        ))),
        Stage::Broken => Err(DispatchError::SealBroken(format!(
            "{}: fold stage hit a broken seal",
            COMPOSITION_TOUPPER_EACH.id
        ))),
    }
}

// ============================================================================
// Composition verdict
// ============================================================================

/// The composition residual: the sealed chain replayed the whole corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToupperEachVerdict {
    pub target: String,
    pub stages: Vec<String>,
    /// The sealed object the toupper stage actually dispatched to.
    pub toupper_object_hash: String,
    pub toupper_elf_symbol: String,
    pub cases_run: u64,
    /// Cases where every byte was served by the sealed object.
    pub toupper_native_cases: u64,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    /// SHA-256 over the whole chain per case (stage status + input + folded output).
    pub chain_hash: String,
    pub verdict: CourtVerdict,
}

impl ToupperEachVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.toupper_native_cases == self.cases_run
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.toupper_object_hash.is_empty()
            && !self.chain_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};stages={};toupper_object_hash={};toupper_elf_symbol={};cases_run={};toupper_native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};dispatches_run={};oracle_hash={};chain_hash={};verdict={}",
            self.target,
            self.stages.join(","),
            self.toupper_object_hash,
            self.toupper_elf_symbol,
            self.cases_run,
            self.toupper_native_cases,
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
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v1\",\n  \"target\": \"{}\",\n  \"stages\": [{}],\n  \"locale_contract\": \"C\",\n  \"toupper_object_hash\": \"{}\",\n  \"toupper_elf_symbol\": \"{}\",\n  \"cases_run\": {},\n  \"toupper_native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"chain_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            stages.join(", "),
            self.toupper_object_hash,
            json_escape(&self.toupper_elf_symbol),
            self.cases_run,
            self.toupper_native_cases,
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

/// SHA-256 over the whole chain: per case, the stage status, the input and the folded
/// output. Folding byte `i` differently changes the hash.
fn chain_hash(rows: &[(String, Stage, String, String)]) -> String {
    let mut buf = String::new();
    for (case_id, stage, input_hex, output_hex) in rows {
        buf.push_str(case_id);
        buf.push(';');
        buf.push_str(stage.as_str());
        buf.push(';');
        buf.push_str(input_hex);
        buf.push(';');
        buf.push_str(output_hex);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// Replay the composition corpus through the sealed chain.
pub fn run_composition_court(
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (ToupperEachVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut toupper_native: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut dispatches: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows = Vec::with_capacity(traces.len());
    let mut toupper_object_hash = String::new();
    let mut toupper_elf_symbol = String::new();

    for t in traces {
        let args = t.input_args();
        let buf = args.first().cloned().unwrap_or_default();
        let n = args.get(1).map(|x| decode_usize(x)).unwrap_or(0);
        let n = n.min(buf.len());

        let outcome = run_chain(&mut dispatcher, &buf[..n], n, auth);
        dispatches += outcome.dispatches;

        if outcome.stage == Stage::Native {
            toupper_native += 1;
        }
        if outcome.stage == Stage::Fallback {
            fallback += 1;
        }
        if outcome.stage == Stage::Broken {
            broken += 1;
        }

        if toupper_object_hash.is_empty() {
            if let Some((h, s)) = dispatcher.sealed_binding(LIBC_TOUPPER.id) {
                toupper_object_hash = h;
                toupper_elf_symbol = s;
            }
        }

        let actual_hex = hex::encode(&outcome.folded);
        if outcome.stage == Stage::Native && actual_hex == t.output_hex && t.status == "ok" {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(Mismatch {
                case_id: t.case_id.clone(),
                expected_output_hex: t.output_hex.clone(),
                actual_output_hex: match outcome.stage {
                    Stage::Broken => String::from("!broken seal"),
                    Stage::Fallback => String::from("!foreign fallback"),
                    Stage::Native => actual_hex.clone(),
                },
            });
        }

        rows.push((
            t.case_id.clone(),
            outcome.stage,
            hex::encode(&buf[..n]),
            actual_hex,
        ));
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0 && fallback == 0 && broken == 0 && toupper_native == cases_run {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        ToupperEachVerdict {
            target: COMPOSITION_TOUPPER_EACH.id.to_string(),
            stages: COMPOSITION_TOUPPER_EACH
                .stages
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            toupper_object_hash,
            toupper_elf_symbol,
            cases_run,
            toupper_native_cases: toupper_native,
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

/// A single folded call, for the CLI/demo path.
#[derive(Clone, Debug)]
pub struct CompositionCall {
    pub folded: Vec<u8>,
    pub stage: &'static str,
    pub dispatches: u64,
}

pub fn run_composition_call(
    index: &SealedPortIndex,
    bytes: &[u8],
    n: usize,
    auth: &PortingAuthority,
) -> CompositionCall {
    let mut dispatcher = NativeDispatcher::new(index.clone());
    let nn = n.min(bytes.len());
    let o = run_chain(&mut dispatcher, &bytes[..nn], nn, auth);
    CompositionCall {
        folded: o.folded,
        stage: o.stage.as_str(),
        dispatches: o.dispatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::LIBC_TOUPPER as TOUPPER;

    fn traces_for(cases: &[TestCase]) -> Vec<OracleTrace> {
        cases
            .iter()
            .map(|c| {
                let buf = &c.args[0];
                let n = decode_usize(&c.args[1]).min(buf.len());
                let folded: Vec<u8> = buf[..n]
                    .iter()
                    .map(|b| {
                        let mut v = *b;
                        if v >= b'a' && v <= b'z' {
                            v -= 0x20;
                        }
                        v
                    })
                    .collect();
                OracleTrace::for_target_id(
                    COMPOSITION_TOUPPER_EACH.id,
                    "C",
                    &c.case_id,
                    &c.args,
                    &folded,
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
        assert_eq!(a.len(), 267);

        let mut ids: Vec<&str> = a.iter().map(|c| c.case_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), a.len());

        for c in &a {
            assert_eq!(c.args.len(), 2, "case {}", c.case_id);
            let n = decode_usize(&c.args[1]);
            assert!(n >= 1, "case {} has an empty fold", c.case_id);
            assert!(n <= c.args[0].len(), "case {}", c.case_id);
            assert!(c.args[0].len() <= 8, "case {}", c.case_id);
        }

        assert!(ids.contains(&"A.lower.1"));
        assert!(ids.contains(&"B.61"));
        assert_eq!(ids.iter().filter(|id| id.starts_with("B.")).count(), 256);
        assert!(ids.contains(&"C.edges"));
        assert!(ids.contains(&"E.prefix.n3"));
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
        assert_eq!(v.toupper_native_cases, 0);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

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

        let cases = composition_corpus();
        let traces = traces_for(&cases);
        let (v, _) = run_composition_court(&traces, &index, &PortingAuthority::granted());
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.broken_seal_cases, v.cases_run);
        assert_eq!(v.fallback_cases, 0);
        assert!(!v.is_sealed_eligible());
    }

    /// The encoded entry point refuses to produce a value when a nested stage is not
    /// sealed — a fallback is never silently folded into the outer chain.
    #[test]
    fn test_encoded_entry_point_fails_closed_without_a_sealed_leaf() {
        let mut dispatcher = NativeDispatcher::new(SealedPortIndex::new());
        let args = alloc::vec![alloc::vec![0x61u8], 1u64.to_le_bytes().to_vec()];
        let err = run_chain_encoded(&mut dispatcher, &args, &PortingAuthority::granted());
        assert!(err.is_err());
    }
}
