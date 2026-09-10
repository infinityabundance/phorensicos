// porting/mod.rs — JIT-Porting Court
//
// Observe a foreign API surface as a black box through a dialect cage, seal the
// observed behavior as oracle traces, replay a clean-room native candidate
// against those traces, and promote the candidate to `sealed` only when every
// case matches exactly.
//
// Scope: **API-surface** porting (byte-in/byte-out functions such as `toupper`).
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

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::kernel::CapabilitySet;

pub mod behavior_signature;
pub mod candidate;
pub mod dialect_cage;
pub mod evidence;
pub mod oracle_trace;
pub mod promotion;
pub mod replay_court;
pub mod target;

pub use behavior_signature::BehaviorSignature;
pub use candidate::CandidateSignature;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedPortEntry {
    pub target: String,
    pub trust: TrustState,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
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
    pub verdict: String,
    pub promotion: String,
    pub sealed_package: String,
    pub evidence_dir: String,
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
/// `candidate_source_hash` binds the clean-room candidate source into the seal;
/// an empty value blocks promotion (fail closed).
pub fn run_port_court(
    symbol: &str,
    auth: &PortingAuthority,
    out_dir: &str,
    depth: PortDepth,
    candidate_source_hash: &str,
) -> Result<PortCourtReport, PortError> {
    // Capability gate: observation requires PORTING.
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    let target =
        resolve_target(symbol).ok_or_else(|| PortError::UnknownTarget(symbol.to_string()))?;

    // 1. Complete input domain (0..=255 for byte-in/byte-out targets).
    let cases = target::byte_domain_cases();

    // 2. Observe foreign behavior through the cage.
    let traces = observe_target(&target, &cases, auth)?;

    // 3. Seal the observed behavior as a behavior signature, and the candidate
    //    residual (behavior + source binding) for the same cases.
    let signature = BehaviorSignature::from_traces(&target, &traces);
    let candidate_sig = CandidateSignature::from_traces(&target, &traces, candidate_source_hash);

    // 4. Replay + compare (fail closed).
    let (verdict, mismatches) = replay_court::run_replay_court(&traces);

    // 5. Promote only on a consistent verdict with a complete evidence set.
    let mut receipt: Option<PromotionReceipt> = None;
    let mut promotion_json: Option<String> = None;
    let mut sealed_json: Option<String> = None;

    if depth == PortDepth::Promote {
        let sealed = evidence::sealed_package_json(&target, &verdict, &signature, &candidate_sig);
        let promotion_evidence = PromotionEvidence {
            sealed_package_written: true,
            replay_residual_written: true,
        };
        let r = promotion::promote(
            &target,
            &verdict,
            candidate_source_hash,
            &promotion_evidence,
            auth,
        )
        .map_err(PortError::PromotionRefused)?;
        promotion_json = Some(r.to_json());
        sealed_json = Some(sealed);
        receipt = Some(r);
    }

    // 6. Write the evidence set (deterministic; the raw court is reproducible).
    let paths = evidence::write_evidence_set(
        out_dir,
        &target,
        depth,
        &traces,
        &signature,
        &candidate_sig,
        &verdict,
        &mismatches,
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
        candidate_source_hash: candidate_source_hash.to_string(),
        verdict: verdict.verdict.as_str().to_string(),
        promotion: promotion_label,
        sealed_package: paths.sealed_package.clone(),
        evidence_dir: paths.dir.clone(),
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
        assert_eq!(cases[0].input, vec![0u8]);
        assert_eq!(cases[255].case_id, "0xff");
        assert_eq!(cases[255].input, vec![255u8]);
        // Ordering is strictly increasing and stable.
        for (i, c) in cases.iter().enumerate() {
            assert_eq!(c.input[0] as usize, i);
        }
    }

    #[test]
    fn test_capability_denied_without_porting() {
        let target = target::LIBC_TOUPPER;
        let cases = target::byte_domain_cases();
        let denied = dialect_cage::observe_target(&target, &cases, &PortingAuthority::none());
        assert_eq!(denied, Err(PortError::CapabilityDenied));

        // Sealed store lookup is also invisible without the capability.
        let mut index = SealedPortIndex::new();
        index.insert(SealedPortEntry {
            target: "libc:toupper:c-locale:u8:v1".to_string(),
            trust: TrustState::Sealed,
            oracle_hash: "aa".to_string(),
            candidate_behavior_hash: "bb".to_string(),
            candidate_source_hash: "cc".to_string(),
            sealed_package: "sealed_package.json".to_string(),
        });
        let id = "libc:toupper:c-locale:u8:v1";
        assert!(index.lookup_gated(id, &PortingAuthority::none()).is_none());
        assert!(index
            .lookup_gated(id, &PortingAuthority::granted())
            .is_some());
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
