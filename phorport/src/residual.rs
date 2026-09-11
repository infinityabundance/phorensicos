// phorport/residual.rs — the Phorensicos structural residual bank
//
// A structural residual is a deterministic classification of *how* a candidate
// diverged from the oracle. It is a hypothesis generator, never a correctness
// claim: it tells the explorer which family of defect the current trajectory
// looks like, so the next experiment can target it.
//
// These names are Phorensicos port semantics. They are deliberately NOT the
// domain names of unrelated DSFB/telemetry banks.

use phost::porting::candidate::decode_index;
use phost::porting::portspec::ObservableSpec;

/// The port-semantics residual families.
pub const FAMILIES: &[&str] = &[
    "PORT.BYTE_CONTENT",
    "PORT.ORDERING",
    "PORT.LENGTH",
    "PORT.ABSENT_SENTINEL",
    "PORT.FIRST_VS_LAST",
    "PORT.POSITION",
    "PORT.BOUND_ESCAPE",
    "PORT.HARNESS",
];

/// Classify a divergence between the oracle and the candidate for an observable.
pub fn classify(observable: ObservableSpec, oracle: &[u8], candidate: &[u8]) -> &'static str {
    match observable {
        ObservableSpec::ExactBytes | ObservableSpec::UnsignedInteger { .. } => "PORT.BYTE_CONTENT",
        ObservableSpec::SignedInteger { .. } | ObservableSpec::Sign => "PORT.ORDERING",
        ObservableSpec::Length => "PORT.LENGTH",
        ObservableSpec::IndexOrAbsent => {
            let o = decode_index(oracle);
            let c = decode_index(candidate);
            if (o < 0) != (c < 0) {
                "PORT.ABSENT_SENTINEL"
            } else if c > o {
                "PORT.FIRST_VS_LAST"
            } else if c < o {
                "PORT.POSITION"
            } else {
                "PORT.HARNESS"
            }
        }
    }
}

/// A deterministic integer distance between the two observables (0 = identical).
pub fn distance(observable: ObservableSpec, oracle: &[u8], candidate: &[u8]) -> u64 {
    match observable {
        ObservableSpec::IndexOrAbsent
        | ObservableSpec::SignedInteger { .. }
        | ObservableSpec::Sign => {
            let o = decode_index(oracle) as i64;
            let c = decode_index(candidate) as i64;
            (o - c).unsigned_abs()
        }
        ObservableSpec::Length => {
            let o = phost::porting::candidate::decode_usize(oracle) as u64;
            let c = phost::porting::candidate::decode_usize(candidate) as u64;
            o.abs_diff(c)
        }
        ObservableSpec::ExactBytes | ObservableSpec::UnsignedInteger { .. } => {
            let n = oracle.len().max(candidate.len());
            let mut d = (oracle.len() as i64 - candidate.len() as i64).unsigned_abs();
            for i in 0..n {
                let a = oracle.get(i).copied().unwrap_or(0);
                let b = candidate.get(i).copied().unwrap_or(0);
                d += (a ^ b).count_ones() as u64;
            }
            d
        }
    }
}
