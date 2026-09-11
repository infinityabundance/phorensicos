// porting/oracle_witness.rs — the implementation axis, made first-class
//
// Phase 7, §16. A single host C library is one *observed implementation*; it is
// not automatically the specification. Qualification therefore binds witnesses:
// which contracts were observed, through which implementation, at which version,
// with which runtime closure, on which target, in which environment and locale.
//
// This module defines the witness and the multi-oracle verdict. It deliberately
// does **not** run anything: the host-side orchestration (`phorport`) observes
// implementations and reports; the authority binds the resulting references.
//
// Two disciplines are load-bearing:
//
//   * disagreement is never majority-voted away. `glibc != musl` is an
//     `OracleDivergenceResidual` to classify, not "two implementations voted for
//     X";
//   * agreement over a bounded region is *concordance observed over that
//     region*, never a proof of the specification.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::porting::ident::{validate_ident, OracleWitnessId};
use crate::porting::portspec::qualification_name;
use crate::porting::{json_escape, sha256_hex};

/// The identity domain tag for a witness. Bumping it is a versioned act.
pub const ORACLE_WITNESS_DOMAIN: &[u8] = b"PHOR/ORACLE-WITNESS/v1\0";

fn enc(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// One implementation that answered the contract's question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleWitness {
    /// The contract the witness was observed against (e.g. `posix:strspn`).
    pub contract: String,
    /// The implementation family (`glibc`, `musl`, `freebsd-libc`, …).
    pub implementation: String,
    /// The implementation version, as the implementation itself reports it.
    pub implementation_version: String,
    /// The identity of the executable that answered (a probe binary hash, or the
    /// library's own version string).
    pub executable_identity: String,
    /// The identity of the runtime closure that actually answered — enough that a
    /// reader knows *what* produced the observation, not merely its name.
    pub runtime_closure_identity: String,
    /// The target triple.
    pub target: String,
    /// The architecture.
    pub architecture: String,
    /// The environment (`gnu`, `musl`, …).
    pub environment: String,
    /// The locale the contract was observed in.
    pub locale: String,
}

impl OracleWitness {
    /// The canonical byte encoding of the witness (excluding the domain tag).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for s in [
            &self.contract,
            &self.implementation,
            &self.implementation_version,
            &self.executable_identity,
            &self.runtime_closure_identity,
            &self.target,
            &self.architecture,
            &self.environment,
            &self.locale,
        ] {
            enc(&mut out, s);
        }
        out
    }

    /// The witness content identity, domain-separated.
    pub fn id(&self) -> OracleWitnessId {
        let mut pre = Vec::with_capacity(ORACLE_WITNESS_DOMAIN.len() + 64);
        pre.extend_from_slice(ORACLE_WITNESS_DOMAIN);
        pre.extend_from_slice(&self.canonical_bytes());
        OracleWitnessId::new(sha256_hex(&pre))
    }

    /// Is the witness complete enough to be evidence? Fail closed.
    pub fn is_complete(&self) -> bool {
        [
            &self.contract,
            &self.implementation,
            &self.implementation_version,
            &self.executable_identity,
            &self.runtime_closure_identity,
            &self.target,
            &self.architecture,
            &self.environment,
            &self.locale,
        ]
        .iter()
        .all(|s| validate_ident(s).is_ok())
    }

    /// A compact human-readable identity line, e.g. `musl 1.2.5 (x86_64-linux-musl)`.
    pub fn summary(&self) -> String {
        format!(
            "{} {} ({}-{}-{})",
            self.implementation,
            self.implementation_version,
            self.target,
            self.environment,
            self.architecture
        )
    }
}

/// The declared region (contract + locale) over which witnesses are compared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleRegion {
    pub contract: String,
    pub locale: String,
}

/// A classified reason two implementations disagreed.
///
/// The category is a hypothesis about *why*, not a verdict about *who is right*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivergenceCategory {
    /// The case was outside the declared contract (a validator bug upstream).
    OutsideContract,
    /// Undefined behavior was accidentally entered.
    UndefinedBehaviorEntered,
    /// The behavior is implementation-defined.
    ImplementationDefined,
    /// An environment difference (locale, target, runtime closure).
    EnvironmentDifference,
    /// The specification admits more than one reading.
    SpecificationInterpretation,
    /// A normalization bug in an observer.
    NormalizationBug,
    /// A harness defect.
    HarnessDefect,
    /// A genuine implementation divergence.
    ImplementationDivergence,
    /// Not yet classified.
    Unclassified,
}

impl DivergenceCategory {
    pub fn tag(self) -> u8 {
        match self {
            DivergenceCategory::OutsideContract => 1,
            DivergenceCategory::UndefinedBehaviorEntered => 2,
            DivergenceCategory::ImplementationDefined => 3,
            DivergenceCategory::EnvironmentDifference => 4,
            DivergenceCategory::SpecificationInterpretation => 5,
            DivergenceCategory::NormalizationBug => 6,
            DivergenceCategory::HarnessDefect => 7,
            DivergenceCategory::ImplementationDivergence => 8,
            DivergenceCategory::Unclassified => 9,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DivergenceCategory::OutsideContract => "outside-contract",
            DivergenceCategory::UndefinedBehaviorEntered => "undefined-behavior-entered",
            DivergenceCategory::ImplementationDefined => "implementation-defined",
            DivergenceCategory::EnvironmentDifference => "environment-difference",
            DivergenceCategory::SpecificationInterpretation => "specification-interpretation",
            DivergenceCategory::NormalizationBug => "normalization-bug",
            DivergenceCategory::HarnessDefect => "harness-defect",
            DivergenceCategory::ImplementationDivergence => "implementation-divergence",
            DivergenceCategory::Unclassified => "unclassified",
        }
    }
}

/// One case on which two witnesses differed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleDivergenceResidual {
    pub contract: String,
    pub locale: String,
    /// The witnesses that disagreed, as their content identities.
    pub witnesses: Vec<String>,
    pub category: DivergenceCategory,
    pub case_id: String,
    pub primary_hex: String,
    pub secondary_hex: String,
    pub note: String,
}

impl OracleDivergenceResidual {
    /// The canonical string the residual hash covers.
    pub fn canonical(&self) -> String {
        let mut body = format!(
            "contract={};locale={};category={};case={};primary={};secondary={};note={}",
            self.contract,
            self.locale,
            self.category.as_str(),
            self.case_id,
            self.primary_hex,
            self.secondary_hex,
            self.note
        );
        // The witness set is order-independent.
        let mut w = self.witnesses.clone();
        w.sort();
        for id in w {
            body.push_str(&format!(";witness={id}"));
        }
        body
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }
}

/// A multi-oracle verdict kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiOracleVerdictKind {
    /// The policy's minimum witnesses answered and agreed on every case.
    Concordant,
    /// At least one case diverged between witnesses.
    Divergent,
    /// Fewer than the policy's minimum witnesses answered.
    InsufficientWitnesses,
    /// No cases were observed.
    Inconclusive,
}

impl MultiOracleVerdictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MultiOracleVerdictKind::Concordant => "concordant",
            MultiOracleVerdictKind::Divergent => "divergent",
            MultiOracleVerdictKind::InsufficientWitnesses => "insufficient-witnesses",
            MultiOracleVerdictKind::Inconclusive => "inconclusive",
        }
    }
}

/// The declared qualification policy (§16) lives in [`crate::porting::portspec`]
/// so a `PortSpec` and the multi-oracle court can never drift. This module
/// re-exports it for convenience.
pub use crate::porting::portspec::QualificationPolicy;

/// The residual for a multi-oracle court.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiOracleVerdict {
    pub schema_version: u32,
    pub target: String,
    pub region: OracleRegion,
    pub policy: QualificationPolicy,
    /// The witnesses that answered, as content identities (in order).
    pub witnesses: Vec<String>,
    pub cases_run: u64,
    pub agreements: u64,
    pub divergences: Vec<OracleDivergenceResidual>,
    pub cases_undetermined: u64,
    pub verdict: MultiOracleVerdictKind,
}

impl MultiOracleVerdict {
    pub const SCHEMA_VERSION: u32 = 1;

    /// The asserted (reproducible) claim. The residual hash covers this.
    pub fn canonical(&self) -> String {
        let mut s = format!(
            "schema={};target={};contract={};locale={};policy={};required={};cases_run={};agreements={};divergences={};undetermined={};verdict={}",
            self.schema_version,
            self.target,
            self.region.contract,
            self.region.locale,
            qualification_name(self.policy),
            self.policy.required_witnesses(),
            self.cases_run,
            self.agreements,
            self.divergences.len(),
            self.cases_undetermined,
            self.verdict.as_str()
        );
        let mut w = self.witnesses.clone();
        w.sort();
        for id in w {
            s.push_str(&format!(";witness={id}"));
        }
        let mut d: Vec<String> = self
            .divergences
            .iter()
            .map(|r| format!("{}:{}", r.case_id, r.category.as_str()))
            .collect();
        d.sort();
        for line in d {
            s.push_str(&format!(";div={line}"));
        }
        s
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    /// True only when the policy's minimum witnesses answered and agreed.
    pub fn is_concordant(&self) -> bool {
        self.verdict == MultiOracleVerdictKind::Concordant
    }

    pub fn to_json(&self) -> String {
        let wit: Vec<String> = self
            .witnesses
            .iter()
            .map(|w| format!("    \"{}\"", json_escape(w)))
            .collect();
        let divs: Vec<String> = self
            .divergences
            .iter()
            .map(|d| {
                format!(
                    "    {{\n      \"case_id\": \"{}\",\n      \"category\": \"{}\",\n      \"primary_hex\": \"{}\",\n      \"secondary_hex\": \"{}\",\n      \"note\": \"{}\",\n      \"residual_hash\": \"{}\"\n    }}",
                    json_escape(&d.case_id),
                    d.category.as_str(),
                    json_escape(&d.primary_hex),
                    json_escape(&d.secondary_hex),
                    json_escape(&d.note),
                    d.residual_hash()
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.multi_oracle_verdict.v1\",\n  \"target\": \"{}\",\n  \"contract\": \"{}\",\n  \"locale\": \"{}\",\n  \"policy\": \"{}\",\n  \"required_witnesses\": {},\n  \"witnesses\": [\n{}\n  ],\n  \"cases_run\": {},\n  \"agreements\": {},\n  \"undetermined\": {},\n  \"verdict\": \"{}\",\n  \"claim\": \"concordance of the named implementations is observed over the declared region; agreement on a bounded corpus is evidence, not proof of the specification\",\n  \"divergences\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.region.contract),
            json_escape(&self.region.locale),
            qualification_name(self.policy),
            self.policy.required_witnesses(),
            wit.join(",\n"),
            self.cases_run,
            self.agreements,
            self.cases_undetermined,
            self.verdict.as_str(),
            divs.join(",\n"),
            self.residual_hash()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::portspec::{self, QualificationPolicy as SpecPolicy};
    use alloc::string::ToString;
    use alloc::vec;

    fn witness(implementation: &str) -> OracleWitness {
        OracleWitness {
            contract: String::from("posix:strspn"),
            implementation: implementation.to_string(),
            implementation_version: String::from("1.2.5"),
            executable_identity: format!("sha256:{}", "a".repeat(64)),
            runtime_closure_identity: format!("sha256:{}", "b".repeat(64)),
            target: String::from("x86_64-unknown-linux"),
            architecture: String::from("x86_64"),
            environment: implementation.to_string(),
            locale: String::from("C"),
        }
    }

    #[test]
    fn test_witness_id_is_domain_separated_and_changes_with_identity() {
        let a = witness("glibc");
        let b = witness("musl");
        assert_ne!(a.id(), b.id());
        assert!(a.is_complete());
        // Whitespace cannot collide two witnesses because fields are
        // length-prefixed, not joined.
        let mut c = witness("glibc");
        c.environment = String::from("glibc ");
        assert_ne!(a.id(), c.id());
    }

    #[test]
    fn test_incomplete_witness_fails_closed() {
        let mut w = witness("musl");
        w.executable_identity = String::new();
        assert!(!w.is_complete());
        let mut w = witness("musl");
        w.contract = String::from("bad\ncontract");
        assert!(!w.is_complete());
    }

    #[test]
    fn test_policy_requires_at_least_two_for_multi() {
        assert_eq!(QualificationPolicy::HostObserved.required_witnesses(), 1);
        assert_eq!(
            QualificationPolicy::MultiImplementation {
                minimum_witnesses: 0
            }
            .required_witnesses(),
            2
        );
        assert_eq!(
            QualificationPolicy::MultiImplementation {
                minimum_witnesses: 3
            }
            .required_witnesses(),
            3
        );
    }

    #[test]
    fn test_verdict_is_concordant_only_without_divergence() {
        let mut v = MultiOracleVerdict {
            schema_version: MultiOracleVerdict::SCHEMA_VERSION,
            target: String::from("posix:strspn:c-locale:u64:v1"),
            region: OracleRegion {
                contract: String::from("posix:strspn"),
                locale: String::from("C"),
            },
            policy: QualificationPolicy::MultiImplementation {
                minimum_witnesses: 2,
            },
            witnesses: vec![
                witness("glibc").id().as_str().to_string(),
                witness("musl").id().as_str().to_string(),
            ],
            cases_run: 10,
            agreements: 10,
            divergences: vec![],
            cases_undetermined: 0,
            verdict: MultiOracleVerdictKind::Concordant,
        };
        assert!(v.is_concordant());
        let good = v.residual_hash();
        v.divergences.push(OracleDivergenceResidual {
            contract: String::from("posix:strspn"),
            locale: String::from("C"),
            witnesses: vec![],
            category: DivergenceCategory::Unclassified,
            case_id: String::from("C.1"),
            primary_hex: "00".to_string(),
            secondary_hex: "01".to_string(),
            note: String::from("unclassified"),
        });
        v.verdict = MultiOracleVerdictKind::Divergent;
        assert!(!v.is_concordant());
        assert_ne!(good, v.residual_hash());
    }

    #[test]
    fn test_portspec_policy_names_the_same_semantics() {
        // The `PortSpec` policy and this module's policy must not drift.
        assert_eq!(
            SpecPolicy::HostObserved.required_witnesses(),
            QualificationPolicy::HostObserved.required_witnesses()
        );
        assert_eq!(
            portspec::qualification_name(SpecPolicy::HostObserved),
            "host-observed"
        );
    }
}
