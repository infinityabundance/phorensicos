// porting/candidate.rs — Clean-room native candidate
//
// The native Phorensic implementation of the target, written from the public
// specification and the observed traces — not from any foreign source. For
// `toupper` this is a byte-in/byte-out function; the mirror `.phor` expression
// of the same logic lives at examples/jit_port_toupper.phor.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::PortTarget;
use crate::porting::{json_escape, sha256_hex};

/// Native clean-room `toupper`: map ASCII `a`..=`z` to `A`..=`Z`, leave every
/// other byte unchanged. This is the C-locale contract, matching the observed
/// foreign behavior for the exhaustive byte domain.
#[inline]
pub fn phor_toupper(b: u8) -> u8 {
    if b >= b'a' && b <= b'z' {
        b - (b'a' - b'A')
    } else {
        b
    }
}

/// Run the native candidate for a target symbol over raw input bytes.
pub fn run_candidate(symbol: &str, input: &[u8]) -> Vec<u8> {
    match symbol {
        "toupper" => input.iter().map(|&b| phor_toupper(b)).collect(),
        // Unknown targets: identity is not a claim of correctness; the court
        // will report a mismatch against any real oracle.
        _ => input.to_vec(),
    }
}

/// SHA-256 over `case_id:candidate_output_hex` per case, in domain order. This
/// is the candidate-side analogue of the combined oracle hash.
pub fn candidate_hash(traces: &[OracleTrace]) -> String {
    let mut buf = String::new();
    for t in traces {
        let input = t.input_bytes();
        let out = run_candidate(&t.target, &input);
        buf.push_str(&t.case_id);
        buf.push(':');
        buf.push_str(&hex::encode(&out));
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// The promoted native candidate, described as a residual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateSignature {
    pub target: String,
    pub symbol: String,
    pub case_count: u64,
    pub candidate_hash: String,
    pub source: String,
}

impl CandidateSignature {
    pub fn from_traces(target: &PortTarget, traces: &[OracleTrace]) -> Self {
        Self {
            target: target.symbol.to_string(),
            symbol: "phor_toupper".to_string(),
            case_count: traces.len() as u64,
            candidate_hash: candidate_hash(traces),
            source:
                "clean-room native (phost::porting::candidate + examples/jit_port_toupper.phor)"
                    .to_string(),
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};symbol={};case_count={};candidate_hash={};source={}",
            self.target, self.symbol, self.case_count, self.candidate_hash, self.source
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.candidate_signature.v1\",\n  \"target\": \"{}\",\n  \"symbol\": \"{}\",\n  \"case_count\": {},\n  \"candidate_hash\": \"{}\",\n  \"source\": \"{}\",\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.symbol),
            self.case_count,
            self.candidate_hash,
            json_escape(&self.source),
            self.residual_hash()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_candidate_behavior() {
        // ASCII letters fold; everything else is identity.
        assert_eq!(phor_toupper(b'a'), b'A');
        assert_eq!(phor_toupper(b'z'), b'Z');
        assert_eq!(phor_toupper(b'A'), b'A');
        assert_eq!(phor_toupper(b'0'), b'0');
        assert_eq!(phor_toupper(b'`'), b'`'); // 0x60, just below 'a'
        assert_eq!(phor_toupper(b'{'), b'{'); // 0x7b, just above 'z'
        assert_eq!(phor_toupper(0x00), 0x00);
        assert_eq!(phor_toupper(0xff), 0xff);
        // Non-ASCII bytes are untouched (C locale contract).
        assert_eq!(phor_toupper(0xe9), 0xe9);
    }

    #[test]
    fn test_candidate_tag_matches_constructor() {
        // The candidate symbol name must be stable (referenced by the seal).
        let target = crate::porting::target::LIBC_TOUPPER;
        let traces = crate::porting::target::byte_domain_cases()
            .iter()
            .map(|c| {
                OracleTrace::new(
                    "toupper",
                    &c.case_id,
                    &c.input,
                    &[phor_toupper(c.input[0])],
                    "ok",
                    &["compute"],
                )
            })
            .collect::<Vec<_>>();
        let sig = CandidateSignature::from_traces(&target, &traces);
        assert_eq!(sig.symbol, "phor_toupper");
        assert_eq!(sig.case_count, 256);
    }
}
