// porting/evidence.rs — Evidence artifacts (std)
//
// Writes the machine-readable residual set for a court run. The raw court is
// deterministic, so re-running produces byte-identical artifacts; the verifier
// relies on that. Raw dumps are small JSON files and are committed.
//
// Artifacts written by `PortDepth::Promote`:
//   oracle_traces.json  behavior_signature.json  candidate_signature.json
//   replay_verdict.json promotion_receipt.json   sealed_package.json

use alloc::format;
use alloc::string::{String, ToString};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::porting::behavior_signature::BehaviorSignature;
use crate::porting::candidate::CandidateSignature;
use crate::porting::oracle_trace::{self, OracleTrace};
use crate::porting::replay_court::{Mismatch, ReplayVerdict};
use crate::porting::target::PortTarget;
use crate::porting::{json_escape, PortDepth};

/// Absolute-ish paths of the artifacts written by a run.
#[derive(Clone, Debug, Default)]
pub struct EvidencePaths {
    pub dir: String,
    pub oracle_traces: String,
    pub behavior_signature: String,
    pub candidate_signature: String,
    pub replay_verdict: String,
    pub promotion_receipt: String,
    pub sealed_package: String,
}

/// The sealed package residual: what a promoted native implementation publishes.
pub fn sealed_package_json(
    target: &PortTarget,
    verdict: &ReplayVerdict,
    signature: &BehaviorSignature,
) -> String {
    format!(
        "{{\n  \"schema\": \"phorensic.porting.sealed_package.v1\",\n  \"package\": \"native:{dialect}:{symbol}\",\n  \"target\": \"{symbol}\",\n  \"dialect\": \"{dialect}\",\n  \"version\": \"{version}\",\n  \"trust\": \"sealed\",\n  \"court_verdict\": \"{verdict}\",\n  \"case_count\": {cases},\n  \"oracle_hash\": \"{oracle}\",\n  \"candidate_hash\": \"{candidate}\",\n  \"native_symbol\": \"phor_{symbol}\",\n  \"sealed_by\": \"phorensic:porting-court:v1\"\n}}\n",
        dialect = json_escape(target.dialect),
        symbol = json_escape(target.symbol),
        version = json_escape(target.version),
        verdict = verdict.verdict.as_str(),
        cases = signature.case_count,
        oracle = verdict.oracle_hash,
        candidate = verdict.candidate_hash,
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

    if depth == PortDepth::Replay {
        return Ok(paths);
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
