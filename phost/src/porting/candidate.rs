// porting/candidate.rs — Clean-room native candidate
//
// The native Phorensic implementation of the target, written from the public
// specification and the observed traces — not from any foreign source. For
// `toupper` this is a byte-in/byte-out function; the mirror `.phor` expression
// of the same logic lives at examples/jit_port_toupper.phor.
//
// Fail-closed rule: an unknown target produces `CandidateError::UnsupportedTarget`,
// never an identity transform. An unsupported candidate can therefore never pass
// the court by accident.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::{PortTarget, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex};

/// Why a candidate could not produce a result for a case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateError {
    /// No native implementation is registered for this target id.
    UnsupportedTarget(String),
}

impl CandidateError {
    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateError::UnsupportedTarget(_) => "unsupported candidate target",
        }
    }
}

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

/// Run the native candidate for a qualified target id over raw input bytes.
///
/// Returns `Err(UnsupportedTarget)` for unknown ids — there is no identity
/// fallback, so an unsupported target cannot masquerade as a passing port.
pub fn run_candidate(target_id: &str, input: &[u8]) -> Result<Vec<u8>, CandidateError> {
    match target_id {
        id if id == LIBC_TOUPPER.id => Ok(input.iter().map(|&b| phor_toupper(b)).collect()),
        other => Err(CandidateError::UnsupportedTarget(String::from(other))),
    }
}

/// SHA-256 over `case_id:candidate_output_hex` per case, in domain order. Cases
/// the candidate cannot produce are encoded as `!` (never as their input), so an
/// unsupported candidate yields a distinct, non-passing hash.
pub fn candidate_behavior_hash(traces: &[OracleTrace]) -> String {
    let mut buf = String::new();
    for t in traces {
        let encoded = match run_candidate(&t.target, &t.input_bytes()) {
            Ok(out) => hex::encode(&out),
            Err(_) => String::from("!"),
        };
        buf.push_str(&t.case_id);
        buf.push(':');
        buf.push_str(&encoded);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// The native candidate, described as a residual.
///
/// `candidate_behavior_hash` covers the candidate's *behavior* over the case
/// domain. `candidate_source_hash` binds the clean-room `.phor` source. An
/// emitted-object hash will be added once the compiled `.phor` candidate is
/// authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateSignature {
    pub target: String,
    pub symbol: String,
    pub case_count: u64,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub source: String,
}

impl CandidateSignature {
    pub fn from_traces(
        target: &PortTarget,
        traces: &[OracleTrace],
        candidate_source_hash: &str,
    ) -> Self {
        Self {
            target: target.id.to_string(),
            symbol: "phor_toupper".to_string(),
            case_count: traces.len() as u64,
            candidate_behavior_hash: candidate_behavior_hash(traces),
            candidate_source_hash: candidate_source_hash.to_string(),
            source: format!(
                "clean-room native (phost::porting::candidate + {})",
                target.candidate_source
            ),
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};symbol={};case_count={};candidate_behavior_hash={};candidate_source_hash={};source={}",
            self.target,
            self.symbol,
            self.case_count,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            self.source
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.candidate_signature.v1\",\n  \"target\": \"{}\",\n  \"symbol\": \"{}\",\n  \"case_count\": {},\n  \"candidate_behavior_hash\": \"{}\",\n  \"candidate_source_hash\": \"{}\",\n  \"source\": \"{}\",\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.symbol),
            self.case_count,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
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
    fn test_unknown_candidate_fails_closed() {
        let err = run_candidate("libc:strlen:c-locale:u64:v1", &[0x41]);
        assert_eq!(
            err,
            Err(CandidateError::UnsupportedTarget(String::from(
                "libc:strlen:c-locale:u64:v1"
            )))
        );
    }

    #[test]
    fn test_candidate_tag_and_source_binding() {
        let target = LIBC_TOUPPER;
        let traces = crate::porting::target::byte_domain_cases()
            .iter()
            .map(|c| {
                OracleTrace::new(
                    &target,
                    &c.case_id,
                    &c.input,
                    &[phor_toupper(c.input[0])],
                    "ok",
                    &["compute"],
                )
            })
            .collect::<Vec<_>>();
        let sig = CandidateSignature::from_traces(&target, &traces, "sourcehash");
        assert_eq!(sig.symbol, "phor_toupper");
        assert_eq!(sig.target, "libc:toupper:c-locale:u8:v1");
        assert_eq!(sig.case_count, 256);
        assert_eq!(sig.candidate_source_hash, "sourcehash");
        assert!(!sig.candidate_behavior_hash.is_empty());
    }
}
