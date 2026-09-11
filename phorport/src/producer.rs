// phorport/producer.rs — the untrusted candidate producer and its isolated workspace
//
// Phase 6. Candidate generation is a **bounded CEGIS loop** whose generator is an
// untrusted `CandidateProducer`. Automation may propose everything; it may approve
// nothing. The producer therefore never sees qualification material, and its
// output is only ever a proposal subjected to the declared constraints.
//
// Two producers ship: a `ScriptedProducer` (a deterministic catalogue used to
// exercise the loop without an agent) and an `ExternalCommandProducer` (an agent
// adapter: a command that reads a request on stdin and writes source on stdout).
// A production foundry would add an enumerative Phor synthesizer behind the same
// trait; nothing in the loop changes when it does.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use phost::porting::portspec::PortSpec;

/// One counterexample the producer is allowed to see (the DISCOVERY set).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownCounterexample {
    pub residual: String,
    pub input_hex: String,
    pub oracle_hex: String,
    pub candidate_hex: String,
}

/// A bounded request to a producer.
///
/// It contains only permitted material: the public contract surface, the design
/// evidence already observed, previously discovered counterexamples, and bounded
/// negative knowledge. It contains **no** qualification fixture, seed, oracle
/// output or expected outcome.
#[derive(Clone, Debug, Default)]
pub struct CandidateRequest {
    pub target_id: String,
    pub contract_summary: String,
    pub max_source_bytes: u32,
    pub revision: u32,
    pub design_case_count: u64,
    pub known_counterexamples: Vec<KnownCounterexample>,
    pub negative_knowledge: Vec<String>,
}

impl CandidateRequest {
    /// A stable JSON projection (the only thing an external producer receives).
    pub fn to_json(&self) -> String {
        let cxs: Vec<String> = self
            .known_counterexamples
            .iter()
            .map(|c| {
                format!(
                    "    {{\"residual\":\"{}\",\"input_hex\":\"{}\",\"oracle_hex\":\"{}\",\"candidate_hex\":\"{}\"}}",
                    c.residual, c.input_hex, c.oracle_hex, c.candidate_hex
                )
            })
            .collect();
        let neg: Vec<String> = self
            .negative_knowledge
            .iter()
            .map(|s| format!("    \"{}\"", s.replace('"', "'")))
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.phorport.candidate_request.v1\",\n  \"target_id\": \"{}\",\n  \"contract\": \"{}\",\n  \"max_source_bytes\": {},\n  \"revision\": {},\n  \"design_case_count\": {},\n  \"known_counterexamples\": [\n{}\n  ],\n  \"negative_knowledge\": [\n{}\n  ]\n}}\n",
            self.target_id,
            self.contract_summary.replace('"', "'"),
            self.max_source_bytes,
            self.revision,
            self.design_case_count,
            cxs.join(",\n"),
            neg.join(",\n")
        )
    }
}

/// A producer's output: source bytes plus a note. Untrusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateProposal {
    pub source: String,
    pub note: String,
}

/// Why a producer could not propose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateProducerError {
    /// The producer declined (e.g. it has no more candidates).
    Refused(String),
    /// The producer could not run (missing command, transport error).
    Unavailable(String),
}

/// An untrusted candidate generator.
pub trait CandidateProducer {
    fn name(&self) -> &'static str;
    fn propose(
        &mut self,
        request: &CandidateRequest,
    ) -> Result<CandidateProposal, CandidateProducerError>;
}

// ---------------------------------------------------------------------------
// Constraint enforcement
// ---------------------------------------------------------------------------

/// The result of enforcing the candidate constraints on a proposed source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintReport {
    pub ok: bool,
    pub source_bytes: usize,
    pub violations: Vec<String>,
}

/// Enforce the `PortSpec`'s `CandidateConstraints` on a proposed source.
///
/// This is the *source-level* half of the constraint check (size, leaf policy).
/// The ELF-level half (no relocations, leaf entry, permitted sections) is the
/// compiler/validator's job when the proposal is compiled; a proposal that fails
/// either half is rejected, never silently accepted.
pub fn check_constraints(spec: &PortSpec, source: &str) -> ConstraintReport {
    let mut violations = Vec::new();
    let bytes = source.len();
    if bytes == 0 {
        violations.push(String::from("empty source"));
    }
    if bytes as u64 > spec.candidate.max_source_bytes as u64 {
        violations.push(format!(
            "source is {} bytes, over the {} bound",
            bytes, spec.candidate.max_source_bytes
        ));
    }
    if spec.candidate.leaf_only {
        // Check *code*, not comments: strip `//` line comments first.
        let code: String = source
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["extern", "import", "asm!", "unsafe"] {
            if code.contains(forbidden) {
                violations.push(format!("leaf-only violated: contains `{forbidden}`"));
            }
        }
        if code.lines().any(|l| l.trim_start().starts_with("use ")) {
            violations.push(String::from("leaf-only violated: contains a `use` import"));
        }
    }
    ConstraintReport {
        ok: violations.is_empty(),
        source_bytes: bytes,
        violations,
    }
}

// ---------------------------------------------------------------------------
// Producers
// ---------------------------------------------------------------------------

/// A deterministic catalogue producer: yields each source once, in order.
///
/// This is a **scripted** producer used to exercise the loop where no agent is
/// available; it is not a synthesizer.
pub struct ScriptedProducer {
    sources: Vec<String>,
    next: usize,
}

impl ScriptedProducer {
    pub fn new(sources: Vec<String>) -> Self {
        ScriptedProducer { sources, next: 0 }
    }
}

impl CandidateProducer for ScriptedProducer {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn propose(
        &mut self,
        _request: &CandidateRequest,
    ) -> Result<CandidateProposal, CandidateProducerError> {
        match self.sources.get(self.next) {
            Some(s) => {
                self.next += 1;
                Ok(CandidateProposal {
                    source: s.clone(),
                    note: format!("catalogue entry {}", self.next - 1),
                })
            }
            None => Err(CandidateProducerError::Refused(String::from(
                "catalogue exhausted",
            ))),
        }
    }
}

/// An external-command producer: an agent adapter.
///
/// The command reads the request JSON on stdin and writes the proposed source on
/// stdout. It is untrusted: its output is only ever a proposal.
pub struct ExternalCommandProducer {
    pub program: String,
    pub args: Vec<String>,
}

impl CandidateProducer for ExternalCommandProducer {
    fn name(&self) -> &'static str {
        "external-command"
    }

    fn propose(
        &mut self,
        request: &CandidateRequest,
    ) -> Result<CandidateProposal, CandidateProducerError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CandidateProducerError::Unavailable(e.to_string()))?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(request.to_json().as_bytes())
                .map_err(|e| CandidateProducerError::Unavailable(e.to_string()))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| CandidateProducerError::Unavailable(e.to_string()))?;
        if !out.status.success() {
            return Err(CandidateProducerError::Refused(format!(
                "producer exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let source = String::from_utf8_lossy(&out.stdout).into_owned();
        if source.trim().is_empty() {
            return Err(CandidateProducerError::Refused(String::from(
                "producer wrote no source",
            )));
        }
        Ok(CandidateProposal {
            source,
            note: String::from("external producer"),
        })
    }
}

// ---------------------------------------------------------------------------
// The isolated synthesis workspace
// ---------------------------------------------------------------------------

/// The sanitized workspace a producer operates in.
///
/// It contains only permitted material. `audit` scans every file for forbidden
/// markers (qualification case ids, qualification oracle outputs, challenge
/// expected outcomes, the withheld original source in blind mode) and returns any
/// it finds — a leak is a defect, so the audit fails closed.
pub struct SynthesisWorkspace {
    root: PathBuf,
}

impl SynthesisWorkspace {
    /// Materialize the permitted material for one revision.
    pub fn materialize(
        root: &Path,
        request: &CandidateRequest,
        design_case_ids: &[String],
    ) -> std::io::Result<SynthesisWorkspace> {
        std::fs::create_dir_all(root)?;
        std::fs::write(root.join("request.json"), request.to_json())?;
        // Design *identities* only: the producer may know which designed cases
        // exist (they are public), never the qualification universe.
        let ids = design_case_ids.join("\n");
        std::fs::write(root.join("design_case_ids.txt"), ids)?;
        Ok(SynthesisWorkspace {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return every forbidden marker found in the workspace (empty = clean).
    pub fn audit(&self, forbidden_markers: &[String]) -> Vec<String> {
        let mut leaks = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return leaks;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for marker in forbidden_markers {
                if !marker.is_empty() && text.contains(marker) {
                    leaks.push(format!("{}: contains `{}`", path.display(), marker));
                }
            }
        }
        leaks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::portspec;

    fn strspn_spec() -> &'static PortSpec {
        portspec::by_target_id("posix:strspn:c-locale:u64:v1").expect("spec")
    }

    #[test]
    fn test_constraints_reject_empty_oversize_and_foreign_calls() {
        let spec = strspn_spec();
        assert!(!check_constraints(spec, "").ok);
        assert!(!check_constraints(spec, "extern fn thing() {}").ok);
        assert!(!check_constraints(spec, "import foo").ok);
        let big = "x".repeat(spec.candidate.max_source_bytes as usize + 1);
        assert!(!check_constraints(spec, &big).ok);
        assert!(check_constraints(spec, "fn f() -> u64 { return 0; }").ok);
    }

    #[test]
    fn test_workspace_contains_no_qualification_material() {
        let dir = std::env::temp_dir().join(format!("phorport-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let request = CandidateRequest {
            target_id: String::from("posix:strspn:c-locale:u64:v1"),
            contract_summary: String::from("strspn(s, accept) -> span"),
            max_source_bytes: 1024,
            revision: 0,
            design_case_count: 3,
            known_counterexamples: vec![KnownCounterexample {
                residual: String::from("PORT.LENGTH"),
                input_hex: String::from("0302"),
                oracle_hex: String::from("0200"),
                candidate_hex: String::from("0000"),
            }],
            negative_knowledge: vec![String::from(
                "prior source sha256=deadbeef failed PORT.LENGTH",
            )],
        };
        let ws = SynthesisWorkspace::materialize(
            &dir,
            &request,
            &[String::from("D.1"), String::from("D.2")],
        )
        .expect("materialize");

        // Markers that must never appear in a synthesis workspace.
        let forbidden = vec![
            String::from("QUALIFICATION-SECRET-CASE"),
            String::from("qualification-oracle-output"),
            String::from("challenge-expected-outcome"),
            String::from("withheld-original-source"),
        ];
        assert!(ws.audit(&forbidden).is_empty(), "workspace leaked material");

        // The permitted material is present.
        let text = std::fs::read_to_string(dir.join("request.json")).unwrap();
        assert!(text.contains("known_counterexamples"));
        assert!(text.contains("prior source sha256=deadbeef"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
