// porting/mod.rs — JIT-Porting Court
//
// Observe a foreign API surface as a black box through a dialect cage, seal the
// observed behavior as oracle traces, replay a clean-room native candidate
// against those traces, and promote the candidate to `sealed` only when every
// case matches exactly and the compiled candidate artifact is bound.
//
// Scope: **API-surface** porting (byte-in/byte-out and small buffer functions).
// This is NOT arbitrary binary translation. The cage observes a foreign
// implementation; it does not copy it.
//
// Trust ladder (docs/REPLAY_COURTS.md):
//   Unknown(0) → Observed(1) → Replayed(2) → OracleCompared(3)
//              → ResidualStable(4) → Sealed(5)
//
// Residual map (one machine-readable artifact per stage):
//   observation / oracle traces → oracle_traces.json
//   behavior signature          → behavior_signature.json
//   candidate residual          → candidate_signature.json
//   replay + comparison         → replay_verdict.json
//   promotion                   → promotion_receipt.json
//   sealed package              → sealed_package.json
//   compiled candidate          → candidate.o + candidate.receipts.json (hashed)

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::kernel::CapabilitySet;

pub mod behavior_signature;
pub mod candidate;
pub mod compiled;
pub mod dialect_cage;
pub mod evidence;
pub mod exec;
pub mod oracle_trace;
pub mod promotion;
pub mod replay_court;
pub mod target;

pub use behavior_signature::BehaviorSignature;
pub use candidate::{CandidateArtifacts, CandidateSignature};
pub use exec::{ExecError, ExecutionVerdict};
pub use oracle_trace::OracleTrace;
pub use promotion::{PromotionError, PromotionEvidence, PromotionReceipt, TrustState};
pub use replay_court::{CourtVerdict, Mismatch, ReplayVerdict};
pub use target::{resolve_target, PortTarget, TestCase};

/// SHA-256, lowercase hex. The strongest hash already used by the repo
/// (phorc's `receipts::hash_bytes`); no weak bootstrap hashes in court evidence.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out: [u8; 32] = hasher.finalize().into();
    hex::encode(out)
}

/// Minimal JSON string escaping for the writer (values are identifiers/hex).
pub fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Errors from the porting court. All failures fail *closed*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortError {
    /// The caller lacks `CapabilitySet::PORTING`.
    CapabilityDenied,
    /// No such port target.
    UnknownTarget(String),
    /// The dialect cage cannot observe this symbol.
    UnsupportedTarget(String),
    /// Compiling the clean-room candidate failed.
    Compile(String),
    /// The sealed object could not be loaded/executed (execution court).
    Execution(String),
    /// Evidence I/O failed (the court never proceeds on I/O error).
    Io(String),
    /// Promotion was refused by the court.
    PromotionRefused(PromotionError),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortError::CapabilityDenied => write!(f, "capability denied: PORTING required"),
            PortError::UnknownTarget(t) => write!(f, "unknown port target: {}", t),
            PortError::UnsupportedTarget(t) => write!(f, "target not observable by cage: {}", t),
            PortError::Compile(m) => write!(f, "candidate compilation failed: {}", m),
            PortError::Execution(m) => write!(f, "execution court failed: {}", m),
            PortError::Io(m) => write!(f, "evidence I/O error: {}", m),
            PortError::PromotionRefused(e) => write!(f, "promotion refused: {}", e.as_str()),
        }
    }
}

/// Authority to observe foreign behavior and to promote candidates.
///
/// Porting is capability-gated at both ends: there is no ambient authority to
/// observe a foreign implementation or to seal a native replacement.
#[derive(Clone, Copy, Debug)]
pub struct PortingAuthority {
    caps: CapabilitySet,
}

impl PortingAuthority {
    /// Authority granted the `PORTING` capability.
    pub fn granted() -> Self {
        Self {
            caps: CapabilitySet {
                bits: CapabilitySet::PORTING,
            },
        }
    }

    /// No authority (used to demonstrate that the court fails closed).
    pub fn none() -> Self {
        Self {
            caps: CapabilitySet::empty(),
        }
    }

    /// Authority from an existing capability set.
    pub fn with(caps: CapabilitySet) -> Self {
        Self { caps }
    }

    pub fn can_observe(&self) -> bool {
        self.caps.has(CapabilitySet::PORTING)
    }

    pub fn can_promote(&self) -> bool {
        self.caps.has(CapabilitySet::PORTING)
    }

    pub fn caps(&self) -> CapabilitySet {
        self.caps
    }
}

/// A sealed port entry — the artifact a promoted native implementation leaves in
/// the store. Lookups are gated: without `PORTING` the entry is invisible.
///
/// The entry points at the *compiled* candidate object, not just the Rust mirror.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedPortEntry {
    pub target: String,
    pub trust: TrustState,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub candidate_object_hash: String,
    /// Path to the compiled ELF64 object that is the authoritative implementation.
    pub candidate_object_path: String,
    pub sealed_package: String,
}

/// In-memory index of sealed ports (the store lookup surface).
#[derive(Clone, Debug, Default)]
pub struct SealedPortIndex {
    entries: Vec<SealedPortEntry>,
}

impl SealedPortIndex {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, entry: SealedPortEntry) {
        self.entries.push(entry);
    }

    pub fn lookup(&self, target: &str) -> Option<&SealedPortEntry> {
        self.entries.iter().find(|e| e.target == target)
    }

    /// Capability-gated lookup: sealed entries are not revealed without
    /// `PORTING`. There is no ambient authority to read the store.
    pub fn lookup_gated(&self, target: &str, auth: &PortingAuthority) -> Option<&SealedPortEntry> {
        if !auth.can_observe() {
            return None;
        }
        self.lookup(target)
    }

    /// The compiled artifact the runtime would prefer, if sealed and authorized.
    pub fn native_artifact(&self, target: &str, auth: &PortingAuthority) -> Option<(&str, &str)> {
        self.lookup_gated(target, auth)
            .filter(|e| e.trust == TrustState::Sealed)
            .map(|e| {
                (
                    e.candidate_object_path.as_str(),
                    e.candidate_object_hash.as_str(),
                )
            })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// How far the court should run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDepth {
    /// Observe foreign behavior, seal oracle traces.
    Observe,
    /// + replay the native candidate and compare.
    Replay,
    /// + promote to sealed (writes the full evidence set).
    Promote,
}

impl PortDepth {
    pub fn parse(s: &str) -> Option<PortDepth> {
        match s {
            "observe" => Some(PortDepth::Observe),
            "replay" => Some(PortDepth::Replay),
            "promote" | "court" => Some(PortDepth::Promote),
            _ => None,
        }
    }
}

/// Outcome of a court run, suitable for printing.
#[derive(Clone, Debug)]
pub struct PortCourtReport {
    pub symbol: String,
    pub target: String,
    pub dialect: String,
    pub version: String,
    pub locale_contract: String,
    pub observed_cases: u64,
    pub replay_cases: u64,
    pub passed: u64,
    pub failed: u64,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub candidate_object_hash: String,
    pub candidate_receipt_hash: String,
    pub compiler_version: String,
    pub candidate_object_path: String,
    pub verdict: String,
    pub promotion: String,
    pub sealed_package: String,
    pub evidence_dir: String,
    /// Execution-court results (the sealed object was loaded and replayed).
    pub abi_symbol: String,
    pub elf_symbol: String,
    pub execution_cases: u64,
    pub execution_passed: u64,
    pub execution_failed: u64,
    pub execution_hash: String,
    pub execution_verdict: String,
}

/// Observe a target's complete domain through the dialect cage.
pub fn observe_target(
    target: &PortTarget,
    cases: &[TestCase],
    auth: &PortingAuthority,
) -> Result<Vec<OracleTrace>, PortError> {
    dialect_cage::observe_target(target, cases, auth)
}

/// Run the porting court for `symbol`.
///
/// Deterministic: the case domain is enumerated in a fixed order, the cage is
/// pure black-box observation, and every hash is SHA-256 over a stable
/// canonical encoding. No wall-clock value participates in the verdict.
///
/// From `Replay` depth up, the clean-room candidate is *compiled* with `phorc`
/// and then **executed**: this module loads the emitted ELF64 object, verifies its
/// hash against the seal, locates the ABI entry symbol and replays the corpus
/// through the compiled code. Promotion additionally requires that execution to
/// match the oracle exactly.
pub fn run_port_court(
    symbol: &str,
    auth: &PortingAuthority,
    out_dir: &str,
    depth: PortDepth,
    phorc: Option<&str>,
) -> Result<PortCourtReport, PortError> {
    // Capability gate: observation requires PORTING.
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    let target =
        resolve_target(symbol).ok_or_else(|| PortError::UnknownTarget(symbol.to_string()))?;

    // 1. The target's deterministic case set.
    let cases = target::cases_for(&target);

    // 2. Observe foreign behavior through the cage.
    let traces = observe_target(&target, &cases, auth)?;

    // 3. Seal the observed behavior as a behavior signature.
    let signature = BehaviorSignature::from_traces(&target, &traces);

    // 4. Replay + compare the Rust mirror of the candidate (fail closed).
    let (verdict, mismatches) = replay_court::run_replay_court(&traces);

    // 5. Compiled candidate authority + sealed-object execution (from Replay
    //    depth up): compile the clean-room `.phor` candidate, hash the emitted
    //    object/receipts, then load and execute the sealed object.
    let (compiled, execution) = if depth == PortDepth::Observe {
        (None, None)
    } else {
        let c = compiled::compile_candidate(&target, out_dir, phorc)
            .map_err(|e| PortError::Compile(e.message()))?;
        // The execution court verifies the object hash against the seal before
        // mapping anything, and refuses non-leaf or relocated entry points.
        let (exec_verdict, exec_mismatches) =
            exec::execute_sealed_candidate(&target, &traces, &c.object_path, &c.object_hash, auth)
                .map_err(|e| PortError::Execution(format!("{}", e)))?;
        (Some(c), Some((exec_verdict, exec_mismatches)))
    };

    let artifacts = match &compiled {
        Some(c) => c.artifacts(),
        None => CandidateArtifacts {
            source_hash: String::new(),
            object_hash: String::new(),
            receipt_hash: String::new(),
            compiler_version: String::new(),
        },
    };
    let candidate_sig = CandidateSignature::from_traces(&target, &traces, &artifacts);

    // 6. Promote only on a consistent replay AND a consistent execution.
    let mut receipt: Option<PromotionReceipt> = None;
    let mut promotion_json: Option<String> = None;
    let mut sealed_json: Option<String> = None;

    if depth == PortDepth::Promote {
        let (exec_verdict, _) = execution.as_ref().expect("execution runs at Promote depth");
        let sealed = evidence::sealed_package_json(
            &target,
            &verdict,
            &signature,
            &candidate_sig,
            exec_verdict,
        );
        let promotion_evidence = PromotionEvidence {
            sealed_package_written: true,
            replay_residual_written: true,
        };
        let r = promotion::promote(
            &target,
            &verdict,
            &artifacts,
            exec_verdict,
            &promotion_evidence,
            auth,
        )
        .map_err(PortError::PromotionRefused)?;
        promotion_json = Some(r.to_json());
        sealed_json = Some(sealed);
        receipt = Some(r);
    }

    // 7. Write the evidence set (deterministic; the raw court is reproducible).
    let paths = evidence::write_evidence_set(
        out_dir,
        &target,
        depth,
        &traces,
        &signature,
        &candidate_sig,
        &verdict,
        &mismatches,
        execution.as_ref().map(|(v, m)| (v, m.as_slice())),
        promotion_json.as_deref(),
        sealed_json.as_deref(),
    )
    .map_err(|e| PortError::Io(format!("{}", e)))?;

    let promotion_label = match receipt {
        Some(ref r) => r.to.as_str().to_string(),
        None => match depth {
            PortDepth::Observe => TrustState::Observed.as_str().to_string(),
            PortDepth::Replay => TrustState::OracleCompared.as_str().to_string(),
            PortDepth::Promote => TrustState::Unknown.as_str().to_string(),
        },
    };

    let exec_verdict = execution.map(|(v, _)| v);

    Ok(PortCourtReport {
        symbol: symbol.to_string(),
        target: target.id.to_string(),
        dialect: target.dialect.to_string(),
        version: target.version.to_string(),
        locale_contract: target.locale_contract.to_string(),
        observed_cases: traces.len() as u64,
        replay_cases: verdict.cases_run,
        passed: verdict.cases_passed,
        failed: verdict.cases_failed,
        oracle_hash: verdict.oracle_hash.clone(),
        candidate_behavior_hash: verdict.candidate_behavior_hash.clone(),
        candidate_source_hash: artifacts.source_hash.clone(),
        candidate_object_hash: artifacts.object_hash.clone(),
        candidate_receipt_hash: artifacts.receipt_hash.clone(),
        compiler_version: artifacts.compiler_version.clone(),
        candidate_object_path: compiled
            .as_ref()
            .map(|c| c.object_path.clone())
            .unwrap_or_default(),
        verdict: verdict.verdict.as_str().to_string(),
        promotion: promotion_label,
        sealed_package: paths.sealed_package.clone(),
        evidence_dir: paths.dir.clone(),
        abi_symbol: exec_verdict
            .as_ref()
            .map(|v| v.abi_symbol.clone())
            .unwrap_or_default(),
        elf_symbol: exec_verdict
            .as_ref()
            .map(|v| v.elf_symbol.clone())
            .unwrap_or_default(),
        execution_cases: exec_verdict.as_ref().map(|v| v.cases_run).unwrap_or(0),
        execution_passed: exec_verdict.as_ref().map(|v| v.cases_passed).unwrap_or(0),
        execution_failed: exec_verdict.as_ref().map(|v| v.cases_failed).unwrap_or(0),
        execution_hash: exec_verdict
            .as_ref()
            .map(|v| v.execution_hash.clone())
            .unwrap_or_default(),
        execution_verdict: exec_verdict
            .as_ref()
            .map(|v| v.verdict.as_str().to_string())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn traces_for_domain() -> Vec<OracleTrace> {
        let target = target::LIBC_TOUPPER;
        let cases = target::byte_domain_cases();
        dialect_cage::observe_target(&target, &cases, &PortingAuthority::granted()).unwrap()
    }

    #[test]
    fn test_complete_input_domain() {
        let cases = target::byte_domain_cases();
        assert_eq!(cases.len(), 256);
        assert_eq!(cases[0].case_id, "0x00");
        assert_eq!(cases[0].args, vec![vec![0u8]]);
        assert_eq!(cases[255].case_id, "0xff");
        assert_eq!(cases[255].args, vec![vec![255u8]]);
        for (i, c) in cases.iter().enumerate() {
            assert_eq!(c.args[0][0] as usize, i);
        }
    }

    #[test]
    fn test_capability_denied_without_porting() {
        let target = target::LIBC_TOUPPER;
        let cases = target::byte_domain_cases();
        let denied = dialect_cage::observe_target(&target, &cases, &PortingAuthority::none());
        assert_eq!(denied, Err(PortError::CapabilityDenied));
    }

    #[test]
    fn test_sealed_index_is_gated_and_points_at_compiled_artifact() {
        let mut index = SealedPortIndex::new();
        index.insert(SealedPortEntry {
            target: "libc:toupper:c-locale:u8:v1".to_string(),
            trust: TrustState::Sealed,
            oracle_hash: "aa".to_string(),
            candidate_behavior_hash: "bb".to_string(),
            candidate_source_hash: "cc".to_string(),
            candidate_object_hash: "dd".to_string(),
            candidate_object_path: "phost/evidence/porting/toupper/candidate.o".to_string(),
            sealed_package: "sealed_package.json".to_string(),
        });
        let id = "libc:toupper:c-locale:u8:v1";

        // No ambient authority: invisible without PORTING.
        assert!(index.lookup_gated(id, &PortingAuthority::none()).is_none());
        assert!(index
            .native_artifact(id, &PortingAuthority::none())
            .is_none());

        // With authority, the runtime artifact is the compiled object.
        let (path, hash) = index
            .native_artifact(id, &PortingAuthority::granted())
            .unwrap();
        assert!(path.ends_with("candidate.o"));
        assert_eq!(hash, "dd");
    }

    #[test]
    fn test_observation_is_deterministic() {
        let a = traces_for_domain();
        let b = traces_for_domain();
        assert_eq!(a, b);
        assert_eq!(
            oracle_trace::combined_oracle_hash(&a),
            oracle_trace::combined_oracle_hash(&b)
        );
    }
}
