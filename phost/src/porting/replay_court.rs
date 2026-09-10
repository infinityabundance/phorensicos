// porting/replay_court.rs — Replay + comparison court
//
// Replays every sealed oracle case against the native candidate and compares
// the exact output bytes. The verdict derives from case comparisons only — never
// from receipt counts — and the court fails closed: an empty case set is
// `Inconclusive`, any mismatch (including an unsupported candidate) is
// `Inconsistent`, and both deny promotion.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate;
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::{json_escape, sha256_hex};

/// Court verdict over the replayed case set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtVerdict {
    /// Every case matched exactly.
    Consistent,
    /// At least one case diverged (or the candidate could not be run).
    Inconsistent,
    /// No cases to judge — the court cannot accept.
    Inconclusive,
}

impl CourtVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            CourtVerdict::Consistent => "consistent",
            CourtVerdict::Inconsistent => "inconsistent",
            CourtVerdict::Inconclusive => "inconclusive",
        }
    }
}

/// One exact-output divergence between oracle and candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    pub case_id: String,
    pub expected_output_hex: String,
    /// Candidate output hex, or `!unsupported:<target>` when the candidate
    /// could not be run for this case.
    pub actual_output_hex: String,
}

impl Mismatch {
    pub fn to_json(&self) -> String {
        format!(
            "    {{\n      \"case_id\": \"{}\",\n      \"expected_output_hex\": \"{}\",\n      \"actual_output_hex\": \"{}\"\n    }}",
            json_escape(&self.case_id),
            self.expected_output_hex,
            json_escape(&self.actual_output_hex)
        )
    }
}

/// The comparison residual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayVerdict {
    pub target: String,
    pub cases_run: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub oracle_hash: String,
    /// SHA-256 over the candidate's *behavior* across the case domain — not a
    /// hash of the candidate's implementation bytes.
    pub candidate_behavior_hash: String,
    pub verdict: CourtVerdict,
}

impl ReplayVerdict {
    /// The court only permits promotion on an exact, non-empty match.
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.oracle_hash.is_empty()
            && !self.candidate_behavior_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};cases_run={};cases_passed={};cases_failed={};oracle_hash={};candidate_behavior_hash={};verdict={}",
            self.target,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[Mismatch]) -> String {
        let body: Vec<String> = mismatches.iter().map(|m| m.to_json()).collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.replay_verdict.v1\",\n  \"target\": \"{}\",\n  \"cases_run\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"oracle_hash\": \"{}\",\n  \"candidate_behavior_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.verdict.as_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

/// Replay the native candidate against the sealed oracle traces.
///
/// Returns the verdict plus every mismatch record (empty on a consistent run).
/// An unsupported candidate counts as a failure — it can never pass by accident.
pub fn run_replay_court(traces: &[OracleTrace]) -> (ReplayVerdict, Vec<Mismatch>) {
    let target = traces.first().map(|t| t.target.clone()).unwrap_or_default();

    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();

    for t in traces {
        let input = t.input_bytes();
        match candidate::run_candidate(&t.target, &input) {
            Ok(out) => {
                let actual_hex = hex::encode(&out);
                if actual_hex == t.output_hex && t.status == "ok" {
                    passed += 1;
                } else {
                    failed += 1;
                    mismatches.push(Mismatch {
                        case_id: t.case_id.clone(),
                        expected_output_hex: t.output_hex.clone(),
                        actual_output_hex: actual_hex,
                    });
                }
            }
            Err(e) => {
                failed += 1;
                mismatches.push(Mismatch {
                    case_id: t.case_id.clone(),
                    expected_output_hex: t.output_hex.clone(),
                    actual_output_hex: format!("!{}", e.as_str()),
                });
            }
        }
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0 {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        ReplayVerdict {
            target,
            cases_run,
            cases_passed: passed,
            cases_failed: failed,
            oracle_hash: combined_oracle_hash(traces),
            candidate_behavior_hash: candidate::candidate_behavior_hash(traces),
            verdict,
        },
        mismatches,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::candidate::phor_toupper;
    use crate::porting::target::{self, byte_domain_cases};

    /// Build a trace set where the oracle agrees with the native candidate.
    fn agreeing_cases() -> Vec<OracleTrace> {
        byte_domain_cases()
            .iter()
            .map(|c| {
                OracleTrace::new(
                    &target::LIBC_TOUPPER,
                    &c.case_id,
                    &c.input,
                    &[phor_toupper(c.input[0])],
                    "ok",
                    &["compute"],
                )
            })
            .collect()
    }

    #[test]
    fn test_replay_court_pass_path() {
        let traces = agreeing_cases();
        let (verdict, mismatches) = run_replay_court(&traces);
        assert_eq!(verdict.verdict, CourtVerdict::Consistent);
        assert_eq!(verdict.cases_run, 256);
        assert_eq!(verdict.cases_passed, 256);
        assert_eq!(verdict.cases_failed, 0);
        assert!(mismatches.is_empty());
        assert!(verdict.is_sealed_eligible());
    }

    #[test]
    fn test_replay_court_mismatch_path() {
        let mut traces = agreeing_cases();
        // Corrupt exactly one oracle output ('a' should map to 'A').
        traces[0x61].output_hex = "42".to_string();
        let (verdict, mismatches) = run_replay_court(&traces);
        assert_eq!(verdict.verdict, CourtVerdict::Inconsistent);
        assert_eq!(verdict.cases_run, 256);
        assert_eq!(verdict.cases_passed, 255);
        assert_eq!(verdict.cases_failed, 1);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].case_id, "0x61");
        assert_eq!(mismatches[0].expected_output_hex, "42");
        assert_eq!(mismatches[0].actual_output_hex, "41");
        assert!(!verdict.is_sealed_eligible());
    }

    #[test]
    fn test_unsupported_candidate_is_never_identity() {
        // Traces for an unsupported target id: the court must fail, not treat
        // the candidate as identity.
        let unknown = crate::porting::PortTarget {
            id: "libc:identity:c-locale:u8:v1",
            dialect: "libc",
            symbol: "identity",
            version: "v1",
            locale_contract: "C",
            input_schema: "u8",
            output_schema: "u8",
            candidate_source: "none",
        };
        let traces: Vec<OracleTrace> = byte_domain_cases()
            .iter()
            .map(|c| OracleTrace::new(&unknown, &c.case_id, &c.input, &c.input, "ok", &["compute"]))
            .collect();
        let (verdict, mismatches) = run_replay_court(&traces);
        assert_eq!(verdict.verdict, CourtVerdict::Inconsistent);
        assert_eq!(verdict.cases_failed, 256);
        assert!(mismatches[0].actual_output_hex.starts_with('!'));
        assert!(!verdict.is_sealed_eligible());
    }

    #[test]
    fn test_empty_case_set_is_inconclusive() {
        let (verdict, _) = run_replay_court(&[]);
        assert_eq!(verdict.verdict, CourtVerdict::Inconclusive);
        assert!(!verdict.is_sealed_eligible());
    }
}
