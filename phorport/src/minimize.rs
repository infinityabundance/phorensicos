// phorport/minimize.rs — court-verified counterexample minimization
//
// The FRF principle: **the minimizer proposes, the court decides.** A reduction
// is accepted only if the *same semantic comparison* that observed the original
// divergence also observes a divergence on the reduced input. A reducer never
// reports success; it proposes, and the comparison harness re-runs the whole
// pipeline (precondition gate, foreign oracle, compiled-object execution).
//
// Every attempt is retained — accepted or refused, with its residual lineage —
// so a reduction that destroyed the defect is evidence, not a silent success.

use crate::config::HarnessConfig;
use crate::harness::{probe, ProbeOutcome};

/// One reduction attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReductionAttempt {
    pub proposed_len: usize,
    pub accepted: bool,
    /// The residual family the reduced case actually exhibited (empty when invalid).
    pub residual: String,
    /// The reduction preserved the original residual lineage.
    pub lineage_preserved: bool,
}

/// The minimizer's residual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Minimization {
    pub original: Vec<u8>,
    pub minimal: Vec<u8>,
    pub original_residual: String,
    pub minimal_residual: String,
    pub attempts: Vec<ReductionAttempt>,
    pub accepted: u64,
    pub refused: u64,
}

/// Propose reductions of `data`: drop one byte at each position.
fn proposals(data: &[u8]) -> Vec<Vec<u8>> {
    (0..data.len())
        .map(|i| {
            let mut v = data.to_vec();
            v.remove(i);
            v
        })
        .collect()
}

/// Minimize a failing input, accepting only reductions the court still rejects.
///
/// Greedy and deterministic: a strictly smaller reduction is accepted only when
/// the comparison harness reports a valid, diverging case; then minimization
/// restarts from the reduced input until no single-byte drop preserves the
/// divergence.
pub fn minimize(config: &HarnessConfig, data: &[u8], original: &ProbeOutcome) -> Minimization {
    let mut current = data.to_vec();
    let mut attempts = Vec::new();
    let mut accepted = 0u64;
    let mut refused = 0u64;

    'outer: loop {
        for candidate in proposals(&current) {
            let outcome = probe(config, &candidate);
            let ok = outcome.valid && !outcome.matched;
            let lineage = ok && outcome.residual == original.residual;
            attempts.push(ReductionAttempt {
                proposed_len: candidate.len(),
                accepted: ok,
                residual: if outcome.valid {
                    outcome.residual.to_string()
                } else {
                    String::new()
                },
                lineage_preserved: lineage,
            });
            if ok {
                current = candidate;
                accepted += 1;
                continue 'outer;
            }
            refused += 1;
        }
        break;
    }

    let minimal = probe(config, &current);
    Minimization {
        original: data.to_vec(),
        minimal: current,
        original_residual: original.residual.to_string(),
        minimal_residual: minimal.residual.to_string(),
        attempts,
        accepted,
        refused,
    }
}
