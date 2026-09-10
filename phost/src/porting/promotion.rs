// porting/promotion.rs — Trust promotion
//
// Promotion advances a candidate along the trust ladder only when the replay
// court returned an exact, non-empty match, the compiled candidate artifact is
// bound (source + object + receipt hashes), and the full evidence set was
// written. Nothing else promotes. This is the one place a native candidate
// becomes trusted native law.

use alloc::format;
use alloc::string::{String, ToString};

use crate::porting::candidate::CandidateArtifacts;
use crate::porting::dispatch::DispatchVerdict;
use crate::porting::exec::ExecutionVerdict;
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
    MissingCandidateObjectHash,
    MissingCandidateReceiptHash,
    SealedPackageNotWritten,
    ReplayResidualNotWritten,
    /// The sealed object was never executed.
    ExecutionNotRun,
    /// The compiled object diverged from the oracle, or could not be replayed.
    ExecutionMismatch,
    /// The object that was executed is not the object bound into the seal.
    ExecutionObjectMismatch,
    /// The dispatch court never ran.
    DispatchNotRun,
    /// The runtime did not serve every case from the sealed object, or the output
    /// diverged.
    DispatchMismatch,
    /// The dispatched object is not the object bound into the seal.
    DispatchObjectMismatch,
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
            PromotionError::MissingCandidateObjectHash => "candidate object hash missing",
            PromotionError::MissingCandidateReceiptHash => "candidate receipt hash missing",
            PromotionError::SealedPackageNotWritten => "sealed package was not written",
            PromotionError::ReplayResidualNotWritten => "replay residual was not written",
            PromotionError::ExecutionNotRun => "sealed object was not executed",
            PromotionError::ExecutionMismatch => "execution court verdict is not consistent",
            PromotionError::ExecutionObjectMismatch => {
                "executed object hash does not match the sealed object hash"
            }
            PromotionError::DispatchNotRun => "sealed native dispatch court did not run",
            PromotionError::DispatchMismatch => {
                "runtime did not serve every case from the sealed object"
            }
            PromotionError::DispatchObjectMismatch => {
                "dispatched object hash does not match the sealed object hash"
            }
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
    pub candidate_object_hash: String,
    pub candidate_receipt_hash: String,
    pub compiler_version: String,
    pub sealed_package: String,
    pub replay_residual_hash: String,
    /// The execution court's behavior hash of the compiled object.
    pub execution_hash: String,
    /// The ELF symbol that was loaded and called.
    pub elf_symbol: String,
    /// The dispatch court's hash: `case_id:source:output` over the whole corpus.
    pub dispatch_hash: String,
    /// Cases the runtime served from the sealed object.
    pub dispatch_native_cases: u64,
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
            "target={};from={};to={};verdict={};oracle_hash={};candidate_behavior_hash={};candidate_source_hash={};candidate_object_hash={};candidate_receipt_hash={};compiler_version={};sealed_package={};replay_residual_hash={};execution_hash={};elf_symbol={};dispatch_hash={};dispatch_native_cases={}",
            self.target,
            self.from.as_str(),
            self.to.as_str(),
            self.verdict,
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            self.candidate_object_hash,
            self.candidate_receipt_hash,
            self.compiler_version,
            self.sealed_package,
            self.replay_residual_hash,
            self.execution_hash,
            self.elf_symbol,
            self.dispatch_hash,
            self.dispatch_native_cases
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.promotion_receipt.v1\",\n  \"target\": \"{}\",\n  \"from\": \"{}\",\n  \"to\": \"{}\",\n  \"verdict\": \"{}\",\n  \"oracle_hash\": \"{}\",\n  \"candidate_behavior_hash\": \"{}\",\n  \"candidate_source_hash\": \"{}\",\n  \"candidate_object_hash\": \"{}\",\n  \"candidate_receipt_hash\": \"{}\",\n  \"compiler_version\": \"{}\",\n  \"execution_hash\": \"{}\",\n  \"elf_symbol\": \"{}\",\n  \"dispatch_hash\": \"{}\",\n  \"dispatch_native_cases\": {},\n  \"sealed_package\": \"{}\",\n  \"replay_residual_hash\": \"{}\",\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            self.from.as_str(),
            self.to.as_str(),
            json_escape(&self.verdict),
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            self.candidate_object_hash,
            self.candidate_receipt_hash,
            json_escape(&self.compiler_version),
            self.execution_hash,
            json_escape(&self.elf_symbol),
            self.dispatch_hash,
            self.dispatch_native_cases,
            json_escape(&self.sealed_package),
            self.replay_residual_hash,
            self.residual_hash()
        )
    }
}

/// Promote a candidate to `Sealed` iff the replay court *and* the sealed-object
/// execution court permit it.
pub fn promote(
    target: &PortTarget,
    verdict: &ReplayVerdict,
    artifacts: &CandidateArtifacts,
    execution: &ExecutionVerdict,
    dispatch: &DispatchVerdict,
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
    if artifacts.source_hash.is_empty() {
        return Err(PromotionError::MissingCandidateSourceHash);
    }
    if artifacts.object_hash.is_empty() {
        return Err(PromotionError::MissingCandidateObjectHash);
    }
    if artifacts.receipt_hash.is_empty() {
        return Err(PromotionError::MissingCandidateReceiptHash);
    }
    if !evidence.sealed_package_written {
        return Err(PromotionError::SealedPackageNotWritten);
    }
    if !evidence.replay_residual_written {
        return Err(PromotionError::ReplayResidualNotWritten);
    }
    // The sealed object must have been executed and matched the oracle exactly.
    if execution.cases_run == 0 {
        return Err(PromotionError::ExecutionNotRun);
    }
    if execution.verdict != CourtVerdict::Consistent
        || execution.cases_failed != 0
        || execution.cases_passed != execution.cases_run
    {
        return Err(PromotionError::ExecutionMismatch);
    }
    if execution.object_hash != artifacts.object_hash {
        return Err(PromotionError::ExecutionObjectMismatch);
    }
    // The runtime must prefer the sealed object for every case.
    if dispatch.cases_run == 0 {
        return Err(PromotionError::DispatchNotRun);
    }
    if dispatch.verdict != CourtVerdict::Consistent
        || dispatch.fallback_cases != 0
        || dispatch.broken_seal_cases != 0
        || dispatch.native_cases != dispatch.cases_run
        || dispatch.cases_failed != 0
    {
        return Err(PromotionError::DispatchMismatch);
    }
    if dispatch.object_hash != artifacts.object_hash {
        return Err(PromotionError::DispatchObjectMismatch);
    }

    Ok(PromotionReceipt {
        target: target.id.to_string(),
        from: TrustState::OracleCompared,
        to: TrustState::Sealed,
        verdict: verdict.verdict.as_str().to_string(),
        oracle_hash: verdict.oracle_hash.clone(),
        candidate_behavior_hash: verdict.candidate_behavior_hash.clone(),
        candidate_source_hash: artifacts.source_hash.clone(),
        candidate_object_hash: artifacts.object_hash.clone(),
        candidate_receipt_hash: artifacts.receipt_hash.clone(),
        compiler_version: artifacts.compiler_version.clone(),
        sealed_package: "sealed_package.json".to_string(),
        replay_residual_hash: verdict.residual_hash(),
        execution_hash: execution.execution_hash.clone(),
        elf_symbol: execution.elf_symbol.clone(),
        dispatch_hash: dispatch.dispatch_hash.clone(),
        dispatch_native_cases: dispatch.native_cases,
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

    fn artifacts() -> CandidateArtifacts {
        CandidateArtifacts {
            source_hash: "c0ffee00000000000000000000000000000000000000000000000000000000ff"
                .to_string(),
            object_hash: "0b1ec700000000000000000000000000000000000000000000000000000000ff"
                .to_string(),
            receipt_hash: "rece1p7000000000000000000000000000000000000000000000000000000ff"
                .to_string(),
            compiler_version: "phorc 0.1.0".to_string(),
        }
    }

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

    fn agreeing_execution() -> ExecutionVerdict {
        let a = artifacts();
        ExecutionVerdict {
            target: "libc:toupper:c-locale:u8:v1".to_string(),
            abi_symbol: "phor_toupper".to_string(),
            elf_symbol: "_phor_phor_toupper".to_string(),
            cases_run: 256,
            cases_passed: 256,
            cases_failed: 0,
            object_hash: a.object_hash,
            oracle_hash: "oracle".to_string(),
            execution_hash: "execution".to_string(),
            verdict: CourtVerdict::Consistent,
        }
    }

    fn agreeing_dispatch() -> DispatchVerdict {
        let a = artifacts();
        DispatchVerdict {
            target: "libc:toupper:c-locale:u8:v1".to_string(),
            cases_run: 256,
            native_cases: 256,
            fallback_cases: 0,
            broken_seal_cases: 0,
            cases_passed: 256,
            cases_failed: 0,
            object_hash: a.object_hash,
            elf_symbol: "_phor_phor_toupper".to_string(),
            oracle_hash: "oracle".to_string(),
            dispatch_hash: "dispatch".to_string(),
            verdict: CourtVerdict::Consistent,
        }
    }

    #[test]
    fn test_promotion_allowed_when_all_pass() {
        let verdict = agreeing_verdict();
        let receipt = promote(
            &target::LIBC_TOUPPER,
            &verdict,
            &artifacts(),
            &agreeing_execution(),
            &agreeing_dispatch(),
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
        assert_eq!(receipt.candidate_source_hash, artifacts().source_hash);
        assert_eq!(receipt.candidate_object_hash, artifacts().object_hash);
        assert_eq!(receipt.candidate_receipt_hash, artifacts().receipt_hash);
        assert_eq!(receipt.compiler_version, "phorc 0.1.0");
        assert_eq!(receipt.execution_hash, "execution");
        assert_eq!(receipt.elf_symbol, "_phor_phor_toupper");
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
            &artifacts(),
            &agreeing_execution(),
            &agreeing_dispatch(),
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
            &artifacts(),
            &agreeing_execution(),
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::none(),
        );
        assert_eq!(refused, Err(PromotionError::CapabilityDenied));
    }

    #[test]
    fn test_promotion_denied_without_candidate_source_hash() {
        let mut a = artifacts();
        a.source_hash = String::new();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &a,
            &agreeing_execution(),
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::MissingCandidateSourceHash));
    }

    #[test]
    fn test_promotion_denied_without_candidate_object_hash() {
        let mut a = artifacts();
        a.object_hash = String::new();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &a,
            &agreeing_execution(),
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::MissingCandidateObjectHash));
    }

    #[test]
    fn test_promotion_denied_without_candidate_receipt_hash() {
        let mut a = artifacts();
        a.receipt_hash = String::new();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &a,
            &agreeing_execution(),
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::MissingCandidateReceiptHash));
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
            &artifacts(),
            &agreeing_execution(),
            &agreeing_dispatch(),
            &partial,
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::SealedPackageNotWritten));
    }

    #[test]
    fn test_promotion_denied_when_dispatch_falls_back() {
        let mut disp = agreeing_dispatch();
        disp.verdict = CourtVerdict::Inconsistent;
        disp.native_cases = 255;
        disp.fallback_cases = 1;
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &agreeing_execution(),
            &disp,
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::DispatchMismatch));
    }

    #[test]
    fn test_promotion_denied_without_dispatch() {
        let mut disp = agreeing_dispatch();
        disp.cases_run = 0;
        disp.native_cases = 0;
        disp.cases_passed = 0;
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &agreeing_execution(),
            &disp,
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::DispatchNotRun));
    }

    #[test]
    fn test_promotion_denied_when_dispatched_object_is_not_sealed_object() {
        let mut disp = agreeing_dispatch();
        disp.object_hash = "different-object".to_string();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &agreeing_execution(),
            &disp,
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::DispatchObjectMismatch));
    }

    #[test]
    fn test_promotion_denied_without_execution() {
        let mut exec = agreeing_execution();
        exec.cases_run = 0;
        exec.cases_passed = 0;
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &exec,
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::ExecutionNotRun));
    }

    #[test]
    fn test_promotion_denied_when_execution_mismatches() {
        let mut exec = agreeing_execution();
        exec.verdict = CourtVerdict::Inconsistent;
        exec.cases_passed = 255;
        exec.cases_failed = 1;
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &exec,
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::ExecutionMismatch));
    }

    #[test]
    fn test_promotion_denied_when_executed_object_is_not_sealed_object() {
        let mut exec = agreeing_execution();
        exec.object_hash = "different-object".to_string();
        let refused = promote(
            &target::LIBC_TOUPPER,
            &agreeing_verdict(),
            &artifacts(),
            &exec,
            &agreeing_dispatch(),
            &full_evidence(),
            &PortingAuthority::granted(),
        );
        assert_eq!(refused, Err(PromotionError::ExecutionObjectMismatch));
    }
}
