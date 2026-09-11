// phorport/cegis.rs — the bounded CEGIS loop and the campaign state machine
//
// Phase 6. Candidate reconstruction is a loop: propose → falsify → revise, with a
// *bounded* discovery budget, until the candidate survives the declared evidence
// and is **frozen**. Only then can later phases qualify and promote it.
//
// Two disciplines are load-bearing here:
//
//   * the producer is untrusted: its proposal must satisfy the declared
//     constraints, and a rejected proposal becomes durable negative knowledge;
//   * a candidate is frozen by source bytes. Any source-byte change is a new
//     revision with a new identity, so no downstream evidence is ever reused
//     across a changed candidate.
//
// The qualification and challenge universes are **not** inputs here: they belong
// to Phase 7 and are deliberately absent from the producer's request.

use std::path::Path;

use phost::porting::portspec::PortSpec;
use phost::porting::target::{cases_for, PortTarget};

use crate::compile::{compile_source, sha256_hex, CandidateIdentity};
use crate::config::HarnessConfig;
use crate::explore::{design_corpus_matches, design_first_divergence};
use crate::minimize;
use crate::producer::{
    check_constraints, CandidateProducer, CandidateProducerError, CandidateRequest,
    KnownCounterexample,
};

/// The frozen premises of a campaign. Changing any of them is a new campaign,
/// not a mid-flight adjustment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampaignManifest {
    pub target_id: String,
    pub port_spec_id: String,
    pub design_generator: String,
    pub qualification_policy: String,
    pub discovery_budget: u32,
    pub challenge_required: bool,
    pub seal_profile: String,
}

impl CampaignManifest {
    /// The content identity of the manifest (its canonical encoding).
    pub fn id(&self) -> String {
        let canon = format!(
            "PHOR/CAMPAIGN/v1|target={};spec={};design={};qual={};budget={};challenge={};seal={}",
            self.target_id,
            self.port_spec_id,
            self.design_generator,
            self.qualification_policy,
            self.discovery_budget,
            self.challenge_required,
            self.seal_profile
        );
        sha256_hex(canon.as_bytes())
    }
}

/// The monotonic campaign states this core drives. Qualification, challenge and
/// sealing are Phase 7; the states exist so a later phase continues the same
/// machine rather than inventing a parallel one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignState {
    Specified,
    CandidateProposed,
    DesignChecked,
    DiscoveryChecked,
    CandidateFrozen,
    CandidateRejected,
    ProducerExhausted,
}

impl CampaignState {
    pub fn as_str(self) -> &'static str {
        match self {
            CampaignState::Specified => "specified",
            CampaignState::CandidateProposed => "candidate-proposed",
            CampaignState::DesignChecked => "design-checked",
            CampaignState::DiscoveryChecked => "discovery-checked",
            CampaignState::CandidateFrozen => "candidate-frozen",
            CampaignState::CandidateRejected => "candidate-rejected",
            CampaignState::ProducerExhausted => "producer-exhausted",
        }
    }
}

/// The CEGIS residual.
#[derive(Clone, Debug)]
pub struct CegisReport {
    pub manifest_id: String,
    pub states: Vec<CampaignState>,
    pub revisions: u32,
    pub frozen: Option<CandidateIdentity>,
    pub rejected: Vec<CandidateIdentity>,
    pub counterexamples: Vec<KnownCounterexample>,
    pub revision_log: Vec<String>,
}

/// A discovery hook: given a compiled candidate, return any counterexamples a
/// bounded exploration found. Phase 4's FRF-Fuzz campaign is the production hook;
/// tests pass a no-op.
pub type DiscoveryHook<'a> = &'a mut dyn FnMut(&HarnessConfig) -> Vec<KnownCounterexample>;

fn to_known(case_id: &str, data: &[u8], o: &crate::harness::ProbeOutcome) -> KnownCounterexample {
    KnownCounterexample {
        residual: format!("{} ({case_id})", o.residual),
        input_hex: hex::encode(data),
        oracle_hex: o.oracle_hex.clone(),
        candidate_hex: o.candidate_hex.clone(),
    }
}

/// Run the bounded CEGIS loop for one campaign.
///
/// `work_root` holds per-revision source/object trees; nothing outside it (in
/// particular no qualification material) is an input to the producer.
#[allow(clippy::too_many_arguments)]
pub fn run_cegis(
    producer: &mut dyn CandidateProducer,
    spec: &PortSpec,
    target: &PortTarget,
    base_config: &HarnessConfig,
    work_root: &Path,
    manifest: &CampaignManifest,
    mut discovery: Option<DiscoveryHook<'_>>,
) -> CegisReport {
    let design_case_ids: Vec<String> = cases_for(target)
        .iter()
        .map(|c| c.case_id.clone())
        .collect();
    let design_case_count = design_case_ids.len() as u64;

    let mut states = vec![CampaignState::Specified];
    let mut rejected = Vec::new();
    let mut counterexamples = Vec::new();
    let mut revision_log = Vec::new();
    let mut frozen = None;
    let mut revisions = 0u32;

    let mut request = CandidateRequest {
        target_id: manifest.target_id.clone(),
        contract_summary: format!(
            "{} ({}) locale={}",
            spec.symbol, spec.target_id, spec.locale_contract
        ),
        max_source_bytes: spec.candidate.max_source_bytes,
        revision: 0,
        design_case_count,
        known_counterexamples: Vec::new(),
        negative_knowledge: Vec::new(),
    };

    for revision in 0..=manifest.discovery_budget {
        request.revision = revision;
        let proposal = match producer.propose(&request) {
            Ok(p) => p,
            Err(CandidateProducerError::Refused(why)) => {
                revision_log.push(format!("revision {revision}: producer refused: {why}"));
                states.push(CampaignState::ProducerExhausted);
                break;
            }
            Err(CandidateProducerError::Unavailable(why)) => {
                revision_log.push(format!("revision {revision}: producer unavailable: {why}"));
                states.push(CampaignState::ProducerExhausted);
                break;
            }
        };
        states.push(CampaignState::CandidateProposed);

        // Constraints first: an untrusted proposal is not even compiled if it
        // violates the declared search space.
        let cr = check_constraints(spec, &proposal.source);
        if !cr.ok {
            revision_log.push(format!(
                "revision {revision}: constraint violation: {}",
                cr.violations.join("; ")
            ));
            request.negative_knowledge.push(format!(
                "revision {revision} left the search space: {}",
                cr.violations.join("; ")
            ));
            revisions = revision + 1;
            continue;
        }

        let dir = work_root.join(format!("rev{revision}"));
        let mut identity = match compile_source(target.symbol, &proposal.source, &dir) {
            Ok(i) => i,
            Err(e) => {
                revision_log.push(format!("revision {revision}: compile failed: {e}"));
                request
                    .negative_knowledge
                    .push(format!("revision {revision} did not compile: {e}"));
                revisions = revision + 1;
                continue;
            }
        };
        identity.revision = revision;

        let config = HarnessConfig {
            target_id: base_config.target_id.clone(),
            candidate_path: identity.object_path.clone(),
            candidate_hash: identity.object_hash.clone(),
        };

        // DESIGN court.
        let (ran, diverged) = design_corpus_matches(&config, target);
        states.push(CampaignState::DesignChecked);
        if diverged > 0 {
            if let Some((case_id, data, outcome)) = design_first_divergence(&config, target) {
                let m = minimize::minimize(&config, &data, &outcome);
                let known = to_known(&case_id, &m.minimal, &outcome);
                request.known_counterexamples.push(known.clone());
                counterexamples.push(known);
            }
            revision_log.push(format!(
                "revision {revision}: rejected by the design court ({diverged} of {ran} cases)"
            ));
            request.negative_knowledge.push(format!(
                "revision {revision} (source {}) failed {diverged} of {ran} design cases",
                identity.source_hash
            ));
            rejected.push(identity);
            states.push(CampaignState::CandidateRejected);
            revisions = revision + 1;
            continue;
        }

        // DISCOVERY court (bounded). The production hook is an FRF-Fuzz campaign;
        // without a hook, discovery is closed at the design corpus.
        let found = match discovery.as_mut() {
            Some(hook) => hook(&config),
            None => Vec::new(),
        };
        states.push(CampaignState::DiscoveryChecked);
        if !found.is_empty() {
            for k in &found {
                request.known_counterexamples.push(k.clone());
            }
            counterexamples.extend(found);
            revision_log.push(format!(
                "revision {revision}: rejected by discovery ({} counterexample(s))",
                counterexamples.len()
            ));
            request.negative_knowledge.push(format!(
                "revision {revision} (source {}) diverged under exploration",
                identity.source_hash
            ));
            rejected.push(identity);
            states.push(CampaignState::CandidateRejected);
            revisions = revision + 1;
            continue;
        }

        // Survived both courts: freeze these exact source bytes.
        revision_log.push(format!(
            "revision {revision}: frozen (source {})",
            identity.source_hash
        ));
        states.push(CampaignState::CandidateFrozen);
        frozen = Some(identity);
        revisions = revision + 1;
        break;
    }

    CegisReport {
        manifest_id: manifest.id(),
        states,
        revisions,
        frozen,
        rejected,
        counterexamples,
        revision_log,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::producer::ScriptedProducer;
    use phost::porting::portspec;

    fn spec() -> &'static PortSpec {
        portspec::by_target_id("posix:strspn:c-locale:u64:v1").expect("spec")
    }

    fn manifest() -> CampaignManifest {
        CampaignManifest {
            target_id: String::from("posix:strspn:c-locale:u64:v1"),
            port_spec_id: "spec".to_string(),
            design_generator: "strspn_corpus".to_string(),
            qualification_policy: "host-observed".to_string(),
            discovery_budget: 4,
            challenge_required: true,
            seal_profile: "autonomous-v1".to_string(),
        }
    }

    /// The loop rejects a candidate the design court falsifies and freezes the
    /// revision that survives — no candidate is frozen without passing.
    #[test]
    fn test_cegis_revises_on_design_failure_and_freezes_the_survivor() {
        // The lib test binary lives in `target/debug/deps`, so `phorc` is not next
        // to it; put `target/debug` on PATH for the compiler lookup.
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));

        let correct = std::fs::read_to_string("../examples/jit_port_strspn.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn.phor"))
            .expect("correct source");
        let wrong = "package \"phorensic:jit_port_strspn_wrong:v1.0\";\n\
                     fn phor_strspn_len(ws: u64, wa: u64, n: u64) -> u64 effect [compute] { return 0; }\n\
                     \n";
        let mut producer = ScriptedProducer::new(vec![wrong.to_string(), correct]);

        let target = phost::porting::target::resolve_target("strspn").expect("target");
        let base = HarnessConfig {
            target_id: target.id.to_string(),
            candidate_path: String::new(),
            candidate_hash: String::new(),
        };
        let work = std::env::temp_dir().join(format!("phorport-cegis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);

        let report = run_cegis(
            &mut producer,
            spec(),
            &target,
            &base,
            &work,
            &manifest(),
            None,
        );

        assert_eq!(report.revisions, 2, "{:?}", report.revision_log);
        assert_eq!(report.rejected.len(), 1, "{:?}", report.revision_log);
        let frozen = report.frozen.expect("a survivor is frozen");
        assert_eq!(frozen.revision, 1);
        assert!(!report.counterexamples.is_empty());
        assert!(report.states.contains(&CampaignState::CandidateRejected));
        assert_eq!(report.states.last(), Some(&CampaignState::CandidateFrozen));

        let _ = std::fs::remove_dir_all(&work);
    }
}
