// porting/oracle_trace.rs — Sealed oracle traces
//
// An oracle trace is one observed case: input bytes in, output bytes out, plus
// the observed locale contract, status/effects, and a self-describing SHA-256
// over a stable canonical encoding. Traces are the sealed evidence a native
// candidate is replayed against.
//
// Determinism rules for court evidence:
//   * JSON field order is fixed by the writer,
//   * cases appear in domain order,
//   * all hex is lowercase,
//   * no timestamps or clocks are covered by any hash.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::sha256_hex;
use crate::porting::target::PortTarget;

/// One sealed observation of a foreign API surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleTrace {
    pub target: String,
    pub locale_contract: String,
    pub case_id: String,
    pub input_hex: String,
    pub output_hex: String,
    pub status: String,
    pub effects: Vec<String>,
    pub observed_hash: String,
}

impl OracleTrace {
    /// Build a trace for `target` and seal its `observed_hash`.
    pub fn new(
        target: &PortTarget,
        case_id: &str,
        input: &[u8],
        output: &[u8],
        status: &str,
        effects: &[&str],
    ) -> Self {
        let mut trace = Self {
            target: target.id.to_string(),
            locale_contract: target.locale_contract.to_string(),
            case_id: case_id.to_string(),
            input_hex: hex::encode(input),
            output_hex: hex::encode(output),
            status: status.to_string(),
            effects: effects.iter().map(|e| (*e).to_string()).collect(),
            observed_hash: String::new(),
        };
        trace.observed_hash = trace.compute_hash();
        trace
    }

    /// Canonical, hash-covered encoding (excludes `observed_hash` itself).
    pub fn canonical(&self) -> String {
        format!(
            "target={};locale={};case_id={};input={};output={};status={};effects={}",
            self.target,
            self.locale_contract,
            self.case_id,
            self.input_hex,
            self.output_hex,
            self.status,
            self.effects.join(",")
        )
    }

    pub fn compute_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    /// True if the sealed hash still matches the trace contents.
    pub fn is_intact(&self) -> bool {
        self.compute_hash() == self.observed_hash
    }

    pub fn input_bytes(&self) -> Vec<u8> {
        hex::decode(&self.input_hex).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        let effects: Vec<String> = self
            .effects
            .iter()
            .map(|e| format!("\"{}\"", crate::porting::json_escape(e)))
            .collect();
        format!(
            "    {{\n      \"target\": \"{}\",\n      \"locale_contract\": \"{}\",\n      \"case_id\": \"{}\",\n      \"input_hex\": \"{}\",\n      \"output_hex\": \"{}\",\n      \"status\": \"{}\",\n      \"effects\": [{}],\n      \"observed_hash\": \"{}\"\n    }}",
            crate::porting::json_escape(&self.target),
            crate::porting::json_escape(&self.locale_contract),
            crate::porting::json_escape(&self.case_id),
            self.input_hex,
            self.output_hex,
            crate::porting::json_escape(&self.status),
            effects.join(", "),
            self.observed_hash
        )
    }
}

/// Serialize the full trace set with fixed field order and one trace per line.
pub fn traces_to_json(traces: &[OracleTrace]) -> String {
    let body: Vec<String> = traces.iter().map(|t| t.to_json()).collect();
    format!(
        "{{\n  \"schema\": \"phorensic.porting.oracle_traces.v1\",\n  \"case_count\": {},\n  \"traces\": [\n{}\n  ]\n}}\n",
        traces.len(),
        body.join(",\n")
    )
}

/// Combined oracle behavior hash: SHA-256 over `case_id:canonical_trace` per
/// case, in domain order. Binding to the full canonical content means any
/// mutation of a covered field (including a stale seal) changes this hash. This
/// is the value the court compares a candidate against.
pub fn combined_oracle_hash(traces: &[OracleTrace]) -> String {
    let mut buf = String::new();
    for t in traces {
        buf.push_str(&t.case_id);
        buf.push(':');
        buf.push_str(&t.canonical());
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target;

    fn trace(case_id: &str, input: u8, output: u8) -> OracleTrace {
        OracleTrace::new(
            &target::LIBC_TOUPPER,
            case_id,
            &[input],
            &[output],
            "ok",
            &["compute"],
        )
    }

    #[test]
    fn test_trace_hash_is_intact() {
        let t = trace("0x61", 0x61, 0x41);
        assert!(t.is_intact());
        assert_eq!(t.input_hex, "61");
        assert_eq!(t.output_hex, "41");
        assert_eq!(t.target, "libc:toupper:c-locale:u8:v1");
        assert_eq!(t.locale_contract, "C");
    }

    #[test]
    fn test_trace_serialization_is_stable() {
        let t = trace("0x61", 0x61, 0x41);
        assert_eq!(t.to_json(), t.to_json());

        let set = alloc::vec![trace("0x00", 0x00, 0x00), trace("0x61", 0x61, 0x41)];
        assert_eq!(traces_to_json(&set), traces_to_json(&set));
        assert_eq!(combined_oracle_hash(&set), combined_oracle_hash(&set));
    }

    #[test]
    fn test_evidence_hash_changes_when_trace_mutated() {
        let cases = alloc::vec![trace("0x00", 0x00, 0x00), trace("0x61", 0x61, 0x41)];
        let before = combined_oracle_hash(&cases);

        // Mutate one observed output: the combined hash must change, and the
        // mutated trace must no longer be self-consistent (two independent
        // tamper detections).
        let mut mutated = cases.clone();
        mutated[1].output_hex = "42".to_string();
        assert_ne!(before, combined_oracle_hash(&mutated));
        assert!(!mutated[1].is_intact());

        // The locale contract is hash-covered too.
        let mut relocaled = cases.clone();
        relocaled[1].locale_contract = "en_US".to_string();
        assert_ne!(before, combined_oracle_hash(&relocaled));
        assert!(!relocaled[1].is_intact());

        // Re-sealing the mutated trace still yields a different behavior hash
        // (it is a different observed behavior, not a correction).
        let mut resealed = mutated.clone();
        resealed[1].observed_hash = resealed[1].compute_hash();
        assert!(resealed[1].is_intact());
        assert_ne!(before, combined_oracle_hash(&resealed));
    }
}
