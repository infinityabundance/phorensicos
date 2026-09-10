// porting/candidate.rs — Clean-room native candidates
//
// The native Phorensic implementations of the targets, written from the public
// specification and the observed traces — not from any foreign source. The
// mirror `.phor` expressions of the same logic live at
// examples/jit_port_toupper.phor and examples/jit_port_memcmp.phor.
//
// Fail-closed rule: an unknown target produces `CandidateError::UnsupportedTarget`
// and malformed arguments produce `CandidateError::MalformedArgs` — never an
// identity transform. An unsupported candidate can therefore never pass the
// court by accident.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::{PortTarget, LIBC_MEMCMP, LIBC_TOUPPER};
use crate::porting::{json_escape, sha256_hex};

/// Why a candidate could not produce a result for a case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateError {
    /// No native implementation is registered for this target id.
    UnsupportedTarget(String),
    /// The target was known but the case arguments were malformed.
    MalformedArgs(String),
}

impl CandidateError {
    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateError::UnsupportedTarget(_) => "unsupported candidate target",
            CandidateError::MalformedArgs(_) => "malformed candidate arguments",
        }
    }
}

// ============================================================================
// toupper
// ============================================================================

/// Native clean-room `toupper`: map ASCII `a`..=`z` to `A`..=`Z`, leave every
/// other byte unchanged (C-locale contract).
#[inline]
pub fn phor_toupper(b: u8) -> u8 {
    if b >= b'a' && b <= b'z' {
        b - (b'a' - b'A')
    } else {
        b
    }
}

// ============================================================================
// memcmp
// ============================================================================

/// Native clean-room `memcmp`, returning the **sign** of the ordering:
/// `-1` if `a` is less than `b`, `0` if equal over `n` bytes, `1` if greater.
///
/// Comparison is over `n` bytes (bounded by both buffers), byte-wise and
/// **unsigned** — the C contract. The exact integer is not part of the contract,
/// so only the sign is produced.
#[inline]
pub fn phor_memcmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    let m = n.min(a.len()).min(b.len());
    let mut i = 0;
    while i < m {
        if a[i] != b[i] {
            return if a[i] < b[i] { -1 } else { 1 };
        }
        i += 1;
    }
    0
}

/// Normalize an integer to its sign and encode as 4 little-endian bytes.
/// Shared with the dialect cage so observed and candidate orderings use the
/// same encoding.
pub fn encode_sign(value: i32) -> Vec<u8> {
    let normalized: i32 = if value < 0 {
        -1
    } else if value > 0 {
        1
    } else {
        0
    };
    normalized.to_le_bytes().to_vec()
}

/// Decode a little-endian `usize` argument (the compared length).
pub fn decode_usize(bytes: &[u8]) -> usize {
    let mut buf = [0u8; 8];
    let n = bytes.len().min(8);
    buf[..n].copy_from_slice(&bytes[..n]);
    u64::from_le_bytes(buf) as usize
}

/// Run the native candidate for a qualified target id over its argument list.
///
/// Returns `Err(UnsupportedTarget)` for unknown ids and `Err(MalformedArgs)` for
/// known ids with the wrong shape — there is no identity fallback.
pub fn run_candidate(target_id: &str, args: &[Vec<u8>]) -> Result<Vec<u8>, CandidateError> {
    if target_id == LIBC_TOUPPER.id {
        let b = args
            .first()
            .and_then(|a| a.first())
            .copied()
            .ok_or_else(|| CandidateError::MalformedArgs(target_id.to_string()))?;
        return Ok(alloc::vec![phor_toupper(b)]);
    }

    if target_id == LIBC_MEMCMP.id {
        if args.len() < 3 {
            return Err(CandidateError::MalformedArgs(target_id.to_string()));
        }
        let sign = phor_memcmp(&args[0], &args[1], decode_usize(&args[2]));
        return Ok(encode_sign(sign));
    }

    Err(CandidateError::UnsupportedTarget(target_id.to_string()))
}

/// SHA-256 over `case_id:candidate_output_hex` per case, in corpus order. Cases
/// the candidate cannot produce are encoded as `!<reason>`, so an unsupported or
/// malformed candidate yields a distinct, non-passing hash.
pub fn candidate_behavior_hash(traces: &[OracleTrace]) -> String {
    let mut buf = String::new();
    for t in traces {
        let encoded = match run_candidate(&t.target, &t.input_args()) {
            Ok(out) => hex::encode(&out),
            Err(e) => format!("!{}", e.as_str()),
        };
        buf.push_str(&t.case_id);
        buf.push(':');
        buf.push_str(&encoded);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// A native candidate, described as a residual.
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
            symbol: format!("phor_{}", target.symbol),
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
    use crate::porting::target::memcmp_corpus;

    #[test]
    fn test_native_toupper_behavior() {
        assert_eq!(phor_toupper(b'a'), b'A');
        assert_eq!(phor_toupper(b'z'), b'Z');
        assert_eq!(phor_toupper(b'A'), b'A');
        assert_eq!(phor_toupper(b'0'), b'0');
        assert_eq!(phor_toupper(b'`'), b'`');
        assert_eq!(phor_toupper(b'{'), b'{');
        assert_eq!(phor_toupper(0x00), 0x00);
        assert_eq!(phor_toupper(0xff), 0xff);
        assert_eq!(phor_toupper(0xe9), 0xe9);
    }

    #[test]
    fn test_native_memcmp_behavior() {
        assert_eq!(phor_memcmp(b"abc", b"abc", 3), 0);
        assert_eq!(phor_memcmp(b"abc", b"abd", 3), -1);
        assert_eq!(phor_memcmp(b"abd", b"abc", 3), 1);
        // n bounds the comparison.
        assert_eq!(phor_memcmp(b"abc", b"abd", 2), 0);
        assert_eq!(phor_memcmp(b"abc", b"abd", 0), 0);
        // Unsigned comparison: 0x80 > 0x7f.
        assert_eq!(phor_memcmp(&[0x80], &[0x7f], 1), 1);
        assert_eq!(phor_memcmp(&[0x7f], &[0x80], 1), -1);
        // First mismatch decides; later differences are irrelevant.
        assert_eq!(phor_memcmp(&[0x01, 0x00], &[0x02, 0xff], 2), -1);
    }

    #[test]
    fn test_unknown_candidate_fails_closed() {
        let err = run_candidate("libc:strlen:c-locale:u64:v1", &[alloc::vec![0x41]]);
        assert_eq!(
            err,
            Err(CandidateError::UnsupportedTarget(String::from(
                "libc:strlen:c-locale:u64:v1"
            )))
        );
    }

    #[test]
    fn test_malformed_args_fail_closed() {
        // memcmp with too few arguments must not be treated as identity.
        let err = run_candidate(LIBC_MEMCMP.id, &[alloc::vec![0x41]]);
        assert_eq!(
            err,
            Err(CandidateError::MalformedArgs(String::from(
                "libc:memcmp:c-locale:sign:v1"
            )))
        );
    }

    #[test]
    fn test_memcmp_candidate_covers_corpus_without_error() {
        for case in memcmp_corpus() {
            match run_candidate(LIBC_MEMCMP.id, &case.args) {
                Ok(out) => assert_eq!(out.len(), 4, "case {}", case.case_id),
                Err(e) => panic!("case {} errored: {:?}", case.case_id, e),
            }
        }
    }

    #[test]
    fn test_candidate_source_binding() {
        let target = LIBC_MEMCMP;
        let traces = memcmp_corpus()
            .iter()
            .map(|c| {
                let out = run_candidate(LIBC_MEMCMP.id, &c.args).unwrap();
                OracleTrace::new(&target, &c.case_id, &c.args, &out, "ok", &["compute"])
            })
            .collect::<Vec<_>>();
        let sig = CandidateSignature::from_traces(&target, &traces, "sourcehash");
        assert_eq!(sig.symbol, "phor_memcmp");
        assert_eq!(sig.target, "libc:memcmp:c-locale:sign:v1");
        assert_eq!(sig.case_count, 312);
        assert_eq!(sig.candidate_source_hash, "sourcehash");
        assert!(!sig.candidate_behavior_hash.is_empty());
    }
}
