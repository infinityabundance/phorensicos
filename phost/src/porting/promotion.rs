// porting/promotion.rs — Trust promotion
//
// Promotion advances a candidate along the trust ladder only when the replay
// court returned an exact, non-empty match, the clean-room source is bound, and
// the full evidence set was written. Nothing else promotes. This is the one
// place a native candidate becomes trusted native law.

use alloc::format;
use alloc::string::{String, ToString};

use crate::porting::replay_court::{CourtVerdict, ReplayVerdict};
use crate::porting::target::PortTarget;
use crate::porting::{json_escape, sha256_hex, PortingAuthority};

/// Trust ladder (docs/REPLAY_COURTS.md §Trust Ladder).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustState {
    Unknown,
    Observed,
    Replayed,
    OracleCompared,
    ResidualStable,
    Sealed,
}

impl TrustState {
    pub fn level(&self) -> u8 {
        match self {
            TrustState::Unknown => 0,
            TrustState::Observed => 1,
            TrustState::Replayed => 2,
            TrustState::OracleCompared => 3,
            TrustState::ResidualStable => 4,
            TrustState::Sealed => 5,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TrustState::Unknown => "unknown",
            TrustState::Observed => "observed",
            TrustState::Replayed => "replayed",
            TrustState::OracleCompared => "oracle-compared",
            TrustState::ResidualStable => "residual-stable",
            TrustState::Sealed => "sealed",
        }
    }
}

/// Why a promotion was refused. Every variant fails closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionError {
    CapabilityDenied,
    NoCases,
    VerdictNotConsistent,
    CasesFailed,
    MissingOracleHash,
    MissingCandidateBehaviorHash,
    MissingCandidateSourceHash,
    SealedPackageNotWritten,
    ReplayResidualNotWritten,
}

impl PromotionError {
    pub fn as_str(&self) -> &'static str {
        match self {
            PromotionError::CapabilityDenied => "PORTING capability required to promote",
            PromotionError::NoCases => "no replayed cases",
            PromotionError::VerdictNotConsistent => "verdict is not consistent",
            PromotionError::CasesFailed => "one or more cases failed",
            PromotionError::MissingOracleHash => "oracle hash missing",
            PromotionError::MissingCandidateBehaviorHash => "candidate behavior hash missing",
            PromotionError::MissingCandidateSourceHash => "candidate source hash missing",
            PromotionError::SealedPackageNotWritten => "sealed package was not written",
            PromotionError::ReplayResidualNotWritten => "replay residual was not written",
        }
    }
}

/// Evidence the court requires before it will seal.
#[derive(Clone, Copy, Debug)]
pub struct PromotionEvidence {
    pub sealed_package_written: bool,
    pub replay_residual_written: bool,
}

/// The promotion residual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionReceipt {
    pub target: String,
    pub from: TrustState,
    pub to: TrustState,
    pub verdict: String,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub sealed_package: String,
    pub replay_residual_hash: String,
}

impl PromotionReceipt {
    pub fn verdict_state(&self) -> CourtVerdict {
        match self.verdict.as_str() {
            "consistent" => CourtVerdict::Consistent,
            "inconsistent" => CourtVerdict::Inconsistent,
            _ => CourtVerdict::Inconclusive,
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};from={};to={};verdict={};oracle_hash={};candidate_behavior_hash={};candidate_source_hash={};sealed_package={};replay_residual_hash={}",
            self.target,
            self.from.as_str(),
            self.to.as_str(),
            self.verdict,
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            self.sealed_package,
            self.replay_residual_hash
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.promotion_receipt.v1\",\n  \"target\": \"{}\",\n  \"from\": \"{}\",\n  \"to\": \"{}\",\n  \"verdict\": \"{}\",\n  \"oracle_hash\": \"{}\",\n  \"candidate_behavior_hash\": \"{}\",\n  \"candidate_source_hash\": \"{}\",\n  \"sealed_package\": \"{}\",\n  \"replay_residual_hash\": \"{}\",\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            self.from.as_str(),
            self.to.as_str(),
            json_escape(&self.verdict),
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            json_escape(&self.sealed_package),
            self.replay_residual_hash,
            self.residual_hash()
        )
    }
}

/// Promote a candidate to `Sealed` iff the court permits it.
pub fn promote(
    target: &PortTarget,
    verdict: &ReplayVerdict,
    candidate_source_hash: &str,
    evidence: &PromotionEvidence,
    auth: &PortingAuthority,
) -> Result<PromotionReceipt, PromotionError> {
    // Capability gate.
    if !auth.can_promote() {
        return Err(PromotionError::CapabilityDenied);
    }
    if verdict.cases_run == 0 {
        return Err(PromotionError::NoCases);
    }
    if verdict.verdict != CourtVerdict::Consistent {
        return Err(PromotionError::VerdictNotConsistent);
    }
    if verdict.cases_failed != 0 || verdict.cases_passed != verdict.cases_run {
        return Err(PromotionError::CasesFailed);
    }
    if verdict.oracle_hash.is_empty() {
        return Err(PromotionError::MissingOracleHash);
    }
    if verdict.candidate_behavior_hash.is_empty() {
        return Err(PromotionError::MissingCandidateBehaviorHash);
    }
    if candidate_source_hash.is_empty() {
        return Err(PromotionError::MissingCandidateSourceHash);
    }
    if !evidence.sealed_package_written {
        return Err(PromotionError::SealedPackageNotWritten);
    }
    if !evidence.replay_residual_written {
        return Err(PromotionError::ReplayResidualNotWritten);
    }

    Ok(PromotionReceipt {
        target: target.id.to_string(),
        from: TrustState::OracleCompared,
        to: TrustState::Sealed,
        verdict: verdict.verdict.as_str().to_string(),
        oracle_hash: verdict.oracle_hash.clone(),
        candidate_behavior_hash: verdict.candidate_behavior_hash.clone(),
        candidate_source_hash: candidate_source_hash.to_string(),
        sealed_package: "sealed_package.json".to_string(),
        replay_residual_hash: verdict.residual_hash(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::candidate::phor_toupper;
    use crate::porting::oracle_trace::OracleTrace;
    use crate::porting::replay_court::run_replay_court;
    use crate::porting::target::{self, byte_domain_cases};
    use alloc::vec::Vec;

    const SOURCE_HASH: &str = "c0ffee00000000000000000000000000000000000000000000000000000000ff";

    fn agreeing_verdict() -> ReplayVerdict {
        let traces: Vec<OracleTrace> = byte_domain_cases()
            .iter()
            .map(|c| {
                OracleTrace::single(
                    &target::LIBC_TOUPPER,
                    &c.case_id,
                    &c.args[0],
                    &[phor_toupper(c.args[0][0])],
                    "ok",
                    &["compute"],
                )
            })
            .collect();
        run_replay_court(&traces).0
    }

    fn full_evidence() -> PromotionEvidence {
        PromotionEvidence {
            sealed_package_written: true,
            replay_residual_written: true,
        }
    }

    #[test]
    fn test_promotion_allowed_when_all_pass() {
        let verdict = agreeing_verdict();
        let receipt = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            SOURCE_HASH,
            &full_evidence(),
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(receipt.to, TrustState::Sealed);
        assert_eq!(receipt.from, TrustState::OracleCompared);
        assert_eq!(receipt.target, "libc:toupper:c-locale:u8:v1");
        assert_eq!(receipt.oracle_hash, verdict.oracle_hash);
        assert_eq!(
            receipt.candidate_behavior_hash,
            verdict.candidate_behavior_hash
        );
        assert_eq!(receipt.candidate_source_hash, SOURCE_HASH);
    }

    #[test]
    fn test_promotion_denied_when_any_case_fails() {
        let mut traces: Vec<OracleTrace> = byte_domain_cases()
            .iter()
            .map(|c| {
                OracleTrace::single(
                    &target::LIBC_TOUPPER,
                    &c.case_id,
                    &c.args[0],
                    &[phor_toupper(c.args[0][0])],
                    "ok",
                    &["compute"],
                )
            })
            .collect();
        traces[0x61].output_hex = "42".to_string();
        let (verdict, _) = run_replay_court(&traces);
        let refused = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            SOURCE_HASH,
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::VerdictNotConsistent));
    }

    #[test]
    fn test_promotion_denied_without_capability() {
        let verdict = agreeing_verdict();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            SOURCE_HASH,
            &full_evidence(),
            &PortingAuthority::none(),
        );
        assert_eq!(refused, Err(PromotionError::CapabilityDenied));
    }

    #[test]
    fn test_promotion_denied_without_candidate_source_hash() {
        let verdict = agreeing_verdict();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            "",
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::MissingCandidateSourceHash));
    }

    #[test]
    fn test_promotion_denied_without_sealed_package() {
        let verdict = agreeing_verdict();
        let partial = PromotionEvidence {
            sealed_package_written: false,
            replay_residual_written: true,
        };
        let refused = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            SOURCE_HASH,
            &partial,
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::SealedPackageNotWritten));
    }
}
