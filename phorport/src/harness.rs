// phorport/harness.rs — the differential comparison harness
//
// This is the heart of Phase 4's counterexample engine. A probe:
//
//   1. decodes a fuzz input into a port case;
//   2. validates it against the port's `PortSpec` preconditions (only a valid
//      case reaches the foreign oracle — undefined/out-of-contract inputs are
//      never used as an oracle);
//   3. observes the foreign implementation through the dialect cage;
//   4. executes the **uninstrumented compiled `.phor` object** (hash-verified
//      before mapping);
//   5. compares the normalized observables and classifies any divergence into a
//      structural residual family.
//
// The harness is the *only* comparison implementation; the fuzz target wrapper
// and the out-of-process minimizer both call it, so the same semantic comparison
// decides every stage. It is `std` host code and never enters a runtime closure.

use std::cell::RefCell;

use phost::porting::exec::SealedObjectHandle;
use phost::porting::target::{resolve_target, TestCase};
use phost::porting::{dialect_cage, portspec, PortingAuthority};

use crate::case::decode_args;
use crate::config::HarnessConfig;
use crate::residual;

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

thread_local! {
    /// A persistent worker maps a candidate once and reuses it for every probe.
    static HANDLE: RefCell<Option<(String, SealedObjectHandle)>> = const { RefCell::new(None) };
}

fn with_handle<R>(config: &HarnessConfig, f: impl FnOnce(&SealedObjectHandle) -> R) -> Option<R> {
    HANDLE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot
            .as_ref()
            .map(|(h, _)| h != &config.candidate_hash)
            .unwrap_or(true)
        {
            let target = resolve_target(&config.target_id)?;
            let handle =
                SealedObjectHandle::load(&target, &config.candidate_path, &config.candidate_hash)
                    .ok()?;
            *slot = Some((config.candidate_hash.clone(), handle));
        }
        slot.as_ref().map(|(_, h)| f(h))
    })
}

/// Run one differential probe for `config` on `data`.
pub fn probe(config: &HarnessConfig, data: &[u8]) -> ProbeOutcome {
    let case_id = format!("fuzz:{}", hex::encode(data));
    let target = match resolve_target(&config.target_id) {
        Some(t) => t,
        None => return ProbeOutcome::invalid(case_id),
    };
    let spec = match portspec::by_target_id(&config.target_id) {
        Some(s) => s,
        None => return ProbeOutcome::invalid(case_id),
    };
    let args = match decode_args(&target, data) {
        Some(a) => a,
        None => return ProbeOutcome::invalid(case_id),
    };
    // 1. The precondition gate: only a validated case reaches the oracle.
    if portspec::validate_case(spec, &args).is_err() {
        return ProbeOutcome::invalid(case_id);
    }
    // 2. The foreign oracle.
    let auth = PortingAuthority::granted();
    let case = TestCase::new(case_id.clone(), args.clone());
    let traces = match dialect_cage::observe_target(&target, core::slice::from_ref(&case), &auth) {
        Ok(t) => t,
        Err(_) => return ProbeOutcome::invalid(case_id),
    };
    let trace = match traces.first() {
        Some(t) => t,
        None => return ProbeOutcome::invalid(case_id),
    };
    let oracle = hex::decode(&trace.output_hex).unwrap_or_default();
    // 3. The compiled object, mapped once per worker.
    let candidate = match with_handle(config, |h| h.call(&target, &args)) {
        Some(Ok(out)) => out,
        // A candidate that cannot run is a divergence (rejected), not invalid.
        Some(Err(_)) => {
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
        None => return ProbeOutcome::invalid(case_id),
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

/// Probe the environment-configured target (the fuzz-target wrapper's entry).
pub fn probe_env(data: &[u8]) -> ProbeOutcome {
    match HarnessConfig::from_env() {
        Some(c) => probe(&c, data),
        None => ProbeOutcome::invalid(String::from("fuzz:unconfigured")),
    }
}
