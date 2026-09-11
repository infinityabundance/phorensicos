// phorport/harness.rs — the differential comparison harness
//
// This is the heart of Phase 4's counterexample engine. A probe:
//
//   1. decodes (or receives) a port case;
//   2. validates it against the port's `PortSpec` preconditions (only a valid
//      case reaches the foreign oracle — undefined/out-of-contract inputs are
//      never used as an oracle);
//   3. observes the foreign implementation through the dialect cage;
//   4. executes the **uninstrumented compiled `.phor` object** (hash-verified
//      before mapping) through a pluggable `CandidateExec`;
//   5. compares the normalized observables and classifies any divergence into a
//      structural residual family.
//
// The comparison is the *only* comparison implementation; the fuzz target
// wrapper, the out-of-process minimizer, the qualification court and the
// contained worker all call it, so the same semantic comparison decides every
// stage. It is `std` host code and never enters a runtime closure.
//
// Phase 7.5 introduced `CandidateExec`: the in-process executor is the default
// (and what an frf-fuzz worker already contains), and a contained worker
// executor is used by the autonomous campaign so a machine-generated candidate
// is never called inside the coordinator process. The comparison code below is
// identical for both — the execution is the only thing that moves.

use std::cell::RefCell;

use phost::porting::exec::SealedObjectHandle;
use phost::porting::target::{resolve_target, PortTarget};
use phost::porting::{dialect_cage, portspec, PortingAuthority};

use crate::case::decode_args;
use crate::config::HarnessConfig;
use crate::residual;

/// How a candidate object is executed for one case.
///
/// `InProcessExec` is the default (and what an frf-fuzz worker already
/// contains). `ContainedExec` (worker.rs) routes the call to a bounded worker
/// process. The harness never cares which one it is: the comparison below is
/// the same.
pub trait CandidateExec {
    fn call(
        &self,
        config: &HarnessConfig,
        target: &PortTarget,
        args: &[Vec<u8>],
    ) -> Result<Vec<u8>, String>;
}

/// The outcome of one differential probe.
#[derive(Clone, Debug)]
pub struct ProbeOutcome {
    /// The decoded case satisfied every declared precondition.
    pub valid: bool,
    /// The candidate matched the oracle (only meaningful when `valid`).
    pub matched: bool,
    /// A deterministic integer distance between the observables.
    pub distance: u64,
    /// The structural residual family when the case diverged.
    pub residual: &'static str,
    /// The observed oracle output (hex), when valid.
    pub oracle_hex: String,
    /// The executed candidate output (hex), when valid.
    pub candidate_hex: String,
    /// The normalized case id.
    pub case_id: String,
}

impl ProbeOutcome {
    fn invalid(case_id: String) -> Self {
        ProbeOutcome {
            valid: false,
            matched: true,
            distance: 0,
            residual: "PORT.HARNESS",
            oracle_hex: String::new(),
            candidate_hex: String::new(),
            case_id,
        }
    }
}

// ---------------------------------------------------------------------------
// In-process execution (the default)
// ---------------------------------------------------------------------------

thread_local! {
    /// A persistent worker maps a candidate once and reuses it for every probe.
    static HANDLE: RefCell<Option<(String, SealedObjectHandle)>> = const { RefCell::new(None) };
}

/// Execute the compiled object inside this process.
///
/// This is correct inside an frf-fuzz worker (which is already a separate
/// process) and inside the deterministic tests. It is **not** used for the
/// autonomous campaign's untrusted candidates; that path uses
/// [`crate::worker::ContainedExec`].
pub struct InProcessExec;

impl CandidateExec for InProcessExec {
    fn call(
        &self,
        config: &HarnessConfig,
        target: &PortTarget,
        args: &[Vec<u8>],
    ) -> Result<Vec<u8>, String> {
        with_handle(target, config, args)
    }
}

fn with_handle(
    target: &PortTarget,
    config: &HarnessConfig,
    args: &[Vec<u8>],
) -> Result<Vec<u8>, String> {
    let hash = config.candidate_hash.clone();
    let path = config.candidate_path.clone();
    HANDLE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().map(|(h, _)| h != &hash).unwrap_or(true) {
            let handle =
                SealedObjectHandle::load(target, &path, &hash).map_err(|e| e.to_string())?;
            *slot = Some((hash.clone(), handle));
        }
        match slot.as_ref() {
            Some((_, h)) => h.call(target, args).map_err(|e| e.to_string()),
            None => Err(String::from("no candidate handle")),
        }
    })
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// Run one differential probe for `config` on a decoded case.
pub fn probe_args_with(
    exec: &dyn CandidateExec,
    config: &HarnessConfig,
    args: &[Vec<u8>],
) -> ProbeOutcome {
    let case_id = format!("case:{}", case_digest(args));
    probe_validated(exec, config, &case_id, args)
}

/// Run one differential probe for `config` on a **validated** case.
///
/// The case id is supplied by the caller so a qualification or minimization
/// case keeps its own identity through the comparison.
pub fn probe_validated(
    exec: &dyn CandidateExec,
    config: &HarnessConfig,
    case_id: &str,
    args: &[Vec<u8>],
) -> ProbeOutcome {
    let case_id = case_id.to_string();
    let target = match resolve_target(&config.target_id) {
        Some(t) => t,
        None => return ProbeOutcome::invalid(case_id),
    };
    let spec = match portspec::by_target_id(&config.target_id) {
        Some(s) => s,
        None => return ProbeOutcome::invalid(case_id),
    };
    // 1. The precondition gate: only a validated case reaches the oracle.
    if portspec::validate_case(spec, args).is_err() {
        return ProbeOutcome::invalid(case_id);
    }
    // 2. The foreign oracle.
    let auth = PortingAuthority::granted();
    let case = phost::porting::target::TestCase::new(case_id.clone(), args.to_vec());
    let traces = match dialect_cage::observe_target(&target, core::slice::from_ref(&case), &auth) {
        Ok(t) => t,
        Err(_) => return ProbeOutcome::invalid(case_id),
    };
    let trace = match traces.first() {
        Some(t) => t,
        None => return ProbeOutcome::invalid(case_id),
    };
    let oracle = hex::decode(&trace.output_hex).unwrap_or_default();
    // 3. The compiled object, through the supplied executor.
    let candidate = match exec.call(config, &target, args) {
        Ok(out) => out,
        // A candidate that cannot run is a divergence (rejected), not invalid.
        Err(_) => {
            return ProbeOutcome {
                valid: true,
                matched: false,
                distance: u64::MAX,
                residual: "PORT.HARNESS",
                oracle_hex: trace.output_hex.clone(),
                candidate_hex: String::new(),
                case_id,
            }
        }
    };
    // 4. The comparison.
    let matched = candidate == oracle;
    let residual = if matched {
        "PORT.BYTE_CONTENT"
    } else {
        residual::classify(spec.observable, &oracle, &candidate)
    };
    ProbeOutcome {
        valid: true,
        matched,
        distance: residual::distance(spec.observable, &oracle, &candidate),
        residual,
        oracle_hex: trace.output_hex.clone(),
        candidate_hex: hex::encode(&candidate),
        case_id,
    }
}

/// The default in-process probe on a decoded fuzz input.
pub fn probe(config: &HarnessConfig, data: &[u8]) -> ProbeOutcome {
    let case_id = format!("fuzz:{}", hex::encode(data));
    let target = match resolve_target(&config.target_id) {
        Some(t) => t,
        None => return ProbeOutcome::invalid(case_id),
    };
    let args = match decode_args(&target, data) {
        Some(a) => a,
        None => return ProbeOutcome::invalid(case_id),
    };
    probe_validated(&InProcessExec, config, &case_id, &args)
}

/// The in-process probe on explicit arguments.
pub fn probe_args(config: &HarnessConfig, args: &[Vec<u8>]) -> ProbeOutcome {
    probe_args_with(&InProcessExec, config, args)
}

/// A stable digest of an argument vector (for a deterministic case id).
fn case_digest(args: &[Vec<u8>]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for a in args {
        for b in a {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h ^= 0x1f;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Probe the environment-configured target (the fuzz-target wrapper's entry).
pub fn probe_env(data: &[u8]) -> ProbeOutcome {
    match HarnessConfig::from_env() {
        Some(c) => probe(&c, data),
        None => ProbeOutcome::invalid(String::from("fuzz:unconfigured")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_digest_is_stable_and_order_sensitive() {
        let a = vec![vec![1u8, 2], vec![3]];
        let b = vec![vec![1u8, 2], vec![3]];
        assert_eq!(case_digest(&a), case_digest(&b));
        let c = vec![vec![3u8], vec![1, 2]];
        assert_ne!(case_digest(&a), case_digest(&c));
    }
}
