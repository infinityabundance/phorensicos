// porting/evidence.rs — Evidence artifacts (std)
//
// Writes the machine-readable residual set for a court run. The raw court is
// deterministic, so re-running produces byte-identical artifacts; the verifier
// relies on that. Raw dumps are small JSON files and are committed.
//
// Artifacts written by `PortDepth::Promote`:
//   oracle_traces.json  behavior_signature.json  candidate_signature.json
//   replay_verdict.json execution_verdict.json   dispatch_verdict.json
//   promotion_receipt.json sealed_package.json

use alloc::format;
use alloc::string::{String, ToString};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::porting::behavior_signature::BehaviorSignature;
use crate::porting::candidate::CandidateSignature;
use crate::porting::composition::CompositionVerdict;
use crate::porting::composition_nested::NestedVerdict;
use crate::porting::composition_pair::PairVerdict;
use crate::porting::composition_slice_search::SliceSearchVerdict;
use crate::porting::composition_strlen_memchr::StrlenMemchrVerdict;
use crate::porting::composition_suffix::SuffixVerdict;
use crate::porting::composition_toupper_each::ToupperEachVerdict;
use crate::porting::cross_impl::{CrossMismatch, CrossVerdict};
use crate::porting::dispatch::DispatchVerdict;
use crate::porting::exec::ExecutionVerdict;
use crate::porting::oracle_trace::{self, OracleTrace};
use crate::porting::replay_court::{Mismatch, ReplayVerdict};
use crate::porting::service::SessionVerdict;
use crate::porting::target::PortTarget;
use crate::porting::{json_escape, sha256_hex, PortDepth};

/// Absolute-ish paths of the artifacts written by a run.
#[derive(Clone, Debug, Default)]
pub struct EvidencePaths {
    pub dir: String,
    pub oracle_traces: String,
    pub behavior_signature: String,
    pub candidate_signature: String,
    pub replay_verdict: String,
    pub execution_verdict: String,
    pub dispatch_verdict: String,
    pub promotion_receipt: String,
    pub sealed_package: String,
}

/// SHA-256 (lowercase hex) of a file's bytes — used to bind the clean-room
/// candidate source into the seal.
pub fn hash_file(path: &str) -> io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

/// The sealed package residual: what a promoted native implementation publishes.
///
/// It binds the qualified target id, the locale contract, the candidate's
/// behavior hash, the compiled candidate artifacts (source, ELF64 object,
/// receipts) plus the compiler that produced them, and — now that the promotion
/// gate includes the execution court — the ABI entry symbol and the behavior
/// hash of the **executed** object.
pub fn sealed_package_json(
    target: &PortTarget,
    verdict: &ReplayVerdict,
    signature: &BehaviorSignature,
    candidate: &CandidateSignature,
    execution: &ExecutionVerdict,
    dispatch: &DispatchVerdict,
) -> String {
    format!(
        "{{\n  \"schema\": \"phorensic.porting.sealed_package.v1\",\n  \"package\": \"native:{id}\",\n  \"target\": \"{id}\",\n  \"dialect\": \"{dialect}\",\n  \"symbol\": \"{symbol}\",\n  \"version\": \"{version}\",\n  \"locale_contract\": \"{locale}\",\n  \"trust\": \"sealed\",\n  \"court_verdict\": \"{verdict}\",\n  \"case_count\": {cases},\n  \"oracle_hash\": \"{oracle}\",\n  \"candidate_behavior_hash\": \"{behavior}\",\n  \"candidate_source_hash\": \"{source}\",\n  \"candidate_object_hash\": \"{object}\",\n  \"candidate_receipt_hash\": \"{receipt}\",\n  \"compiler_version\": \"{compiler}\",\n  \"native_symbol\": \"{native_symbol}\",\n  \"candidate_abi_symbol\": \"{abi_symbol}\",\n  \"executed_elf_symbol\": \"{elf_symbol}\",\n  \"candidate_execution_hash\": \"{execution_hash}\",\n  \"execution_verdict\": \"{exec_verdict}\",\n  \"execution_cases\": {exec_cases},\n  \"dispatch_cases\": {dispatch_cases},\n  \"dispatch_native_cases\": {dispatch_native},\n  \"dispatch_fallback_cases\": {dispatch_fallback},\n  \"dispatch_broken_seal_cases\": {dispatch_broken_seal},\n  \"dispatch_hash\": \"{dispatch_hash}\",\n  \"dispatch_verdict\": \"{dispatch_verdict}\",\n  \"sealed_by\": \"phorensic:porting-court:v1\"\n}}\n",
        id = json_escape(target.id),
        dialect = json_escape(target.dialect),
        symbol = json_escape(target.symbol),
        version = json_escape(target.version),
        locale = json_escape(target.locale_contract),
        verdict = verdict.verdict.as_str(),
        cases = signature.case_count,
        oracle = verdict.oracle_hash,
        behavior = candidate.candidate_behavior_hash,
        source = candidate.candidate_source_hash,
        object = candidate.candidate_object_hash,
        receipt = candidate.candidate_receipt_hash,
        compiler = json_escape(&candidate.compiler_version),
        native_symbol = json_escape(&candidate.symbol),
        abi_symbol = json_escape(&execution.abi_symbol),
        elf_symbol = json_escape(&execution.elf_symbol),
        execution_hash = execution.execution_hash,
        exec_verdict = execution.verdict.as_str(),
        exec_cases = execution.cases_run,
        dispatch_cases = dispatch.cases_run,
        dispatch_native = dispatch.native_cases,
        dispatch_fallback = dispatch.fallback_cases,
        dispatch_broken_seal = dispatch.broken_seal_cases,
        dispatch_hash = dispatch.dispatch_hash,
        dispatch_verdict = dispatch.verdict.as_str(),
    )
}

/// Write the evidence set for `depth`. Later stages include earlier stages, so a
/// `Promote` run writes all six artifacts.
#[allow(clippy::too_many_arguments)]
pub fn write_evidence_set(
    dir: &str,
    _target: &PortTarget,
    depth: PortDepth,
    traces: &[OracleTrace],
    signature: &BehaviorSignature,
    candidate_sig: &CandidateSignature,
    verdict: &ReplayVerdict,
    mismatches: &[Mismatch],
    execution: Option<(&ExecutionVerdict, &[Mismatch])>,
    dispatch: Option<(&DispatchVerdict, &[Mismatch])>,
    promotion_json: Option<&str>,
    sealed_json: Option<&str>,
) -> io::Result<EvidencePaths> {
    let base: PathBuf = PathBuf::from(dir);
    fs::create_dir_all(&base)?;

    let mut paths = EvidencePaths {
        dir: base.display().to_string(),
        ..Default::default()
    };

    // Stage 1: observation / oracle traces + behavior signature.
    let oracle_path = base.join("oracle_traces.json");
    fs::write(&oracle_path, oracle_trace::traces_to_json(traces))?;
    paths.oracle_traces = oracle_path.display().to_string();

    let sig_path = base.join("behavior_signature.json");
    fs::write(&sig_path, signature.to_json())?;
    paths.behavior_signature = sig_path.display().to_string();

    if depth == PortDepth::Observe {
        return Ok(paths);
    }

    // Stage 2: candidate residual + replay/comparison residual.
    let cand_path = base.join("candidate_signature.json");
    fs::write(&cand_path, candidate_sig.to_json())?;
    paths.candidate_signature = cand_path.display().to_string();

    let verdict_path = base.join("replay_verdict.json");
    fs::write(&verdict_path, verdict.to_json(mismatches))?;
    paths.replay_verdict = verdict_path.display().to_string();

    // Stage 2b: execution residual — the sealed object was loaded and replayed.
    if let Some((exec_verdict, exec_mismatches)) = execution {
        let exec_path = base.join("execution_verdict.json");
        fs::write(&exec_path, exec_verdict.to_json(exec_mismatches))?;
        paths.execution_verdict = exec_path.display().to_string();
    }

    if depth == PortDepth::Replay {
        return Ok(paths);
    }

    // Stage 2c: dispatch residual — the runtime preferred the sealed object.
    if let Some((disp_verdict, disp_mismatches)) = dispatch {
        let disp_path = base.join("dispatch_verdict.json");
        fs::write(&disp_path, disp_verdict.to_json(disp_mismatches))?;
        paths.dispatch_verdict = disp_path.display().to_string();
    }

    // Stage 3: promotion residual + sealed package.
    let receipt_path = base.join("promotion_receipt.json");
    fs::write(&receipt_path, promotion_json.unwrap_or_default())?;
    paths.promotion_receipt = receipt_path.display().to_string();

    let sealed_path = base.join("sealed_package.json");
    fs::write(&sealed_path, sealed_json.unwrap_or_default())?;
    paths.sealed_package = sealed_path.display().to_string();

    Ok(paths)
}

/// Helper for callers that only need the sealed package residual string.
pub fn package_id(target: &PortTarget) -> String {
    format!("native:{}", target.id)
}

/// A composition verdict that can be written as court evidence.
///
/// Implemented by every composition court, so the evidence writer does not need to
/// know which chain produced the verdict (composition #1's committed evidence is
/// unaffected: the trait only names the existing `to_json`).
pub trait CompositionEvidence {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String;
}

impl CompositionEvidence for CompositionVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for StrlenMemchrVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for PairVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for ToupperEachVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for NestedVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for SuffixVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

impl CompositionEvidence for SliceSearchVerdict {
    fn evidence_json(&self, mismatches: &[Mismatch]) -> String {
        self.to_json(mismatches)
    }
}

/// Write the composition evidence set: the sealed oracle traces and the
/// composition verdict. Returns the verdict path.
///
/// The composition has no candidate of its own — its implementation is the
/// chain of already-sealed objects — so the evidence is the oracle plus the
/// per-stage chain verdict.
pub fn write_composition_evidence(
    dir: &str,
    traces: &[OracleTrace],
    verdict: &impl CompositionEvidence,
    mismatches: &[Mismatch],
) -> io::Result<String> {
    let base = PathBuf::from(dir);
    fs::create_dir_all(&base)?;
    fs::write(
        base.join("composition_oracle_traces.json"),
        oracle_trace::traces_to_json(traces),
    )?;
    let verdict_path = base.join("composition_verdict.json");
    fs::write(&verdict_path, verdict.evidence_json(mismatches))?;
    Ok(verdict_path.display().to_string())
}

/// Write the sealed native **session** evidence: many consumers served from one
/// verified store load. Returns the verdict path.
///
/// A session has no oracle of its own — every result it checks is the sealed
/// port's own recorded expectation — so the residual is the session verdict.
pub fn write_session_evidence(dir: &str, verdict: &SessionVerdict) -> io::Result<String> {
    let base = PathBuf::from(dir);
    fs::create_dir_all(&base)?;
    let path = base.join("session_verdict.json");
    fs::write(&path, verdict.to_json())?;
    Ok(path.display().to_string())
}

/// Write the **cross-implementation** evidence: the same sealed corpus observed
/// through a second, independent implementation. Returns the verdict path.
///
/// The residual is the verdict (which binds the sealed oracle hash) plus the
/// disagreements, if any — a cross-implementation court that disagreed is evidence
/// too, and must be committed rather than hidden.
pub fn write_cross_evidence(
    dir: &str,
    verdict: &CrossVerdict,
    mismatches: &[CrossMismatch],
) -> io::Result<String> {
    let base = PathBuf::from(dir);
    fs::create_dir_all(&base)?;
    let path = base.join("cross_implementation_verdict.json");
    fs::write(&path, verdict.to_json(mismatches))?;
    Ok(path.display().to_string())
}
