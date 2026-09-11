// phorport/multioracle.rs — the implementation axis (§16)
//
// Phase 7. The dialect cage observes the host C library: one implementation. A
// `PortSpec` may require more. This module observes the *same sealed corpus*
// through an independent implementation (musl, out-of-process) and reports a
// `MultiOracleVerdict`.
//
// It does not reimplement the cross-implementation court: `phost::porting::
// cross_impl` is the one implementation of that comparison, and its
// `CrossVerdict` already binds the sealed oracle hash. This module *classifies*
// the result: agreement is concordance over the declared region, and any
// disagreement is an `OracleDivergenceResidual` — never a majority vote.
//
// Fails closed: when the policy requires more witnesses than can answer, the
// verdict is `InsufficientWitnesses`, and the obligation that binds witnesses
// refuses. Absence of a second implementation is never silently accepted as
// agreement with one.

use std::path::Path;

use phost::porting::cross_impl::{compile_probe, run_cross_court};
use phost::porting::ident::OracleWitnessId;
use phost::porting::oracle_witness::{
    DivergenceCategory, MultiOracleVerdict, MultiOracleVerdictKind, OracleDivergenceResidual,
    OracleRegion, OracleWitness, QualificationPolicy,
};
use phost::porting::target::PortTarget;
use phost::porting::PortingAuthority;

/// Why a multi-oracle run could not produce a verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiOracleError {
    /// The second implementation could not be built (musl-gcc absent).
    SecondImplementationUnavailable(String),
    /// The court refused (capability, missing corpus, probe error).
    Court(String),
}

impl MultiOracleError {
    pub fn as_str(&self) -> &'static str {
        match self {
            MultiOracleError::SecondImplementationUnavailable(_) => {
                "second implementation unavailable"
            }
            MultiOracleError::Court(_) => "cross-implementation court refused",
        }
    }
}

/// The label for the host implementation, from the compile target.
fn host_implementation() -> String {
    if cfg!(target_env = "musl") {
        String::from("musl")
    } else {
        String::from("glibc")
    }
}

fn target_triple() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// Build the host-libc witness from the cross verdict's primary observation.
fn host_witness(verdict: &phost::porting::cross_impl::CrossVerdict, locale: &str) -> OracleWitness {
    OracleWitness {
        contract: verdict
            .target
            .split(':')
            .take(2)
            .collect::<Vec<_>>()
            .join(":"),
        implementation: host_implementation(),
        implementation_version: verdict.primary_version_observed.clone(),
        executable_identity: format!("sealed-oracle:{}", verdict.primary_oracle_hash),
        runtime_closure_identity: format!("in-process:{}", verdict.primary_mechanism),
        target: target_triple(),
        architecture: std::env::consts::ARCH.to_string(),
        environment: std::env::consts::OS.to_string(),
        locale: locale.to_string(),
    }
}

/// Build the musl witness from the cross verdict's secondary observation.
fn musl_witness(verdict: &phost::porting::cross_impl::CrossVerdict, locale: &str) -> OracleWitness {
    OracleWitness {
        contract: verdict
            .target
            .split(':')
            .take(2)
            .collect::<Vec<_>>()
            .join(":"),
        implementation: String::from("musl"),
        // The probe reports its library and ABI sizes but not a version string;
        // "unreported" is the honest value, and the runtime closure identity
        // carries what the probe actually said.
        implementation_version: String::from("unreported"),
        executable_identity: format!("sha256:{}", verdict.probe_binary_hash),
        runtime_closure_identity: format!("probe:{}", verdict.secondary_identity),
        target: target_triple(),
        architecture: std::env::consts::ARCH.to_string(),
        environment: String::from("musl"),
        locale: locale.to_string(),
    }
}

/// The witness identities for a region, without running anything.
pub fn witness_ids(witnesses: &[OracleWitness]) -> Vec<String> {
    witnesses.iter().map(|w| w.id().canonical()).collect()
}

/// A multi-oracle run: the verdict and the witnesses that produced it (the
/// witnesses are the evidence; the verdict binds its own identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiOracleReport {
    pub verdict: MultiOracleVerdict,
    pub witnesses: Vec<OracleWitness>,
}

/// Observe the sealed corpus through every available witness and report.
///
/// For a policy requiring one witness, the host observation alone is used (the
/// sealed corpus is the region). For a policy requiring more, a second
/// implementation is built and compared; if it cannot answer, the verdict is
/// `InsufficientWitnesses`.
pub fn run_multi_oracle(
    target: &PortTarget,
    policy: QualificationPolicy,
    locale: &str,
    probe_out: &Path,
    auth: &PortingAuthority,
) -> Result<MultiOracleReport, MultiOracleError> {
    let required = policy.required_witnesses();

    if required <= 1 {
        return host_only(target, policy, locale, auth);
    }

    // A second implementation is required. Build it, or fail closed.
    let probe = match compile_probe(probe_out.to_str().unwrap_or("musl_probe")) {
        Ok(p) => p,
        Err(e) => {
            return Ok(insufficient(
                target,
                policy,
                locale,
                format!("musl-gcc could not build the second implementation: {e}"),
            ));
        }
    };

    let (verdict, mismatches) = run_cross_court(target, &probe, auth)
        .map_err(|e| MultiOracleError::Court(format!("{e}")))?;

    let host = host_witness(&verdict, locale);
    let musl = musl_witness(&verdict, locale);
    let witnesses = vec![host.id().canonical(), musl.id().canonical()];
    let mut divergences: Vec<OracleDivergenceResidual> = mismatches
        .iter()
        .map(|m| OracleDivergenceResidual {
            contract: format!("{}:{}", target.dialect, target.symbol),
            locale: locale.to_string(),
            witnesses: witnesses.clone(),
            // Classification is a separate act; the court reports the fact.
            category: DivergenceCategory::Unclassified,
            case_id: m.case_id.clone(),
            primary_hex: m.primary_hex.clone(),
            secondary_hex: m.secondary_hex.clone(),
            note: String::from("observed divergence between the host implementation and musl"),
        })
        .collect();
    divergences.sort_by(|a, b| a.case_id.cmp(&b.case_id));

    let kind = if verdict.cases_run == 0 {
        MultiOracleVerdictKind::Inconclusive
    } else if divergences.is_empty() {
        MultiOracleVerdictKind::Concordant
    } else {
        MultiOracleVerdictKind::Divergent
    };

    Ok(MultiOracleReport {
        verdict: MultiOracleVerdict {
            schema_version: MultiOracleVerdict::SCHEMA_VERSION,
            target: target.id.to_string(),
            region: OracleRegion {
                contract: format!("{}:{}", target.dialect, target.symbol),
                locale: locale.to_string(),
            },
            policy,
            witnesses,
            cases_run: verdict.cases_run,
            agreements: verdict.agreements,
            divergences,
            cases_undetermined: 0,
            verdict: kind,
        },
        witnesses: vec![host, musl],
    })
}

/// The host-only path: one witness, observed over the sealed corpus.
fn host_only(
    target: &PortTarget,
    policy: QualificationPolicy,
    locale: &str,
    auth: &PortingAuthority,
) -> Result<MultiOracleReport, MultiOracleError> {
    use phost::porting::dialect_cage;
    use phost::porting::oracle_trace::combined_oracle_hash;
    use phost::porting::target::cases_for;

    if !auth.can_observe() {
        return Err(MultiOracleError::Court(
            "PORTING capability required to observe".to_string(),
        ));
    }
    let cases = cases_for(target);
    if cases.is_empty() {
        return Err(MultiOracleError::Court(format!(
            "no cases for {}",
            target.id
        )));
    }
    let traces = dialect_cage::observe_target(target, &cases, auth)
        .map_err(|e| MultiOracleError::Court(format!("{e}")))?;
    let oracle_hash = combined_oracle_hash(&traces);
    let witness = OracleWitness {
        contract: format!("{}:{}", target.dialect, target.symbol),
        implementation: host_implementation(),
        implementation_version: String::from("unreported"),
        executable_identity: format!("sealed-oracle:{oracle_hash}"),
        runtime_closure_identity: format!("in-process:{}", target.locale_contract),
        target: target_triple(),
        architecture: std::env::consts::ARCH.to_string(),
        environment: std::env::consts::OS.to_string(),
        locale: locale.to_string(),
    };
    let kind = if traces.is_empty() {
        MultiOracleVerdictKind::Inconclusive
    } else {
        MultiOracleVerdictKind::Concordant
    };
    let host_id = witness.id().canonical();
    Ok(MultiOracleReport {
        verdict: MultiOracleVerdict {
            schema_version: MultiOracleVerdict::SCHEMA_VERSION,
            target: target.id.to_string(),
            region: OracleRegion {
                contract: format!("{}:{}", target.dialect, target.symbol),
                locale: locale.to_string(),
            },
            policy,
            witnesses: vec![host_id],
            cases_run: traces.len() as u64,
            agreements: traces.len() as u64,
            divergences: vec![],
            cases_undetermined: 0,
            verdict: kind,
        },
        witnesses: vec![witness],
    })
}

/// A fail-closed verdict when the second implementation cannot answer.
fn insufficient(
    target: &PortTarget,
    policy: QualificationPolicy,
    locale: &str,
    note: String,
) -> MultiOracleReport {
    MultiOracleReport {
        verdict: MultiOracleVerdict {
            schema_version: MultiOracleVerdict::SCHEMA_VERSION,
            target: target.id.to_string(),
            region: OracleRegion {
                contract: format!("{}:{}", target.dialect, target.symbol),
                locale: locale.to_string(),
            },
            policy,
            witnesses: vec![],
            cases_run: 0,
            agreements: 0,
            divergences: vec![OracleDivergenceResidual {
                contract: format!("{}:{}", target.dialect, target.symbol),
                locale: locale.to_string(),
                witnesses: vec![],
                category: DivergenceCategory::EnvironmentDifference,
                case_id: String::from("region"),
                primary_hex: String::new(),
                secondary_hex: String::new(),
                note,
            }],
            cases_undetermined: 0,
            verdict: MultiOracleVerdictKind::InsufficientWitnesses,
        },
        witnesses: vec![],
    }
}

/// The witness identity type, re-exported for callers that bind it.
pub type WitnessId = OracleWitnessId;

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::target::resolve_target;

    #[test]
    fn test_host_only_policy_needs_no_second_implementation() {
        let target = resolve_target("strspn").expect("target");
        let auth = PortingAuthority::granted();
        let tmp = std::env::temp_dir().join("phorport-multioracle-hostonly");
        let r = run_multi_oracle(&target, QualificationPolicy::HostObserved, "C", &tmp, &auth)
            .expect("report");
        assert_eq!(r.verdict.witnesses.len(), 1);
        assert!(r.verdict.is_concordant());
        assert!(r.verdict.cases_run > 0);
        assert_eq!(r.witnesses.len(), 1);
    }

    #[test]
    fn test_multi_policy_either_concords_or_reports_insufficient() {
        // musl-gcc may or may not be present in this environment. Either way the
        // result must be honest: concordant with two witnesses, or an explicit
        // insufficient-witness verdict — never a silent success with one.
        let target = resolve_target("strspn").expect("target");
        let auth = PortingAuthority::granted();
        let tmp = std::env::temp_dir().join("phorport-multioracle-multi");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tmp");
        let policy = QualificationPolicy::MultiImplementation {
            minimum_witnesses: 2,
        };
        let r =
            run_multi_oracle(&target, policy, "C", &tmp.join("musl_probe"), &auth).expect("report");
        match r.verdict.verdict {
            MultiOracleVerdictKind::Concordant => {
                assert_eq!(r.verdict.witnesses.len(), 2);
                assert_eq!(r.witnesses.len(), 2);
                assert_eq!(r.verdict.divergences.len(), 0);
            }
            MultiOracleVerdictKind::InsufficientWitnesses => {
                assert!(r.verdict.witnesses.is_empty());
                assert!(!r.verdict.is_concordant());
            }
            other => panic!("unexpected verdict {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_witness_ids_are_namespaced() {
        let target = resolve_target("strspn").expect("target");
        let auth = PortingAuthority::granted();
        let tmp = std::env::temp_dir().join("phorport-multioracle-ids");
        let r = run_multi_oracle(&target, QualificationPolicy::HostObserved, "C", &tmp, &auth)
            .expect("report");
        assert!(r.verdict.witnesses[0].starts_with("phor.oracle-witness:"));
    }
}
