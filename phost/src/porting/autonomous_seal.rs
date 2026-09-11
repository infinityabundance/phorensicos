// porting/autonomous_seal.rs — the autonomous promotion profile (§18)
//
// Phase 7. Legacy promotion (`promotion.rs`) advances a candidate on the replay
// court plus the sealed-object execution and dispatch courts. The autonomous
// profile binds strictly more: the `PortSpec`, the oracle witnesses, source →
// compiler → object provenance, the design and discovery courts, the held-out
// qualification, the court-sensitivity controls, the FRF outer-court verdicts,
// the uninstrumented execution, the dispatch court, the evidence closure and
// the store-generation relation.
//
// Two seal profiles coexist and never rewrite each other's history:
//
//   * `LegacyV1`     — the pre-autonomous seal (`promotion.rs`); its evidence is
//                      immutable and is never re-described as having passed
//                      tests that did not exist when it was made.
//   * `AutonomousV1` — this profile. A candidate cannot receive it unless every
//                      obligation is present *and* cross-consistent.
//
// The profile is data, not a bag of booleans: each obligation carries the
// identity references it is built from, and the verification is a pure function
// of those references. Nothing here interprets an FRF or Gemel id — they are
// opaque tokens (invariant §2.4).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::porting::ident::{
    ArtifactHash, CandidateObjectHash, CandidateSourceHash, ChallengeReceiptId, ClosureError,
    CompilerReceiptHash, EvidenceClosure, EvidenceClosureId, EvidenceEdge, EvidenceRole,
    FrfClaimId, FrfReceiptId, GemelGid, PortSpecId, QualificationReceiptId, StoreGenerationId,
};
use crate::porting::oracle_witness::OracleWitness;
use crate::porting::portspec::QualificationPolicy;
use crate::porting::{json_escape, sha256_hex};

/// The identity domain tag for an autonomous promotion receipt.
pub const AUTONOMOUS_SEAL_DOMAIN: &[u8] = b"PHOR/AUTONOMOUS-SEAL/v1\0";

/// The seal profile a package carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealProfile {
    /// The pre-autonomous promotion seal. Evidence immutable.
    LegacyV1,
    /// The autonomous profile defined by this module.
    AutonomousV1,
}

impl SealProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            SealProfile::LegacyV1 => "LegacyV1",
            SealProfile::AutonomousV1 => "AutonomousV1",
        }
    }

    /// The stable tag used in canonical encodings (never renumber these).
    pub fn as_tag(self) -> u8 {
        match self {
            SealProfile::LegacyV1 => 1,
            SealProfile::AutonomousV1 => 2,
        }
    }

    /// The obligations this profile requires.
    pub fn required_obligations(self) -> &'static [Obligation] {
        match self {
            // The legacy profile is checked by `promotion::promote`; it has no
            // autonomous obligations. It is listed here so a reader cannot
            // mistake one profile for the other.
            SealProfile::LegacyV1 => &[],
            SealProfile::AutonomousV1 => &Obligation::ALL,
        }
    }
}

/// One machine-readable promotion obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obligation {
    O1PortSpecIdentity,
    O2PreconditionValidation,
    O3OracleWitnesses,
    O4BuildProvenance,
    O5DesignConsistency,
    O6DiscoveryRegressions,
    O7Qualification,
    O8CourtSensitivity,
    O9FrfReceipts,
    O10UninstrumentedExecution,
    O11Dispatch,
    O12EvidenceClosure,
    O13StoreGeneration,
}

impl Obligation {
    pub const ALL: [Obligation; 13] = [
        Obligation::O1PortSpecIdentity,
        Obligation::O2PreconditionValidation,
        Obligation::O3OracleWitnesses,
        Obligation::O4BuildProvenance,
        Obligation::O5DesignConsistency,
        Obligation::O6DiscoveryRegressions,
        Obligation::O7Qualification,
        Obligation::O8CourtSensitivity,
        Obligation::O9FrfReceipts,
        Obligation::O10UninstrumentedExecution,
        Obligation::O11Dispatch,
        Obligation::O12EvidenceClosure,
        Obligation::O13StoreGeneration,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Obligation::O1PortSpecIdentity => "O1",
            Obligation::O2PreconditionValidation => "O2",
            Obligation::O3OracleWitnesses => "O3",
            Obligation::O4BuildProvenance => "O4",
            Obligation::O5DesignConsistency => "O5",
            Obligation::O6DiscoveryRegressions => "O6",
            Obligation::O7Qualification => "O7",
            Obligation::O8CourtSensitivity => "O8",
            Obligation::O9FrfReceipts => "O9",
            Obligation::O10UninstrumentedExecution => "O10",
            Obligation::O11Dispatch => "O11",
            Obligation::O12EvidenceClosure => "O12",
            Obligation::O13StoreGeneration => "O13",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Obligation::O1PortSpecIdentity => "PortSpec identity valid",
            Obligation::O2PreconditionValidation => {
                "all oracle cases passed typed precondition validation"
            }
            Obligation::O3OracleWitnesses => "required oracle witnesses bound",
            Obligation::O4BuildProvenance => {
                "candidate source → compiler → object provenance bound"
            }
            Obligation::O5DesignConsistency => "design replay consistent",
            Obligation::O6DiscoveryRegressions => "all admitted discovery regressions consistent",
            Obligation::O7Qualification => "held-out qualification consistent",
            Obligation::O8CourtSensitivity => {
                "court sensitivity demonstrated for required mutation families"
            }
            Obligation::O9FrfReceipts => "FRF required receipts verify",
            Obligation::O10UninstrumentedExecution => {
                "ordinary uninstrumented candidate execution consistent"
            }
            Obligation::O11Dispatch => {
                "dispatch court serves the native artifact with zero broken seals/fallbacks"
            }
            Obligation::O12EvidenceClosure => "evidence closure complete",
            Obligation::O13StoreGeneration => "store-generation parent/current relation valid",
        }
    }
}

/// Why an autonomous seal was refused. Every variant fails closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealRefusal {
    /// An obligation is absent.
    Missing(Obligation),
    /// An obligation is present but its evidence is inconsistent.
    Inconsistent(Obligation, &'static str),
    /// The evidence closure itself is malformed or empty.
    Closure(ClosureError),
    /// A receipt cannot carry the legacy profile: legacy evidence is made by
    /// `promotion.rs`, not here.
    LegacyProfileNotAutonomous,
}

impl SealRefusal {
    pub fn as_str(&self) -> &'static str {
        match self {
            SealRefusal::Missing(_) => "required obligation missing",
            SealRefusal::Inconsistent(_, _) => "obligation evidence is inconsistent",
            SealRefusal::Closure(_) => "evidence closure is invalid",
            SealRefusal::LegacyProfileNotAutonomous => {
                "the legacy profile is verified by promotion.rs, not the autonomous profile"
            }
        }
    }

    /// A deterministic, leak-free message. It names the obligation but never
    /// echoes evidence bytes, so a refusal cannot disclose a held-out answer.
    pub fn describe(&self) -> String {
        match self {
            SealRefusal::Missing(o) => {
                format!("{}: {}", o.id(), "required obligation missing")
            }
            SealRefusal::Inconsistent(o, why) => {
                format!("{}: {} ({})", o.id(), "inconsistent", why)
            }
            SealRefusal::Closure(e) => {
                format!("O12: {} ({})", "evidence closure is invalid", e.as_str())
            }
            SealRefusal::LegacyProfileNotAutonomous => {
                "LegacyV1: the legacy profile is verified by promotion.rs".to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence shapes
// ---------------------------------------------------------------------------

/// O2/O3 — the foreign oracle observation and its declared witnesses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleEvidence {
    /// Cases that passed typed precondition validation and reached the oracle.
    pub cases_validated: u64,
    /// Cases that reached the oracle *without* passing validation. Must be 0.
    pub cases_out_of_contract_reaching_oracle: u64,
    /// The policy the witnesses are judged against.
    pub policy: QualificationPolicy,
    /// The witnesses that answered.
    pub witnesses: Vec<OracleWitness>,
    /// The multi-oracle verdict's residual hash (its own identity).
    pub multi_oracle_residual_hash: String,
}

/// O4 — source → compiler → object provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildEvidence {
    pub source_hash: CandidateSourceHash,
    pub object_hash: CandidateObjectHash,
    pub compiler_receipt_hash: CompilerReceiptHash,
    /// Whether an independent rebuild reproduced the object (§22). The profile
    /// does not require byte reproducibility; it requires that the *result* of
    /// the rebuild attempt be recorded, and that it be re-checkable.
    pub independent_rebuild_matches: bool,
    /// The rebuild's outcome is always asserted (a mismatch is evidence too).
    pub independent_rebuild_checked: bool,
}

/// O5/O6 — the design court and the admitted discovery regressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignEvidence {
    pub cases_run: u64,
    pub cases_failed: u64,
    /// Discovery counterexamples admitted as permanent regression knowledge.
    pub discovery_regressions: u64,
    /// Of those, how many are consistent in the final candidate.
    pub discovery_regressions_consistent: u64,
    /// Discovery counterexamples still outstanding (unfalsified). Must be 0.
    pub discovery_outstanding: u64,
}

/// O7 — held-out qualification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationEvidence {
    pub receipt_id: QualificationReceiptId,
    /// The held-out universe identity (distinct from any design evidence).
    pub universe_id: String,
    pub cases_run: u64,
    pub cases_failed: u64,
    /// Whether the isolation audit held (no qualification material reached the
    /// producer workspace). Must be true.
    pub isolated: bool,
}

/// O8 — court sensitivity (challenge).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChallengeEvidence {
    pub receipt_id: ChallengeReceiptId,
    pub families_declared: u64,
    pub families_detected: u64,
    /// Mutants excluded as equivalent under the declared domain.
    pub families_equivalent_excluded: u64,
    /// Mutants whose outcome is undetermined. Must be 0.
    pub families_undetermined: u64,
}

/// O9 — the FRF outer court.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrfEvidence {
    pub receipts: Vec<FrfReceiptId>,
    pub claims: Vec<FrfClaimId>,
    pub verified: bool,
}

/// O10 — ordinary, uninstrumented execution of the compiled object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEvidence {
    pub object_hash: CandidateObjectHash,
    pub cases_run: u64,
    pub cases_failed: u64,
    /// The executed object was the ordinary (uninstrumented) artifact. Must be
    /// true: an instrumented search artifact can never be promoted.
    pub uninstrumented: bool,
}

/// O11 — the native dispatch court.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchEvidence {
    pub object_hash: CandidateObjectHash,
    pub cases_run: u64,
    pub native_cases: u64,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub cases_failed: u64,
}

/// O13 — the immutable store-generation relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGenerationEvidence {
    pub current: StoreGenerationId,
    /// The generation this one descends from (`None` only for genesis).
    pub parent: Option<StoreGenerationId>,
    /// Whether a parent is required by the publication policy.
    pub parent_required: bool,
    /// The seal profile recorded for the parent generation, when present.
    pub parent_seal_profile: Option<SealProfile>,
}

/// The full evidence bundle the autonomous profile verifies.
///
/// Each `Option` is an obligation. `None` means the obligation is *absent*, and
/// verification fails closed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutonomousSealEvidence {
    pub port_spec_id: Option<PortSpecId>,
    pub oracle: Option<OracleEvidence>,
    pub build: Option<BuildEvidence>,
    pub design: Option<DesignEvidence>,
    pub qualification: Option<QualificationEvidence>,
    pub challenge: Option<ChallengeEvidence>,
    pub frf: Option<FrfEvidence>,
    pub execution: Option<ExecutionEvidence>,
    pub dispatch: Option<DispatchEvidence>,
    pub closure: Option<EvidenceClosure>,
    pub store: Option<StoreGenerationEvidence>,
    /// Bounded Gemel references (never identity-bearing on their own).
    pub gemel_refs: Vec<GemelGid>,
    /// The candidate behavior hash the design/execution courts observed. Bound
    /// here so a later reader can re-check the object's behavior, not just its
    /// bytes.
    pub candidate_behavior_hash: Option<ArtifactHash>,
}

// ---------------------------------------------------------------------------
// The receipt
// ---------------------------------------------------------------------------

/// One satisfied obligation, with the references it is built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObligationReceipt {
    pub obligation: Obligation,
    pub evidence: Vec<EvidenceEdge>,
}

/// The autonomous promotion receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutonomousPromotionReceipt {
    pub schema_version: u32,
    pub profile: SealProfile,
    pub obligations: Vec<ObligationReceipt>,
    pub closure: EvidenceClosureId,
    pub residual_hash: String,
}

impl AutonomousPromotionReceipt {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn to_json(&self) -> String {
        let obs: Vec<String> = self
            .obligations
            .iter()
            .map(|o| {
                let ev: Vec<String> = o
                    .evidence
                    .iter()
                    .map(|e| {
                        format!(
                            "        {{\"role\": \"{}\", \"identity\": \"{}\"}}",
                            e.role.as_str(),
                            json_escape(&e.identity)
                        )
                    })
                    .collect();
                format!(
                    "    {{\n      \"id\": \"{}\",\n      \"requirement\": \"{}\",\n      \"status\": \"satisfied\",\n      \"evidence\": [\n{}\n      ]\n    }}",
                    o.obligation.id(),
                    o.obligation.description(),
                    ev.join(",\n")
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.autonomous_promotion_receipt.v1\",\n  \"seal_profile\": \"{}\",\n  \"obligations\": [\n{}\n  ],\n  \"evidence_closure\": \"{}\",\n  \"claim\": \"every obligation of the declared profile is present and cross-consistent; this is bounded evidence, not a proof of equivalence\",\n  \"residual_hash\": \"{}\"\n}}\n",
            self.profile.as_str(),
            obs.join(",\n"),
            self.closure.as_str(),
            self.residual_hash
        )
    }
}

/// Verify the autonomous profile against `evidence`.
///
/// Pure and deterministic: no clock, no I/O. Returns the receipt only when every
/// obligation is present and cross-consistent.
pub fn verify_autonomous_seal(
    profile: SealProfile,
    evidence: &AutonomousSealEvidence,
) -> Result<AutonomousPromotionReceipt, SealRefusal> {
    if profile != SealProfile::AutonomousV1 {
        return Err(SealRefusal::LegacyProfileNotAutonomous);
    }

    let mut obligations: Vec<ObligationReceipt> = Vec::with_capacity(Obligation::ALL.len());

    // O1 — PortSpec identity.
    let spec = evidence
        .port_spec_id
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or(SealRefusal::Missing(Obligation::O1PortSpecIdentity))?;
    obligations.push(one(
        Obligation::O1PortSpecIdentity,
        vec![EvidenceEdge::new(EvidenceRole::PortSpec, spec.canonical())],
    ));

    // O2/O3 — precondition validation and the witness set.
    let oracle = evidence
        .oracle
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O2PreconditionValidation))?;
    if oracle.cases_out_of_contract_reaching_oracle != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O2PreconditionValidation,
            "an out-of-contract case reached the foreign oracle",
        ));
    }
    if oracle.cases_validated == 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O2PreconditionValidation,
            "no validated oracle cases",
        ));
    }
    obligations.push(one(
        Obligation::O2PreconditionValidation,
        vec![EvidenceEdge::new(
            EvidenceRole::DesignResult,
            format!("phor.oracle-cases:validated={}", oracle.cases_validated),
        )],
    ));
    let required = oracle.policy.required_witnesses() as usize;
    if oracle.witnesses.len() < required {
        return Err(SealRefusal::Inconsistent(
            Obligation::O3OracleWitnesses,
            "fewer witnesses than the policy requires",
        ));
    }
    if oracle.witnesses.iter().any(|w| !w.is_complete()) {
        return Err(SealRefusal::Inconsistent(
            Obligation::O3OracleWitnesses,
            "an incomplete oracle witness was bound",
        ));
    }
    if oracle.multi_oracle_residual_hash.is_empty() {
        return Err(SealRefusal::Inconsistent(
            Obligation::O3OracleWitnesses,
            "the multi-oracle verdict identity is missing",
        ));
    }
    let mut witness_edges: Vec<EvidenceEdge> = oracle
        .witnesses
        .iter()
        .map(|w| EvidenceEdge::new(EvidenceRole::OracleWitness, w.id().canonical()))
        .collect();
    witness_edges.push(EvidenceEdge::new(
        EvidenceRole::DesignResult,
        format!(
            "phor.multi-oracle:{hash}",
            hash = oracle.multi_oracle_residual_hash
        ),
    ));
    obligations.push(one(Obligation::O3OracleWitnesses, witness_edges));

    // O4 — provenance.
    let build = evidence
        .build
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O4BuildProvenance))?;
    for (s, why) in [
        (build.source_hash.as_str().is_empty(), "source hash missing"),
        (build.object_hash.as_str().is_empty(), "object hash missing"),
        (
            build.compiler_receipt_hash.as_str().is_empty(),
            "compiler receipt hash missing",
        ),
    ] {
        if s {
            return Err(SealRefusal::Inconsistent(
                Obligation::O4BuildProvenance,
                why,
            ));
        }
    }
    if !build.independent_rebuild_checked {
        return Err(SealRefusal::Inconsistent(
            Obligation::O4BuildProvenance,
            "the independent rebuild was not checked",
        ));
    }
    obligations.push(one(
        Obligation::O4BuildProvenance,
        vec![
            EvidenceEdge::new(EvidenceRole::CandidateSource, build.source_hash.canonical()),
            EvidenceEdge::new(EvidenceRole::CandidateBuild, build.object_hash.canonical()),
            EvidenceEdge::new(
                EvidenceRole::CandidateBuild,
                build.compiler_receipt_hash.canonical(),
            ),
        ],
    ));

    // O5/O6 — design consistency and discovery regressions.
    let design = evidence
        .design
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O5DesignConsistency))?;
    if design.cases_run == 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O5DesignConsistency,
            "the design court ran no cases",
        ));
    }
    if design.cases_failed != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O5DesignConsistency,
            "the design court reported a failure",
        ));
    }
    obligations.push(one(
        Obligation::O5DesignConsistency,
        vec![EvidenceEdge::new(
            EvidenceRole::DesignResult,
            format!("phor.design:run={}", design.cases_run),
        )],
    ));
    if design.discovery_outstanding != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O6DiscoveryRegressions,
            "discovery counterexamples remain outstanding",
        ));
    }
    if design.discovery_regressions_consistent != design.discovery_regressions {
        return Err(SealRefusal::Inconsistent(
            Obligation::O6DiscoveryRegressions,
            "an admitted discovery regression is inconsistent in the final candidate",
        ));
    }
    let mut disc_edges = vec![EvidenceEdge::new(
        EvidenceRole::DiscoveryCounterexample,
        format!("phor.discovery:admitted={}", design.discovery_regressions),
    )];
    for i in 0..design.discovery_regressions {
        disc_edges.push(EvidenceEdge::new(
            EvidenceRole::ReductionRecord,
            format!("phor.reduction:{i}"),
        ));
    }
    obligations.push(one(Obligation::O6DiscoveryRegressions, disc_edges));

    // O7 — held-out qualification.
    let qual = evidence
        .qualification
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O7Qualification))?;
    if qual.receipt_id.is_empty() || qual.universe_id.is_empty() {
        return Err(SealRefusal::Inconsistent(
            Obligation::O7Qualification,
            "the qualification receipt or universe identity is missing",
        ));
    }
    if !qual.isolated {
        return Err(SealRefusal::Inconsistent(
            Obligation::O7Qualification,
            "the qualification isolation audit did not hold",
        ));
    }
    if qual.cases_run == 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O7Qualification,
            "the qualification universe ran no cases",
        ));
    }
    if qual.cases_failed != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O7Qualification,
            "the candidate failed held-out qualification",
        ));
    }
    obligations.push(one(
        Obligation::O7Qualification,
        vec![
            EvidenceEdge::new(
                EvidenceRole::QualificationResult,
                qual.receipt_id.canonical(),
            ),
            EvidenceEdge::new(
                EvidenceRole::QualificationResult,
                format!("phor.qualification-universe:{}", qual.universe_id),
            ),
        ],
    ));

    // O8 — court sensitivity.
    let challenge = evidence
        .challenge
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O8CourtSensitivity))?;
    if challenge.receipt_id.is_empty() {
        return Err(SealRefusal::Inconsistent(
            Obligation::O8CourtSensitivity,
            "the challenge receipt identity is missing",
        ));
    }
    if challenge.families_declared == 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O8CourtSensitivity,
            "no mutation families were declared",
        ));
    }
    if challenge.families_undetermined != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O8CourtSensitivity,
            "a mutation family outcome is undetermined",
        ));
    }
    if challenge.families_detected
        != challenge
            .families_declared
            .saturating_sub(challenge.families_equivalent_excluded)
    {
        return Err(SealRefusal::Inconsistent(
            Obligation::O8CourtSensitivity,
            "the court did not detect every non-equivalent declared family",
        ));
    }
    obligations.push(one(
        Obligation::O8CourtSensitivity,
        vec![EvidenceEdge::new(
            EvidenceRole::ChallengeResult,
            challenge.receipt_id.canonical(),
        )],
    ));

    // O9 — FRF receipts verify.
    let frf = evidence
        .frf
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O9FrfReceipts))?;
    if !frf.verified {
        return Err(SealRefusal::Inconsistent(
            Obligation::O9FrfReceipts,
            "FRF did not verify the required receipts",
        ));
    }
    if frf.receipts.is_empty() {
        return Err(SealRefusal::Inconsistent(
            Obligation::O9FrfReceipts,
            "no FRF receipt is bound",
        ));
    }
    if frf.receipts.iter().any(|r| r.is_empty()) {
        return Err(SealRefusal::Inconsistent(
            Obligation::O9FrfReceipts,
            "an empty FRF receipt reference was bound",
        ));
    }
    let mut frf_edges: Vec<EvidenceEdge> = frf
        .receipts
        .iter()
        .map(|r| EvidenceEdge::new(EvidenceRole::FrfReceipt, r.canonical()))
        .collect();
    for c in &frf.claims {
        if c.is_empty() {
            return Err(SealRefusal::Inconsistent(
                Obligation::O9FrfReceipts,
                "an empty FRF claim reference was bound",
            ));
        }
        frf_edges.push(EvidenceEdge::new(EvidenceRole::FrfClaim, c.canonical()));
    }
    obligations.push(one(Obligation::O9FrfReceipts, frf_edges));

    // O10 — ordinary uninstrumented execution.
    let execution = evidence
        .execution
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O10UninstrumentedExecution))?;
    if !execution.uninstrumented {
        return Err(SealRefusal::Inconsistent(
            Obligation::O10UninstrumentedExecution,
            "the executed artifact was not the ordinary uninstrumented object",
        ));
    }
    if execution.cases_run == 0 || execution.cases_failed != 0 {
        return Err(SealRefusal::Inconsistent(
            Obligation::O10UninstrumentedExecution,
            "the execution court did not pass every case",
        ));
    }
    if execution.object_hash != build.object_hash {
        return Err(SealRefusal::Inconsistent(
            Obligation::O10UninstrumentedExecution,
            "the executed object is not the built object",
        ));
    }
    obligations.push(one(
        Obligation::O10UninstrumentedExecution,
        vec![EvidenceEdge::new(
            EvidenceRole::CandidateExecution,
            execution.object_hash.canonical(),
        )],
    ));

    // O11 — dispatch court.
    let dispatch = evidence
        .dispatch
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O11Dispatch))?;
    if dispatch.object_hash != build.object_hash {
        return Err(SealRefusal::Inconsistent(
            Obligation::O11Dispatch,
            "the dispatched object is not the built object",
        ));
    }
    if dispatch.cases_run == 0
        || dispatch.fallback_cases != 0
        || dispatch.broken_seal_cases != 0
        || dispatch.cases_failed != 0
    {
        return Err(SealRefusal::Inconsistent(
            Obligation::O11Dispatch,
            "the runtime did not serve every case from the sealed object",
        ));
    }
    if dispatch.native_cases != dispatch.cases_run {
        return Err(SealRefusal::Inconsistent(
            Obligation::O11Dispatch,
            "fewer native cases than the dispatch corpus",
        ));
    }
    obligations.push(one(
        Obligation::O11Dispatch,
        vec![EvidenceEdge::new(
            EvidenceRole::DispatchResult,
            format!("phor.dispatch:native={}", dispatch.native_cases),
        )],
    ));

    // O12 — evidence closure.
    let closure = evidence
        .closure
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O12EvidenceClosure))?;
    let closure_id = closure.id().map_err(SealRefusal::Closure)?;
    // The closure must bind the candidate artifact; a closure that omits the
    // very thing being sealed is not a closure.
    if !closure.contains(&build.object_hash.canonical()) {
        return Err(SealRefusal::Inconsistent(
            Obligation::O12EvidenceClosure,
            "the closure does not bind the candidate object",
        ));
    }
    obligations.push(one(
        Obligation::O12EvidenceClosure,
        vec![EvidenceEdge::new(
            EvidenceRole::PromotionReceipt,
            closure_id.canonical(),
        )],
    ));

    // O13 — store-generation relation.
    let store = evidence
        .store
        .clone()
        .ok_or(SealRefusal::Missing(Obligation::O13StoreGeneration))?;
    if store.current.is_empty() {
        return Err(SealRefusal::Inconsistent(
            Obligation::O13StoreGeneration,
            "the current store generation is missing",
        ));
    }
    match &store.parent {
        Some(p) => {
            if p.is_empty() {
                return Err(SealRefusal::Inconsistent(
                    Obligation::O13StoreGeneration,
                    "the parent store generation is empty",
                ));
            }
            if p == &store.current {
                return Err(SealRefusal::Inconsistent(
                    Obligation::O13StoreGeneration,
                    "a generation cannot descend from itself",
                ));
            }
        }
        None => {
            if store.parent_required {
                return Err(SealRefusal::Inconsistent(
                    Obligation::O13StoreGeneration,
                    "a parent generation is required but absent",
                ));
            }
        }
    }
    let mut store_edges = vec![EvidenceEdge::new(
        EvidenceRole::StoreGeneration,
        store.current.canonical(),
    )];
    if let Some(p) = &store.parent {
        store_edges.push(EvidenceEdge::new(
            EvidenceRole::PreviousStoreGeneration,
            p.canonical(),
        ));
    }
    obligations.push(one(Obligation::O13StoreGeneration, store_edges));

    // The receipt's own identity.
    let mut pre = Vec::with_capacity(AUTONOMOUS_SEAL_DOMAIN.len() + 256);
    pre.extend_from_slice(AUTONOMOUS_SEAL_DOMAIN);
    pre.extend_from_slice(
        format!(
            "profile={};closure={};obligations={}",
            profile.as_str(),
            closure_id.as_str(),
            obligations
                .iter()
                .map(|o| o.obligation.id())
                .collect::<Vec<_>>()
                .join(",")
        )
        .as_bytes(),
    );

    Ok(AutonomousPromotionReceipt {
        schema_version: AutonomousPromotionReceipt::SCHEMA_VERSION,
        profile,
        obligations,
        closure: closure_id,
        residual_hash: sha256_hex(&pre),
    })
}

fn one(obligation: Obligation, evidence: Vec<EvidenceEdge>) -> ObligationReceipt {
    ObligationReceipt {
        obligation,
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::ident::{CandidateObjectHash, EvidenceClosure, EvidenceEdge, EvidenceRole};

    fn witness(implementation: &str, version: &str) -> OracleWitness {
        OracleWitness {
            contract: String::from("posix:strspn"),
            implementation: implementation.to_string(),
            implementation_version: version.to_string(),
            executable_identity: format!("sha256:{}", "a".repeat(64)),
            runtime_closure_identity: format!("sha256:{}", "b".repeat(64)),
            target: String::from("x86_64-unknown-linux"),
            architecture: String::from("x86_64"),
            environment: implementation.to_string(),
            locale: String::from("C"),
        }
    }

    fn valid() -> AutonomousSealEvidence {
        let object = CandidateObjectHash::new("object-hash");
        let closure = EvidenceClosure::new(vec![
            EvidenceEdge::new(EvidenceRole::PortSpec, "phor.portspec:spec"),
            EvidenceEdge::new(EvidenceRole::CandidateBuild, object.canonical()),
            EvidenceEdge::new(EvidenceRole::FrfReceipt, "frf.receipt:r1"),
        ]);
        AutonomousSealEvidence {
            port_spec_id: Some(PortSpecId::new("spec")),
            oracle: Some(OracleEvidence {
                cases_validated: 128,
                cases_out_of_contract_reaching_oracle: 0,
                policy: QualificationPolicy::MultiImplementation {
                    minimum_witnesses: 2,
                },
                witnesses: vec![witness("glibc", "2.44"), witness("musl", "1.2.5")],
                multi_oracle_residual_hash: String::from("multi-oracle-hash"),
            }),
            build: Some(BuildEvidence {
                source_hash: CandidateSourceHash::new("source-hash"),
                object_hash: object.clone(),
                compiler_receipt_hash: CompilerReceiptHash::new("receipt-hash"),
                independent_rebuild_matches: true,
                independent_rebuild_checked: true,
            }),
            design: Some(DesignEvidence {
                cases_run: 578,
                cases_failed: 0,
                discovery_regressions: 2,
                discovery_regressions_consistent: 2,
                discovery_outstanding: 0,
            }),
            qualification: Some(QualificationEvidence {
                receipt_id: QualificationReceiptId::new("qual-1"),
                universe_id: String::from("held-out-v1"),
                cases_run: 64,
                cases_failed: 0,
                isolated: true,
            }),
            challenge: Some(ChallengeEvidence {
                receipt_id: ChallengeReceiptId::new("challenge-1"),
                families_declared: 7,
                families_detected: 6,
                families_equivalent_excluded: 1,
                families_undetermined: 0,
            }),
            frf: Some(FrfEvidence {
                receipts: vec![FrfReceiptId::new("receipt-run-abc")],
                claims: vec![FrfClaimId::new("claim-1")],
                verified: true,
            }),
            execution: Some(ExecutionEvidence {
                object_hash: object.clone(),
                cases_run: 578,
                cases_failed: 0,
                uninstrumented: true,
            }),
            dispatch: Some(DispatchEvidence {
                object_hash: object.clone(),
                cases_run: 578,
                native_cases: 578,
                fallback_cases: 0,
                broken_seal_cases: 0,
                cases_failed: 0,
            }),
            closure: Some(closure),
            store: Some(StoreGenerationEvidence {
                current: StoreGenerationId::new("gen-1"),
                parent: Some(StoreGenerationId::new("gen-0")),
                parent_required: true,
                parent_seal_profile: Some(SealProfile::LegacyV1),
            }),
            gemel_refs: vec![],
            candidate_behavior_hash: Some(ArtifactHash::new("behavior-hash")),
        }
    }

    #[test]
    fn test_valid_evidence_seals_with_every_obligation() {
        let receipt = verify_autonomous_seal(SealProfile::AutonomousV1, &valid()).expect("seals");
        assert_eq!(receipt.profile, SealProfile::AutonomousV1);
        let ids: Vec<&str> = receipt
            .obligations
            .iter()
            .map(|o| o.obligation.id())
            .collect();
        assert_eq!(
            ids,
            ["O1", "O2", "O3", "O4", "O5", "O6", "O7", "O8", "O9", "O10", "O11", "O12", "O13"]
        );
        assert!(!receipt.closure.is_empty());
    }

    #[test]
    fn test_legacy_profile_is_not_sealed_here() {
        assert_eq!(
            verify_autonomous_seal(SealProfile::LegacyV1, &valid()),
            Err(SealRefusal::LegacyProfileNotAutonomous)
        );
    }

    #[test]
    fn test_every_missing_obligation_fails_closed() {
        let cases: [(fn(&mut AutonomousSealEvidence), Obligation); 13] = [
            (|e| e.port_spec_id = None, Obligation::O1PortSpecIdentity),
            (|e| e.oracle = None, Obligation::O2PreconditionValidation),
            (|e| e.build = None, Obligation::O4BuildProvenance),
            (|e| e.design = None, Obligation::O5DesignConsistency),
            (|e| e.qualification = None, Obligation::O7Qualification),
            (|e| e.challenge = None, Obligation::O8CourtSensitivity),
            (|e| e.frf = None, Obligation::O9FrfReceipts),
            (
                |e| e.execution = None,
                Obligation::O10UninstrumentedExecution,
            ),
            (|e| e.dispatch = None, Obligation::O11Dispatch),
            (|e| e.closure = None, Obligation::O12EvidenceClosure),
            (|e| e.store = None, Obligation::O13StoreGeneration),
            (
                |e| e.port_spec_id = Some(PortSpecId::new("")),
                Obligation::O1PortSpecIdentity,
            ),
            (|e| e.oracle = None, Obligation::O2PreconditionValidation),
        ];
        for (mutate, expected) in cases {
            let mut e = valid();
            mutate(&mut e);
            match verify_autonomous_seal(SealProfile::AutonomousV1, &e) {
                Err(SealRefusal::Missing(o)) | Err(SealRefusal::Inconsistent(o, _)) => {
                    assert_eq!(o, expected, "wrong obligation reported")
                }
                other => panic!("expected refusal for {}, got {other:?}", expected.id()),
            }
        }
    }

    #[test]
    fn test_out_of_contract_oracle_case_is_refused() {
        let mut e = valid();
        e.oracle
            .as_mut()
            .unwrap()
            .cases_out_of_contract_reaching_oracle = 1;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(
                Obligation::O2PreconditionValidation,
                _
            ))
        ));
    }

    #[test]
    fn test_insufficient_or_incomplete_witnesses_are_refused() {
        let mut e = valid();
        e.oracle.as_mut().unwrap().witnesses.pop();
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O3OracleWitnesses, _))
        ));
        let mut e = valid();
        e.oracle.as_mut().unwrap().witnesses[0]
            .runtime_closure_identity
            .clear();
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O3OracleWitnesses, _))
        ));
    }

    #[test]
    fn test_artifact_authority_is_cross_checked() {
        let mut e = valid();
        e.execution.as_mut().unwrap().object_hash = CandidateObjectHash::new("other");
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(
                Obligation::O10UninstrumentedExecution,
                _
            ))
        ));
    }

    #[test]
    fn test_instrumented_artifact_can_never_be_promoted() {
        let mut e = valid();
        e.execution.as_mut().unwrap().uninstrumented = false;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(
                Obligation::O10UninstrumentedExecution,
                _
            ))
        ));
    }

    #[test]
    fn test_qualification_leakage_and_failure_are_refused() {
        let mut e = valid();
        e.qualification.as_mut().unwrap().isolated = false;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O7Qualification, _))
        ));
        let mut e = valid();
        e.qualification.as_mut().unwrap().cases_failed = 1;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O7Qualification, _))
        ));
    }

    #[test]
    fn test_court_blindness_and_undetermined_mutants_are_refused() {
        let mut e = valid();
        e.challenge.as_mut().unwrap().families_detected = 5;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O8CourtSensitivity, _))
        ));
        let mut e = valid();
        e.challenge.as_mut().unwrap().families_undetermined = 1;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O8CourtSensitivity, _))
        ));
    }

    #[test]
    fn test_unverified_frf_is_refused() {
        let mut e = valid();
        e.frf.as_mut().unwrap().verified = false;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O9FrfReceipts, _))
        ));
    }

    #[test]
    fn test_dispatch_fallback_is_refused() {
        let mut e = valid();
        e.dispatch.as_mut().unwrap().fallback_cases = 1;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O11Dispatch, _))
        ));
    }

    #[test]
    fn test_closure_must_bind_the_candidate_object() {
        let mut e = valid();
        e.closure = Some(EvidenceClosure::new(vec![EvidenceEdge::new(
            EvidenceRole::PortSpec,
            "phor.portspec:spec",
        )]));
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O12EvidenceClosure, _))
        ));
    }

    #[test]
    fn test_store_generation_relation_is_checked() {
        let mut e = valid();
        let cur = e.store.as_ref().unwrap().current.clone();
        e.store.as_mut().unwrap().parent = Some(cur);
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O13StoreGeneration, _))
        ));
        let mut e = valid();
        e.store.as_mut().unwrap().parent = None;
        assert!(matches!(
            verify_autonomous_seal(SealProfile::AutonomousV1, &e),
            Err(SealRefusal::Inconsistent(Obligation::O13StoreGeneration, _))
        ));
    }

    #[test]
    fn test_receipt_is_deterministic_and_identity_sensitive() {
        let a = verify_autonomous_seal(SealProfile::AutonomousV1, &valid()).unwrap();
        let b = verify_autonomous_seal(SealProfile::AutonomousV1, &valid()).unwrap();
        assert_eq!(a, b);
        let mut e = valid();
        e.port_spec_id = Some(PortSpecId::new("another-spec"));
        // A changed spec changes the closure? No: the closure is supplied, so
        // change the closure too (as the host would). Then the receipt differs.
        e.closure = Some(EvidenceClosure::new(vec![
            EvidenceEdge::new(EvidenceRole::PortSpec, "phor.portspec:another-spec"),
            EvidenceEdge::new(
                EvidenceRole::CandidateBuild,
                "phor.object-sha256:object-hash",
            ),
        ]));
        let c = verify_autonomous_seal(SealProfile::AutonomousV1, &e).unwrap();
        assert_ne!(a.residual_hash, c.residual_hash);
        assert_ne!(a.closure, c.closure);
    }

    #[test]
    fn test_refusal_message_names_the_obligation_without_echoing_evidence() {
        let mut e = valid();
        e.qualification.as_mut().unwrap().cases_failed = 3;
        let refusal = verify_autonomous_seal(SealProfile::AutonomousV1, &e).unwrap_err();
        let msg = refusal.describe();
        assert!(msg.contains("O7"));
        // It never echoes an oracle answer or a case payload.
        assert!(!msg.contains("0x"));
        assert!(!msg.contains("held-out-v1"));
    }
}
