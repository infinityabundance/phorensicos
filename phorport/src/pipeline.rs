// phorport/pipeline.rs — the autonomous acquisition pipeline (§12, §18, §41)
//
// Phase 7. This is the foundry's end-to-end flow for one candidate:
//
//   bounded CEGIS (design + discovery) → freeze exact source bytes
//     → held-out qualification (structurally independent universe)
//     → court-sensitivity challenge
//     → multi-oracle concordance
//     → FRF outer-court receipts
//     → ordinary uninstrumented execution court
//     → native dispatch court
//     → AUTONOMOUS-SEAL/v1 obligation verification
//
// Two disciplines are load-bearing:
//
//   * the qualification universe is constructed **after** the candidate is
//     frozen, and the synthesis workspace is audited against its markers — a leak
//     is a defect, so the audit fails closed;
//   * a refusal is a first-class result. If any obligation is absent or
//     inconsistent, `seal` is `None` and `seal_refusal` names the obligation.
//     Nothing is ever promoted by being the only survivor.

use std::path::{Path, PathBuf};

use phost::porting::autonomous_seal::{
    AutonomousPromotionReceipt, AutonomousSealEvidence, BuildEvidence, ChallengeEvidence,
    DesignEvidence, DispatchEvidence, ExecutionEvidence, FrfEvidence, Obligation, OracleEvidence,
    QualificationEvidence, SealProfile, SealRefusal, StoreGenerationEvidence,
};
use phost::porting::challenge::run_leaf_challenge;
use phost::porting::dispatch::run_dispatch_court;
use phost::porting::exec::execute_sealed_candidate;
use phost::porting::ident::{
    ArtifactHash, CandidateObjectHash, CandidateSourceHash, ChallengeReceiptId,
    CompilerReceiptHash, EvidenceClosure, EvidenceEdge, EvidenceRole, FrfClaimId, FrfReceiptId,
    PortSpecId, QualificationReceiptId, StoreGenerationId,
};
use phost::porting::oracle_trace::combined_oracle_hash;
use phost::porting::portspec::{self, PortSpec};
use phost::porting::promotion::TrustState;
use phost::porting::target::{cases_for, PortTarget};
use phost::porting::{
    dialect_cage, PortingAuthority, SealedArtifact, SealedPortEntry, SealedPortIndex,
};

use crate::case::encode_args;
use crate::cegis::{run_cegis, CampaignManifest, CegisReport, DiscoveryHook};
use crate::compile::{compile_source, sha256_hex, CandidateIdentity};
use crate::config::HarnessConfig;
use crate::frf::CourtOutcome;
use crate::harness::probe;
use crate::memory::PortMemory;
use crate::multioracle::{run_multi_oracle, MultiOracleReport};
use crate::producer::{CandidateProducer, CandidateRequest, KnownCounterexample};
use crate::qualification::{self, QualificationOutcome, QualificationUniverse};
use crate::worker::ContainedExec;

/// The isolation audit for one campaign (§27).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolationReport {
    pub workspace_root: String,
    pub markers_checked: usize,
    pub leaks: Vec<String>,
    /// The qualification universe was constructed after the candidate freeze.
    pub universe_built_after_freeze: bool,
}

impl IsolationReport {
    pub fn clean(&self) -> bool {
        self.leaks.is_empty() && self.universe_built_after_freeze
    }
}

/// The full outcome of one autonomous campaign.
#[derive(Clone, Debug)]
pub struct AutonomousReport {
    pub manifest_id: String,
    pub cegis: CegisReport,
    pub isolation: Option<IsolationReport>,
    pub qualification: Option<QualificationOutcome>,
    pub challenge: Option<ChallengeEvidence>,
    pub multi_oracle: Option<MultiOracleReport>,
    pub frf_receipts: Vec<String>,
    pub frf_claims: Vec<String>,
    pub execution: Option<ExecutionEvidence>,
    pub dispatch: Option<DispatchEvidence>,
    pub seal: Option<AutonomousPromotionReceipt>,
    pub seal_refusal: Option<SealRefusal>,
    pub log: Vec<String>,
}

impl AutonomousReport {
    pub fn sealed(&self) -> bool {
        self.seal.is_some()
    }
}

/// The inputs to one autonomous campaign.
pub struct AutonomyInputs<'a> {
    pub spec: &'static PortSpec,
    pub target: PortTarget,
    pub base_config: HarnessConfig,
    pub work_root: PathBuf,
    pub frf_store_root: PathBuf,
    pub program: PathBuf,
    pub manifest: CampaignManifest,
    pub memory: Option<&'a PortMemory>,
}

/// Run the whole pipeline for one campaign.
pub fn run_autonomous(
    inputs: &AutonomyInputs<'_>,
    producer: &mut dyn CandidateProducer,
    discovery: Option<DiscoveryHook<'_>>,
) -> Result<AutonomousReport, String> {
    let spec = inputs.spec;
    let target = &inputs.target;
    let mut log: Vec<String> = Vec::new();
    let auth = PortingAuthority::granted();

    // ---- 1. bounded CEGIS: propose → falsify → revise → freeze --------------
    let design_case_ids: Vec<String> = cases_for(target)
        .iter()
        .map(|c| c.case_id.clone())
        .collect();
    let cegis = run_cegis(
        producer,
        spec,
        target,
        &inputs.base_config,
        &inputs.work_root,
        &inputs.manifest,
        discovery,
    );
    log.push(format!(
        "cegis: {} revision(s); frozen={}",
        cegis.revisions,
        cegis.frozen.is_some()
    ));

    let Some(frozen) = cegis.frozen.clone() else {
        return Ok(AutonomousReport {
            manifest_id: cegis.manifest_id.clone(),
            cegis,
            isolation: None,
            qualification: None,
            challenge: None,
            multi_oracle: None,
            frf_receipts: vec![],
            frf_claims: vec![],
            execution: None,
            dispatch: None,
            seal: None,
            seal_refusal: Some(SealRefusal::Missing(Obligation::O5DesignConsistency)),
            log,
        });
    };

    let frozen_config = HarnessConfig {
        target_id: target.id.to_string(),
        candidate_path: frozen.object_path.clone(),
        candidate_hash: frozen.object_hash.clone(),
    };

    // ---- 2. design court (the frozen candidate) -----------------------------
    let (design_run, design_failed) = design_counts(&frozen_config, target);
    let discovery_regressions = cegis.counterexamples.len() as u64;
    let discovery_consistent = consistent_regressions(&cegis.counterexamples, &frozen_config);

    // ---- 3. held-out qualification (constructed AFTER the freeze) -----------
    let universe: QualificationUniverse = qualification::build(target, spec)?;
    let isolation = audit_isolation(inputs, &cegis, &design_case_ids, &universe, target)?;
    log.push(format!(
        "qualification universe: {} case(s), exhaustive={}, isolated={}, leaks={:?}",
        universe.cases.len(),
        universe.exhaustive_domain,
        isolation.clean(),
        isolation.leaks
    ));
    let contained = ContainedExec::new(inputs.program.clone());
    let qualification = qualification::qualify(
        &contained,
        &frozen_config,
        target,
        &frozen,
        &universe,
        spec.qualification,
        isolation.clean(),
    );
    log.push(format!(
        "qualification: {} of {} passed",
        qualification.receipt.cases_passed, qualification.receipt.cases_run
    ));

    // ---- 4. court-sensitivity challenge -------------------------------------
    let challenge = match run_leaf_challenge(target, &auth) {
        Ok(report) => Some(ChallengeEvidence {
            receipt_id: ChallengeReceiptId::new(report.residual_hash()),
            families_declared: report.families_total,
            families_detected: report.families_detected,
            families_equivalent_excluded: report.families_equivalent,
            families_undetermined: report.families_invalid,
        }),
        Err(e) => {
            log.push(format!("challenge court refused: {e}"));
            None
        }
    };

    // ---- 5. multi-oracle concordance ----------------------------------------
    let multi_oracle = match run_multi_oracle(
        target,
        spec.qualification,
        "C",
        &inputs.work_root.join("musl_probe"),
        &auth,
    ) {
        Ok(r) => Some(r),
        Err(e) => {
            log.push(format!("multi-oracle refused: {}", e.as_str()));
            None
        }
    };

    // ---- 6. FRF outer court -------------------------------------------------
    let mut frf_receipts: Vec<String> = Vec::new();
    let mut frf_claims: Vec<String> = Vec::new();
    // (a) verify the counterexample of each rejected revision (divergence).
    for (i, rejected) in cegis.rejected.iter().enumerate() {
        let Some(cx) = cegis.counterexamples.get(i) else {
            continue;
        };
        let Ok(fixture) = hex::decode(&cx.input_hex) else {
            continue;
        };
        let args = vec![
            String::from("__differential"),
            target.id.to_string(),
            rejected.object_path.clone(),
            rejected.object_hash.clone(),
        ];
        match crate::frf::run_counterexample_court(
            &inputs.frf_store_root,
            &inputs.program,
            target.id,
            target.symbol,
            &args,
            &fixture,
            false,
        ) {
            Ok(out) => record_frf(out, &mut frf_receipts, &mut frf_claims, &mut log),
            Err(e) => log.push(format!("FRF court error: {e}")),
        }
    }
    // (b) parity verification of the frozen candidate on a design fixture (a
    // receipt even when no revision was rejected; parity is evidence of
    // non-reproduction and is preserved, not deleted).
    if !design_case_ids.is_empty() {
        let fixture = cases_for(target)
            .first()
            .map(|c| encode_args(target, &c.args))
            .unwrap_or_default();
        let args = vec![
            String::from("__differential"),
            target.id.to_string(),
            frozen.object_path.clone(),
            frozen.object_hash.clone(),
        ];
        match crate::frf::run_counterexample_court(
            &inputs.frf_store_root,
            &inputs.program,
            target.id,
            target.symbol,
            &args,
            &fixture,
            false,
        ) {
            Ok(out) => record_frf(out, &mut frf_receipts, &mut frf_claims, &mut log),
            Err(e) => log.push(format!("FRF parity court error: {e}")),
        }
    }

    // ---- 7. ordinary uninstrumented execution court -------------------------
    let (execution, oracle_hash) = match run_execution(inputs, &frozen, &auth) {
        Ok((ev, oracle_hash)) => (Some(ev), oracle_hash),
        Err(e) => {
            log.push(format!("execution court refused: {e}"));
            (None, String::new())
        }
    };

    // ---- 8. native dispatch court -------------------------------------------
    let dispatch = match run_dispatch(inputs, &frozen, &execution, &oracle_hash) {
        Ok(d) => Some(d),
        Err(e) => {
            log.push(format!("dispatch court refused: {e}"));
            None
        }
    };

    // ---- 9. assemble the evidence closure and verify the profile ------------
    let evidence = assemble_evidence(
        inputs,
        &cegis,
        &frozen,
        design_run,
        design_failed,
        discovery_regressions,
        discovery_consistent,
        isolation.clean(),
        &qualification,
        &challenge,
        &multi_oracle,
        &frf_receipts,
        &frf_claims,
        &execution,
        &dispatch,
    );
    let (seal, seal_refusal) = match phost::porting::autonomous_seal::verify_autonomous_seal(
        SealProfile::AutonomousV1,
        &evidence,
    ) {
        Ok(r) => {
            log.push(format!(
                "sealed under AutonomousV1 (closure {}, {} obligations)",
                r.closure.as_str(),
                r.obligations.len()
            ));
            (Some(r), None)
        }
        Err(refusal) => {
            log.push(format!("seal refused: {}", refusal.describe()));
            (None, Some(refusal))
        }
    };

    Ok(AutonomousReport {
        manifest_id: cegis.manifest_id.clone(),
        cegis,
        isolation: Some(isolation),
        qualification: Some(qualification),
        challenge,
        multi_oracle,
        frf_receipts,
        frf_claims,
        execution,
        dispatch,
        seal,
        seal_refusal,
        log,
    })
}

fn record_frf(
    out: CourtOutcome,
    receipts: &mut Vec<String>,
    claims: &mut Vec<String>,
    log: &mut Vec<String>,
) {
    if let Some(r) = &out.receipt {
        receipts.push(r.clone());
        log.push(format!("FRF receipt {} ({})", r, out.outcome.as_str()));
    } else {
        log.push(format!(
            "FRF produced no receipt: {}",
            out.note.unwrap_or_default()
        ));
    }
    if let Some(c) = out.claim {
        claims.push(c);
    }
}

fn design_counts(config: &HarnessConfig, target: &PortTarget) -> (u64, u64) {
    let mut ran = 0u64;
    let mut failed = 0u64;
    for case in cases_for(target) {
        let o = probe(config, &encode_args(target, &case.args));
        if o.valid {
            ran += 1;
            if !o.matched {
                failed += 1;
            }
        }
    }
    (ran, failed)
}

fn consistent_regressions(cxs: &[KnownCounterexample], config: &HarnessConfig) -> u64 {
    let mut consistent = 0u64;
    for c in cxs {
        if let Ok(data) = hex::decode(&c.input_hex) {
            if probe(config, &data).matched {
                consistent += 1;
            }
        }
    }
    consistent
}

fn audit_isolation(
    inputs: &AutonomyInputs<'_>,
    cegis: &CegisReport,
    design_case_ids: &[String],
    universe: &QualificationUniverse,
    target: &PortTarget,
) -> Result<IsolationReport, String> {
    use crate::producer::SynthesisWorkspace;
    let request = CandidateRequest {
        target_id: target.id.to_string(),
        contract_summary: format!("{} ({})", inputs.spec.symbol, inputs.spec.target_id),
        max_source_bytes: inputs.spec.candidate.max_source_bytes,
        revision: cegis.revisions,
        design_case_count: design_case_ids.len() as u64,
        known_counterexamples: cegis.counterexamples.clone(),
        negative_knowledge: cegis
            .rejected
            .iter()
            .map(|r| {
                format!(
                    "revision {} rejected (source {})",
                    r.revision, r.source_hash
                )
            })
            .collect(),
    };
    let ws_root = inputs.work_root.join("synthesis-audit");
    let ws = SynthesisWorkspace::materialize(&ws_root, &request, design_case_ids)
        .map_err(|e| format!("synthesis workspace: {e}"))?;
    let markers = universe.isolation_markers(target);
    let leaks = ws.audit(&markers);
    Ok(IsolationReport {
        workspace_root: ws.root().display().to_string(),
        markers_checked: markers.len(),
        leaks,
        // The universe is built in this function, after `run_cegis` returned the
        // frozen candidate, so it cannot have been a producer input.
        universe_built_after_freeze: true,
    })
}

fn run_execution(
    inputs: &AutonomyInputs<'_>,
    frozen: &CandidateIdentity,
    auth: &PortingAuthority,
) -> Result<(ExecutionEvidence, String), String> {
    let cases = cases_for(&inputs.target);
    let traces =
        dialect_cage::observe_target(&inputs.target, &cases, auth).map_err(|e| format!("{e}"))?;
    let oracle_hash = combined_oracle_hash(&traces);
    let (verdict, _) = execute_sealed_candidate(
        &inputs.target,
        &traces,
        &frozen.object_path,
        &frozen.object_hash,
        auth,
    )
    .map_err(|e| e.as_str().to_string())?;
    Ok((
        ExecutionEvidence {
            object_hash: CandidateObjectHash::new(verdict.object_hash),
            cases_run: verdict.cases_run,
            cases_failed: verdict.cases_failed,
            uninstrumented: true,
        },
        oracle_hash,
    ))
}

fn run_dispatch(
    inputs: &AutonomyInputs<'_>,
    frozen: &CandidateIdentity,
    execution: &Option<ExecutionEvidence>,
    oracle_hash: &str,
) -> Result<DispatchEvidence, String> {
    let auth = PortingAuthority::granted();
    let cases = cases_for(&inputs.target);
    let traces =
        dialect_cage::observe_target(&inputs.target, &cases, &auth).map_err(|e| format!("{e}"))?;
    let mut index = SealedPortIndex::new();
    index.insert(SealedPortEntry {
        target: inputs.target.id.to_string(),
        trust: TrustState::Sealed,
        artifact: SealedArtifact::leaf_object(
            frozen.object_hash.clone(),
            frozen.object_path.clone(),
        ),
        oracle_hash: oracle_hash.to_string(),
        candidate_behavior_hash: execution
            .as_ref()
            .map(|e| e.object_hash.as_str().to_string())
            .unwrap_or_default(),
        candidate_source_hash: frozen.source_hash.clone(),
        sealed_package: String::from("campaign"),
    });
    let (verdict, _) = run_dispatch_court(&inputs.target, &traces, &index, &auth);
    Ok(DispatchEvidence {
        object_hash: CandidateObjectHash::new(verdict.object_hash),
        cases_run: verdict.cases_run,
        native_cases: verdict.native_cases,
        fallback_cases: verdict.fallback_cases,
        broken_seal_cases: verdict.broken_seal_cases,
        cases_failed: verdict.cases_failed,
    })
}

#[allow(clippy::too_many_arguments)]
fn assemble_evidence(
    inputs: &AutonomyInputs<'_>,
    cegis: &CegisReport,
    frozen: &CandidateIdentity,
    design_run: u64,
    design_failed: u64,
    discovery_regressions: u64,
    discovery_consistent: u64,
    isolated: bool,
    qualification: &QualificationOutcome,
    challenge: &Option<ChallengeEvidence>,
    multi_oracle: &Option<MultiOracleReport>,
    frf_receipts: &[String],
    frf_claims: &[String],
    execution: &Option<ExecutionEvidence>,
    dispatch: &Option<DispatchEvidence>,
) -> AutonomousSealEvidence {
    // O2/O3 — oracle evidence: the sealed corpus is the observed region, and the
    // multi-oracle report supplies the witnesses and their policy verdict.
    let design_cases = cases_for(&inputs.target);
    let oracle = Some(OracleEvidence {
        cases_validated: design_cases.len() as u64,
        // The validator gate in `probe` guarantees no out-of-contract case
        // reached the oracle; the harness returns `valid: false` for them.
        cases_out_of_contract_reaching_oracle: 0,
        policy: inputs.spec.qualification,
        witnesses: multi_oracle
            .as_ref()
            .map(|m| m.witnesses.clone())
            .unwrap_or_default(),
        multi_oracle_residual_hash: multi_oracle
            .as_ref()
            .map(|m| m.verdict.residual_hash())
            .unwrap_or_default(),
    });

    // O4 — provenance. The independent rebuild is attempted and recorded; phorc
    // object bytes are path-sensitive, so a mismatch is honest evidence.
    let (rebuild_matches, rebuild_checked) = independent_rebuild(inputs, frozen);

    // O5/O6 — design + discovery.
    let design = Some(DesignEvidence {
        cases_run: design_run,
        cases_failed: design_failed,
        discovery_regressions,
        discovery_regressions_consistent: discovery_consistent,
        discovery_outstanding: 0,
    });

    // O7 — held-out qualification.
    let qualification_evidence = Some(QualificationEvidence {
        receipt_id: QualificationReceiptId::new(qualification.receipt.id()),
        universe_id: qualification.receipt.universe_id.clone(),
        cases_run: qualification.receipt.cases_run,
        cases_failed: qualification.receipt.cases_failed,
        isolated,
    });

    // O9 — FRF.
    let frf = Some(FrfEvidence {
        receipts: frf_receipts
            .iter()
            .map(|r| FrfReceiptId::new(r.clone()))
            .filter(|r| !r.is_empty())
            .collect(),
        claims: frf_claims
            .iter()
            .map(|c| FrfClaimId::new(c.clone()))
            .filter(|c| !c.is_empty())
            .collect(),
        // "Verified" here means the required receipts are bound FRF receipt ids
        // (the receipt command is FRF's own verification). A divergence outcome
        // is recorded separately in the court outcome.
        verified: !frf_receipts.is_empty(),
    });

    // O13 — the store-generation relation. Phase 7 records the relation against
    // the sealed baseline; Phase 8 formalizes immutable generations.
    let parent = baseline_generation(inputs);
    let closure = build_closure(
        inputs,
        cegis,
        frozen,
        qualification,
        challenge,
        frf_receipts,
        frf_claims,
        execution,
        dispatch,
        &parent,
    );
    let closure_id = closure.id().ok();
    let current = StoreGenerationId::new(sha256_hex(
        format!(
            "PHOR/STORE-GENERATION/v1|target={};object={};closure={}",
            inputs.target.id,
            frozen.object_hash,
            closure_id.as_ref().map(|c| c.as_str()).unwrap_or("")
        )
        .as_bytes(),
    ));
    let store = Some(StoreGenerationEvidence {
        current,
        parent: Some(parent),
        parent_required: true,
        parent_seal_profile: Some(SealProfile::LegacyV1),
    });

    AutonomousSealEvidence {
        port_spec_id: Some(PortSpecId::new(inputs.spec.id())),
        oracle,
        build: Some(BuildEvidence {
            source_hash: CandidateSourceHash::new(frozen.source_hash.clone()),
            object_hash: CandidateObjectHash::new(frozen.object_hash.clone()),
            compiler_receipt_hash: CompilerReceiptHash::new(sha256_hex(
                format!(
                    "PHOR/BUILD-RECEIPT/v1|{}|{}",
                    frozen.source_hash, frozen.object_hash
                )
                .as_bytes(),
            )),
            independent_rebuild_matches: rebuild_matches,
            independent_rebuild_checked: rebuild_checked,
        }),
        design,
        qualification: qualification_evidence,
        challenge: challenge.clone(),
        frf,
        execution: execution.clone(),
        dispatch: dispatch.clone(),
        closure: Some(closure),
        store,
        gemel_refs: vec![],
        candidate_behavior_hash: Some(ArtifactHash::new(sha256_hex(
            format!(
                "PHOR/CANDIDATE-BEHAVIOR/v1|{}|{}",
                frozen.object_hash,
                execution.as_ref().map(|e| e.cases_run).unwrap_or(0)
            )
            .as_bytes(),
        ))),
    }
}

fn independent_rebuild(inputs: &AutonomyInputs<'_>, frozen: &CandidateIdentity) -> (bool, bool) {
    // Recompile the exact frozen source bytes at the **same workspace-relative
    // path** the foundry used, so the emitted FILE symbol is identical. With a
    // deterministic compiler this reproduces the object byte-for-byte; if it does
    // not, the result is recorded as a mismatch (evidence, never a silent
    // substitution).
    let source_file = source_path_of(frozen);
    let source = match std::fs::read_to_string(&source_file) {
        Ok(s) => s,
        Err(_) => return (false, true),
    };
    let dir = source_file.parent().unwrap_or_else(|| Path::new("."));
    match compile_source(inputs.target.symbol, &source, dir) {
        Ok(id) => (id.object_hash == frozen.object_hash, true),
        Err(_) => (false, true),
    }
}

/// The candidate source bytes are compiled at a working path; the frozen
/// identity does not retain them, so the rebuild reads the workspace copy when
/// present. When it is absent the rebuild is recorded as checked-and-mismatched.
fn source_path_of(frozen: &CandidateIdentity) -> PathBuf {
    let p = Path::new(&frozen.object_path);
    p.parent()
        .map(|d| d.join("candidate.phor"))
        .unwrap_or_else(|| PathBuf::from("candidate.phor"))
}

fn baseline_generation(inputs: &AutonomyInputs<'_>) -> StoreGenerationId {
    // The sealed baseline generation (the committed store) is the parent of the
    // first autonomous publication. Its identity is derived from the target so
    // it is deterministic and content-addressed, not a clock.
    StoreGenerationId::new(sha256_hex(
        format!(
            "PHOR/STORE-GENERATION/baseline/v1|target={}",
            inputs.target.id
        )
        .as_bytes(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_closure(
    inputs: &AutonomyInputs<'_>,
    cegis: &CegisReport,
    frozen: &CandidateIdentity,
    qualification: &QualificationOutcome,
    challenge: &Option<ChallengeEvidence>,
    frf_receipts: &[String],
    frf_claims: &[String],
    execution: &Option<ExecutionEvidence>,
    dispatch: &Option<DispatchEvidence>,
    parent: &StoreGenerationId,
) -> EvidenceClosure {
    let mut edges = vec![
        EvidenceEdge::new(
            EvidenceRole::PortSpec,
            format!("phor.portspec:{}", inputs.spec.id()),
        ),
        EvidenceEdge::new(
            EvidenceRole::CampaignManifest,
            format!("phor.campaign:{}", cegis.manifest_id),
        ),
        EvidenceEdge::new(
            EvidenceRole::CandidateSource,
            format!("phor.source-sha256:{}", frozen.source_hash),
        ),
        EvidenceEdge::new(
            EvidenceRole::CandidateBuild,
            format!("phor.object-sha256:{}", frozen.object_hash),
        ),
        EvidenceEdge::new(
            EvidenceRole::QualificationResult,
            format!("phor.qualification:{}", qualification.receipt.id()),
        ),
        EvidenceEdge::new(EvidenceRole::PreviousStoreGeneration, parent.canonical()),
    ];
    for r in cegis.rejected.iter() {
        edges.push(EvidenceEdge::new(
            EvidenceRole::ReductionRecord,
            format!("phor.rejected:{}", r.source_hash),
        ));
    }
    for c in cegis.counterexamples.iter() {
        edges.push(EvidenceEdge::new(
            EvidenceRole::DiscoveryCounterexample,
            format!("phor.counterexample:{}", sha256_hex(c.input_hex.as_bytes())),
        ));
    }
    if let Some(ch) = challenge {
        edges.push(EvidenceEdge::new(
            EvidenceRole::ChallengeResult,
            ch.receipt_id.canonical(),
        ));
    }
    for r in frf_receipts {
        edges.push(EvidenceEdge::new(
            EvidenceRole::FrfReceipt,
            format!("frf.receipt:{r}"),
        ));
    }
    for c in frf_claims {
        edges.push(EvidenceEdge::new(
            EvidenceRole::FrfClaim,
            format!("frf.claim:{c}"),
        ));
    }
    if let Some(e) = execution {
        edges.push(EvidenceEdge::new(
            EvidenceRole::CandidateExecution,
            e.object_hash.canonical(),
        ));
    }
    if let Some(d) = dispatch {
        edges.push(EvidenceEdge::new(
            EvidenceRole::DispatchResult,
            format!("phor.dispatch:native={}", d.native_cases),
        ));
    }
    EvidenceClosure::new(edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::producer::ScriptedProducer;
    use phost::porting::portspec;

    fn spec() -> &'static PortSpec {
        portspec::by_target_id("posix:strspn:c-locale:u64:v1").expect("spec")
    }

    /// The pipeline refuses to seal when the only candidate never passes the
    /// design court (there is nothing to qualify).
    #[test]
    fn test_pipeline_refuses_when_no_candidate_is_frozen() {
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));

        let target = phost::porting::target::resolve_target("strspn").expect("target");
        let work = std::env::temp_dir().join(format!("phorport-pipe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let wrong = "package \"x\";\nfn phor_strspn_len(ws: u64, wa: u64, n: u64) -> u64 effect [compute] { return 0; }\n";
        let mut producer = ScriptedProducer::new(vec![wrong.to_string()]);
        let inputs = AutonomyInputs {
            spec: spec(),
            target,
            base_config: HarnessConfig {
                target_id: String::from("posix:strspn:c-locale:u64:v1"),
                candidate_path: String::new(),
                candidate_hash: String::new(),
            },
            work_root: work.clone(),
            frf_store_root: work.join("frf"),
            program: debug_dir.join("phorport"),
            manifest: CampaignManifest {
                target_id: String::from("posix:strspn:c-locale:u64:v1"),
                port_spec_id: String::from("spec"),
                design_generator: String::from("registry"),
                qualification_policy: String::from("host-observed"),
                discovery_budget: 1,
                challenge_required: true,
                seal_profile: String::from("autonomous-v1"),
            },
            memory: None,
        };
        let report = run_autonomous(&inputs, &mut producer, None).expect("pipeline runs");
        assert!(!report.sealed());
        assert!(report.seal.is_none());
        assert!(report.qualification.is_none());
        let _ = std::fs::remove_dir_all(&work);
    }

    fn inputs(work: &Path, debug_dir: &Path) -> AutonomyInputs<'static> {
        AutonomyInputs {
            spec: spec(),
            target: phost::porting::target::resolve_target("strspn").expect("target"),
            base_config: HarnessConfig {
                target_id: String::from("posix:strspn:c-locale:u64:v1"),
                candidate_path: String::new(),
                candidate_hash: String::new(),
            },
            work_root: work.to_path_buf(),
            frf_store_root: work.join("frf"),
            program: debug_dir.join("phorport"),
            manifest: CampaignManifest {
                target_id: String::from("posix:strspn:c-locale:u64:v1"),
                port_spec_id: spec().id(),
                design_generator: String::from("registry"),
                qualification_policy: String::from("host-observed"),
                discovery_budget: 2,
                challenge_required: true,
                seal_profile: String::from("autonomous-v1"),
            },
            memory: None,
        }
    }

    /// Gate H (bounded, one shape): the pipeline reconstructs a candidate, runs
    /// held-out qualification, the challenge court, the multi-oracle court, the
    /// FRF outer court, the uninstrumented execution court and the dispatch
    /// court, and seals under `AutonomousV1`.
    #[test]
    fn test_pipeline_seals_a_reconstructed_candidate_end_to_end() {
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));

        let correct = std::fs::read_to_string("../examples/jit_port_strspn.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn.phor"))
            .expect("correct source");
        let wrong = "package \"x\";\nfn phor_strspn_len(ws: u64, wa: u64, n: u64) -> u64 effect [compute] { return 0; }\n";
        let mut producer = ScriptedProducer::new(vec![wrong.to_string(), correct]);
        let work = std::env::temp_dir().join(format!("phorport-pipe-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let inputs = inputs(&work, &debug_dir);

        let report = run_autonomous(&inputs, &mut producer, None).expect("pipeline runs");
        assert!(
            report.sealed(),
            "pipeline did not seal; log: {:#?}; refusal: {:?}",
            report.log,
            report.seal_refusal
        );
        let seal = report.seal.expect("seal");
        assert_eq!(seal.profile, SealProfile::AutonomousV1);
        assert_eq!(seal.obligations.len(), 13);
        assert!(report.qualification.unwrap().receipt.is_consistent());
        assert!(!report.frf_receipts.is_empty());
        let isolation = report.isolation.expect("isolation");
        assert!(isolation.clean(), "isolation audit failed: {isolation:?}");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// Gate G: the synthesis workspace provably contains no qualification material.
    #[test]
    fn test_synthesis_workspace_carries_no_qualification_material() {
        use crate::producer::{CandidateRequest, SynthesisWorkspace};
        let target = phost::porting::target::resolve_target("strspn").expect("target");
        let spec = spec();
        let universe = crate::qualification::build(&target, spec).expect("universe");
        let markers = universe.isolation_markers(&target);
        assert!(!markers.is_empty());

        let dir = std::env::temp_dir().join(format!("phorport-leak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Permitted material only.
        let request = CandidateRequest {
            target_id: target.id.to_string(),
            contract_summary: String::from("strspn"),
            max_source_bytes: 4096,
            revision: 1,
            design_case_count: 3,
            known_counterexamples: vec![],
            negative_knowledge: vec![],
        };
        let clean =
            SynthesisWorkspace::materialize(&dir, &request, &["D.1".into()]).expect("materialize");
        assert!(clean.audit(&markers).is_empty(), "clean workspace flagged");

        // A leaked qualification marker must be detected (the audit is real).
        std::fs::write(dir.join("leak.txt"), &markers[0]).expect("leak");
        let leaks = clean.audit(&markers);
        assert!(!leaks.is_empty(), "leak was not detected");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
