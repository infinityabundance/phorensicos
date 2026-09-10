// porting/behavior_signature.rs — Sealed behavior signature
//
// Summarizes a sealed oracle trace set: the qualified target id, the locale
// contract, how many cases were observed, the combined oracle hash, and a
// human-readable description of the input domain. This is the value a replay
// verdict's `oracle_hash` must match.

use alloc::format;
use alloc::string::{String, ToString};

use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::target::PortTarget;
use crate::porting::{json_escape, sha256_hex};

/// Sealed summary of observed foreign behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorSignature {
    pub target: String,
    pub locale_contract: String,
    pub case_count: u64,
    pub combined_oracle_hash: String,
    pub input_domain_summary: String,
}

impl BehaviorSignature {
    pub fn from_traces(target: &PortTarget, traces: &[OracleTrace]) -> Self {
        Self {
            target: target.id.to_string(),
            locale_contract: target.locale_contract.to_string(),
            case_count: traces.len() as u64,
            combined_oracle_hash: combined_oracle_hash(traces),
            input_domain_summary: format!("{} cases; {}", traces.len(), target.domain_summary),
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};locale={};case_count={};combined_oracle_hash={};input_domain_summary={}",
            self.target,
            self.locale_contract,
            self.case_count,
            self.combined_oracle_hash,
            self.input_domain_summary
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.behavior_signature.v1\",\n  \"target\": \"{}\",\n  \"locale_contract\": \"{}\",\n  \"case_count\": {},\n  \"combined_oracle_hash\": \"{}\",\n  \"input_domain_summary\": \"{}\",\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.locale_contract),
            self.case_count,
            self.combined_oracle_hash,
            json_escape(&self.input_domain_summary),
            self.residual_hash()
        )
    }
}

/// Convenience for callers that only have traces.
pub fn from_traces(target: &PortTarget, traces: &[OracleTrace]) -> BehaviorSignature {
    BehaviorSignature::from_traces(target, traces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target;
    use alloc::vec::Vec;

    fn two_traces() -> Vec<OracleTrace> {
        alloc::vec![
            OracleTrace::single(
                &target::LIBC_TOUPPER,
                "0x00",
                &[0x00],
                &[0x00],
                "ok",
                &["compute"]
            ),
            OracleTrace::single(
                &target::LIBC_TOUPPER,
                "0x61",
                &[0x61],
                &[0x41],
                "ok",
                &["compute"]
            ),
        ]
    }

    #[test]
    fn test_signature_counts_and_hash() {
        let traces = two_traces();
        let sig = BehaviorSignature::from_traces(&target::LIBC_TOUPPER, &traces);
        assert_eq!(sig.case_count, 2);
        assert_eq!(sig.target, "libc:toupper:c-locale:u8:v1");
        assert_eq!(sig.locale_contract, "C");
        assert_eq!(sig.combined_oracle_hash, combined_oracle_hash(&traces));
        assert_eq!(sig.residual_hash(), sig.residual_hash());
    }
}
