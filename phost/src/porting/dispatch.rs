// porting/dispatch.rs — Sealed native dispatch (runtime preference)
//
// This is the *runtime* side of the JIT-porting court. The court proves a target
// can be ported; dispatch proves the runtime actually **prefers the sealed native
// artifact** at a call site.
//
// At a call site the dispatcher:
//   1. looks the target up in the capability-gated sealed store;
//   2. if there is no usable sealed artifact, reports a foreign fallback (so the
//      caller uses the foreign implementation);
//   3. if a **sealed** entry exists, verifies the object's SHA-256 against the
//      seal, maps its entry function once and calls it.
//
// Fail-closed rule: a *broken seal* is never a fallback. If a sealed entry claims
// to exist but its object is missing, stale or malformed, dispatch returns an
// error. The runtime must not silently run the foreign implementation when it
// believes it is running verified native code.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::demand::{DemandSink, PortDemand};
use crate::porting::exec::{ExecError, SealedObjectHandle};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::promotion::TrustState;
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{resolve_target, PortTarget};
use crate::porting::{
    json_escape, sha256_hex, PortingAuthority, SealedArtifact, SealedPortEntry, SealedPortIndex,
};

/// Where a dispatched result came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchSource {
    /// Served by the sealed compiled object — the preferred native path.
    SealedObject,
    /// No usable sealed artifact; the caller must use the foreign implementation.
    ForeignFallback,
}

impl DispatchSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DispatchSource::SealedObject => "sealed-object",
            DispatchSource::ForeignFallback => "foreign-fallback",
        }
    }

    pub fn is_native(&self) -> bool {
        *self == DispatchSource::SealedObject
    }
}

/// Why a dispatch could not proceed. Every variant is terminal: the caller must
/// not fall back when one of these is returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchError {
    /// A sealed entry exists but its object did not verify, load or run.
    SealBroken(String),
}

impl DispatchError {
    pub fn as_str(&self) -> &'static str {
        match self {
            DispatchError::SealBroken(_) => "sealed artifact failed verification",
        }
    }
}

impl core::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DispatchError::SealBroken(m) => write!(f, "{}: {}", self.as_str(), m),
        }
    }
}

/// One dispatched call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub target: String,
    pub source: DispatchSource,
    pub trust: String,
    /// The sealed object hash that was verified (empty on a fallback).
    pub object_hash: String,
    /// The ELF symbol that was located and called (empty on a fallback).
    pub elf_symbol: String,
    pub output: Vec<u8>,
    /// Empty for a native dispatch; the fallback reason otherwise.
    pub reason: &'static str,
}

impl DispatchOutcome {
    fn fallback(port_id: &str, reason: &'static str) -> Self {
        Self {
            target: port_id.to_string(),
            source: DispatchSource::ForeignFallback,
            trust: TrustState::Unknown.as_str().to_string(),
            object_hash: String::new(),
            elf_symbol: String::new(),
            output: Vec::new(),
            reason,
        }
    }
}

/// The runtime dispatcher: a sealed-port index plus the loaded native objects.
///
/// Objects are loaded (hash-verified and mapped) lazily on first use and reused
/// afterwards, so steady-state dispatch is a plain indirect call.
///
/// A port may be a leaf object or a **composition**; a composition dispatch recurses
/// through this same dispatcher, so every nested stage is resolved from the same
/// sealed store.
pub struct NativeDispatcher {
    index: SealedPortIndex,
    loaded: BTreeMap<String, SealedObjectHandle>,
    /// Composition port ids actually dispatched through this dispatcher, so a nested
    /// stage's seal is only reported once it has really been used.
    dispatched_compositions: BTreeSet<String>,
    /// Every `dispatch_port` call, per port id — leaf or composition, including the
    /// calls a chain makes from inside its own runner. This is the fan-in of one
    /// seal into many consumers.
    per_port_dispatches: BTreeMap<String, u64>,
    dispatches: u64,
    /// The generation this dispatcher serves (recorded on a demand).
    store_generation: Option<String>,
    /// A bounded, non-blocking sink for runtime misses. Absent by default, so the
    /// existing courts and counts are unchanged.
    demand_sink: Option<Rc<DemandSink>>,
}

impl NativeDispatcher {
    pub fn new(index: SealedPortIndex) -> Self {
        Self {
            index,
            loaded: BTreeMap::new(),
            dispatched_compositions: BTreeSet::new(),
            per_port_dispatches: BTreeMap::new(),
            dispatches: 0,
            store_generation: None,
            demand_sink: None,
        }
    }

    /// Attach a non-blocking demand sink and the generation this dispatcher
    /// serves. A runtime miss then emits data instead of blocking.
    pub fn with_demand_sink(mut self, sink: Rc<DemandSink>, generation: impl Into<String>) -> Self {
        self.demand_sink = Some(sink);
        self.store_generation = Some(generation.into());
        self
    }

    /// Record a runtime miss (non-blocking; never fails the call).
    fn record_fallback(&self, port_id: &str, reason: &'static str) {
        let Some(sink) = &self.demand_sink else {
            return;
        };
        let mut available_dependencies: Vec<String> = self
            .index
            .entries()
            .iter()
            .map(|e| e.target.clone())
            .collect();
        available_dependencies.sort();
        sink.record(PortDemand {
            requested_surface: port_id.to_string(),
            callsite_family: String::from(reason),
            demand_count: 1,
            store_generation: self.store_generation.clone().unwrap_or_default(),
            available_dependencies,
        });
    }

    /// A dispatcher over a single sealed entry (the path a promoted target takes).
    pub fn with_entry(entry: SealedPortEntry) -> Self {
        let mut index = SealedPortIndex::new();
        index.insert(entry);
        Self::new(index)
    }

    pub fn index(&self) -> &SealedPortIndex {
        &self.index
    }

    /// Total `dispatch_port` calls, including the stages a chain dispatches.
    pub fn dispatches(&self) -> u64 {
        self.dispatches
    }

    /// `dispatch_port` calls per port id (fan-in), in ascending id order.
    pub fn per_port_dispatches(&self) -> &BTreeMap<String, u64> {
        &self.per_port_dispatches
    }

    /// Number of native objects currently loaded and mapped.
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// The object hash + ELF symbol (leaf) or chain hash + composition marker
    /// (composition) of the sealed artifact loaded for `target_id`.
    ///
    /// A composition has no loaded object, so its binding is the seal it published —
    /// and it is only reported once the composition has actually been dispatched.
    pub fn sealed_binding(&self, target_id: &str) -> Option<(String, String)> {
        if let Some(h) = self.loaded.get(target_id) {
            return Some((h.object_hash.clone(), h.elf_symbol.clone()));
        }
        if self.dispatched_compositions.contains(target_id) {
            if let Some(entry) = self.index.lookup(target_id) {
                if let SealedArtifact::Composition {
                    composition_id,
                    chain_hash,
                    ..
                } = &entry.artifact
                {
                    return Some((chain_hash.clone(), format!("compose:{}", composition_id)));
                }
            }
        }
        None
    }

    /// Dispatch one call for `target` (a leaf port).
    pub fn dispatch(
        &mut self,
        target: &PortTarget,
        args: &[Vec<u8>],
        auth: &PortingAuthority,
    ) -> Result<DispatchOutcome, DispatchError> {
        self.dispatch_port(target.id, args, auth)
    }

    /// Dispatch one call by qualified **port id**.
    ///
    /// A leaf id loads and calls the sealed ELF64 object. A composition id resolves to
    /// the sealed chain and recurses through this same dispatcher, so a composition is
    /// consumed like any other sealed port.
    pub fn dispatch_port(
        &mut self,
        port_id: &str,
        args: &[Vec<u8>],
        auth: &PortingAuthority,
    ) -> Result<DispatchOutcome, DispatchError> {
        // Fan-in accounting: every resolution, however deep, is counted against
        // the port it asked for.
        self.dispatches += 1;
        *self
            .per_port_dispatches
            .entry(port_id.to_string())
            .or_insert(0) += 1;

        // No ambient authority: without PORTING the sealed store reveals nothing.
        if !auth.can_observe() {
            self.record_fallback(port_id, "capability denied (no ambient authority)");
            return Ok(DispatchOutcome::fallback(
                port_id,
                "capability denied (no ambient authority)",
            ));
        }

        let entry = match self.index.lookup_gated(port_id, auth) {
            Some(e) => e.clone(),
            None => {
                self.record_fallback(port_id, "no sealed artifact in the store");
                return Ok(DispatchOutcome::fallback(
                    port_id,
                    "no sealed artifact in the store",
                ));
            }
        };

        if entry.trust != TrustState::Sealed {
            self.record_fallback(port_id, "artifact is not sealed");
            return Ok(DispatchOutcome::fallback(port_id, "artifact is not sealed"));
        }

        // A sealed entry exists: from here on, failure is terminal (fail closed).
        match entry.artifact.clone() {
            SealedArtifact::LeafObject {
                object_hash,
                object_path,
            } => {
                let target = resolve_target(port_id).ok_or_else(|| {
                    DispatchError::SealBroken(format!("no port target for {}", port_id))
                })?;
                if !self.loaded.contains_key(port_id) {
                    let handle = SealedObjectHandle::load(&target, &object_path, &object_hash)
                        .map_err(|e: ExecError| DispatchError::SealBroken(format!("{}", e)))?;
                    self.loaded.insert(port_id.to_string(), handle);
                }
                let handle = self
                    .loaded
                    .get(port_id)
                    .expect("entry was just inserted or already loaded");

                let output = handle
                    .call(&target, args)
                    .map_err(|e| DispatchError::SealBroken(format!("{}", e)))?;

                Ok(DispatchOutcome {
                    target: port_id.to_string(),
                    source: DispatchSource::SealedObject,
                    trust: entry.trust.as_str().to_string(),
                    object_hash: handle.object_hash.clone(),
                    elf_symbol: handle.elf_symbol.clone(),
                    output,
                    reason: "",
                })
            }
            SealedArtifact::Composition {
                composition_id,
                chain_hash,
                leaves,
            } => {
                // The recursion must bottom out in sealed leaves: a composition entry
                // whose leaves are not themselves sealed is a broken seal.
                for leaf in &leaves {
                    let ok = self
                        .index
                        .lookup_gated(leaf, auth)
                        .map(|e| e.trust == TrustState::Sealed)
                        .unwrap_or(false);
                    if !ok {
                        return Err(DispatchError::SealBroken(format!(
                            "composition {} requires the sealed port {}",
                            composition_id, leaf
                        )));
                    }
                }

                let output = crate::porting::composition_engine::eval_composition_port(
                    self,
                    &composition_id,
                    args,
                    auth,
                )?;
                self.dispatched_compositions.insert(port_id.to_string());

                Ok(DispatchOutcome {
                    target: port_id.to_string(),
                    source: DispatchSource::SealedObject,
                    trust: entry.trust.as_str().to_string(),
                    object_hash: chain_hash,
                    elf_symbol: format!("compose:{}", composition_id),
                    output,
                    reason: "",
                })
            }
        }
    }
}

// ============================================================================
// Sealed Native Dispatch Court
// ============================================================================

/// The dispatch residual: the runtime served the corpus from the sealed object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchVerdict {
    pub target: String,
    pub cases_run: u64,
    /// Cases served by the sealed compiled object (the preferred native path).
    pub native_cases: u64,
    /// Cases with no usable sealed artifact, so the caller uses the foreign
    /// implementation (no capability, no entry, or entry not sealed).
    pub fallback_cases: u64,
    /// Cases where a *sealed* entry existed but failed verification. This is NOT a
    /// fallback: the runtime refuses rather than silently running foreign code.
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub object_hash: String,
    pub elf_symbol: String,
    pub oracle_hash: String,
    /// SHA-256 over `case_id:source:output_hex` per case, in corpus order.
    pub dispatch_hash: String,
    pub verdict: CourtVerdict,
}

impl DispatchVerdict {
    /// The runtime preferred the sealed object for *every* case, with no foreign
    /// fallback and no broken seal, and every case matched the oracle.
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && self.cases_failed == 0
            && self.cases_passed == self.cases_run
            && !self.object_hash.is_empty()
            && !self.dispatch_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};cases_run={};native_cases={};fallback_cases={};broken_seal_cases={};cases_passed={};cases_failed={};object_hash={};elf_symbol={};oracle_hash={};dispatch_hash={};verdict={}",
            self.target,
            self.cases_run,
            self.native_cases,
            self.fallback_cases,
            self.broken_seal_cases,
            self.cases_passed,
            self.cases_failed,
            self.object_hash,
            self.elf_symbol,
            self.oracle_hash,
            self.dispatch_hash,
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[Mismatch]) -> String {
        let body: Vec<String> = mismatches.iter().map(|m| m.to_json()).collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.dispatch_verdict.v1\",\n  \"target\": \"{}\",\n  \"cases_run\": {},\n  \"native_cases\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"object_hash\": \"{}\",\n  \"elf_symbol\": \"{}\",\n  \"oracle_hash\": \"{}\",\n  \"dispatch_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            self.cases_run,
            self.native_cases,
            self.fallback_cases,
            self.broken_seal_cases,
            self.cases_passed,
            self.cases_failed,
            self.object_hash,
            json_escape(&self.elf_symbol),
            self.oracle_hash,
            self.dispatch_hash,
            self.verdict.as_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

fn dispatch_behavior_hash(outputs: &[(String, DispatchSource, Vec<u8>)]) -> String {
    let mut buf = String::new();
    for (case_id, source, out) in outputs {
        buf.push_str(case_id);
        buf.push(':');
        buf.push_str(source.as_str());
        buf.push(':');
        buf.push_str(&hex::encode(out));
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// Run the dispatch court: replay the whole corpus through the *runtime
/// dispatcher* over `index` and require every case to be served by the sealed
/// object and to match the oracle.
///
/// A broken seal (error) is recorded as a failed case — the court never treats a
/// failed verification as a successful fallback.
pub fn run_dispatch_court(
    target: &PortTarget,
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (DispatchVerdict, Vec<Mismatch>) {
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut native: u64 = 0;
    let mut fallback: u64 = 0;
    let mut broken_seal: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut outputs: Vec<(String, DispatchSource, Vec<u8>)> = Vec::with_capacity(traces.len());
    let mut object_hash = String::new();
    let mut elf_symbol = String::new();

    for t in traces {
        let args = t.input_args();
        match dispatcher.dispatch(target, &args, auth) {
            Ok(outcome) => {
                if outcome.source.is_native() {
                    native += 1;
                    if object_hash.is_empty() {
                        object_hash = outcome.object_hash.clone();
                        elf_symbol = outcome.elf_symbol.clone();
                    }
                } else {
                    fallback += 1;
                }
                let actual_hex = hex::encode(&outcome.output);
                if outcome.source.is_native() && actual_hex == t.output_hex && t.status == "ok" {
                    passed += 1;
                } else {
                    failed += 1;
                    mismatches.push(Mismatch {
                        case_id: t.case_id.clone(),
                        expected_output_hex: t.output_hex.clone(),
                        actual_output_hex: if outcome.source.is_native() {
                            actual_hex
                        } else {
                            format!("!fallback: {}", outcome.reason)
                        },
                    });
                }
                outputs.push((t.case_id.clone(), outcome.source, outcome.output));
            }
            Err(e) => {
                // Broken seal: terminal, never counted as a fallback.
                failed += 1;
                broken_seal += 1;
                mismatches.push(Mismatch {
                    case_id: t.case_id.clone(),
                    expected_output_hex: t.output_hex.clone(),
                    actual_output_hex: format!("!{}", e.as_str()),
                });
                outputs.push((
                    t.case_id.clone(),
                    DispatchSource::ForeignFallback,
                    Vec::new(),
                ));
            }
        }
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0 && fallback == 0 && broken_seal == 0 && native == cases_run {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    (
        DispatchVerdict {
            target: target.id.to_string(),
            cases_run,
            native_cases: native,
            fallback_cases: fallback,
            broken_seal_cases: broken_seal,
            cases_passed: passed,
            cases_failed: failed,
            object_hash,
            elf_symbol,
            oracle_hash: combined_oracle_hash(traces),
            dispatch_hash: dispatch_behavior_hash(&outputs),
            verdict,
        },
        mismatches,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::LIBC_TOUPPER;
    use alloc::vec;

    fn entry(path: &str, hash: &str) -> SealedPortEntry {
        SealedPortEntry {
            target: LIBC_TOUPPER.id.to_string(),
            trust: TrustState::Sealed,
            artifact: crate::porting::SealedArtifact::leaf_object(
                hash.to_string(),
                path.to_string(),
            ),
            oracle_hash: "oracle".to_string(),
            candidate_behavior_hash: "behavior".to_string(),
            candidate_source_hash: "source".to_string(),
            sealed_package: "sealed_package.json".to_string(),
        }
    }

    #[test]
    fn test_no_capability_falls_back_to_foreign() {
        let mut d = NativeDispatcher::with_entry(entry("/nonexistent.o", "deadbeef"));
        let out = d
            .dispatch(&LIBC_TOUPPER, &vec![vec![b'a']], &PortingAuthority::none())
            .unwrap();
        assert_eq!(out.source, DispatchSource::ForeignFallback);
        assert!(!out.source.is_native());
        assert!(out.reason.contains("capability"));
        // Nothing was loaded, and no error was raised (this is a legitimate
        // fallback: the store is invisible without authority).
        assert_eq!(d.loaded_count(), 0);
    }

    #[test]
    fn test_empty_store_falls_back() {
        let mut d = NativeDispatcher::new(SealedPortIndex::new());
        let out = d
            .dispatch(
                &LIBC_TOUPPER,
                &vec![vec![b'a']],
                &PortingAuthority::granted(),
            )
            .unwrap();
        assert_eq!(out.source, DispatchSource::ForeignFallback);
        assert!(out.reason.contains("no sealed artifact"));
    }

    #[test]
    fn test_unsealed_entry_falls_back() {
        let mut e = entry("/nonexistent.o", "deadbeef");
        e.trust = TrustState::OracleCompared;
        let mut d = NativeDispatcher::with_entry(e);
        let out = d
            .dispatch(
                &LIBC_TOUPPER,
                &vec![vec![b'a']],
                &PortingAuthority::granted(),
            )
            .unwrap();
        assert_eq!(out.source, DispatchSource::ForeignFallback);
        assert!(out.reason.contains("not sealed"));
    }

    /// A sealed entry whose object cannot be verified must fail closed, never
    /// fall back to the foreign implementation.
    #[test]
    fn test_broken_seal_fails_closed() {
        let mut d = NativeDispatcher::with_entry(entry("/nonexistent/candidate.o", "deadbeef"));
        let err = d
            .dispatch(
                &LIBC_TOUPPER,
                &vec![vec![b'a']],
                &PortingAuthority::granted(),
            )
            .unwrap_err();
        assert!(matches!(err, DispatchError::SealBroken(_)));
        assert_eq!(d.loaded_count(), 0);
    }

    /// The store is keyed by target id, so a lookup can only return an entry for
    /// the requested target; an entry registered for a different target is simply
    /// invisible and the call falls back.
    #[test]
    fn test_entry_for_another_target_is_invisible() {
        let mut e = entry("/nonexistent.o", "x");
        e.target = "libc:memcmp:c-locale:sign:v1".to_string();
        let mut d = NativeDispatcher::with_entry(e);
        let out = d
            .dispatch(
                &LIBC_TOUPPER,
                &vec![vec![b'a']],
                &PortingAuthority::granted(),
            )
            .unwrap();
        assert_eq!(out.source, DispatchSource::ForeignFallback);
        assert!(out.reason.contains("no sealed artifact"));
    }

    /// A corpus of mismatching traces must not be reported as a consistent
    /// dispatch just because the object loaded.
    #[test]
    fn test_dispatch_court_rejects_fallbacks() {
        let traces = vec![
            OracleTrace::single(&LIBC_TOUPPER, "0x61", &[0x61], &[0x41], "ok", &["compute"]),
            OracleTrace::single(&LIBC_TOUPPER, "0x62", &[0x62], &[0x42], "ok", &["compute"]),
        ];
        // Empty index: every case falls back.
        let (verdict, mismatches) = run_dispatch_court(
            &LIBC_TOUPPER,
            &traces,
            &SealedPortIndex::new(),
            &PortingAuthority::granted(),
        );
        assert_eq!(verdict.verdict, CourtVerdict::Inconsistent);
        assert_eq!(verdict.fallback_cases, 2);
        assert_eq!(verdict.broken_seal_cases, 0);
        assert_eq!(verdict.native_cases, 0);
        assert_eq!(mismatches.len(), 2);
        assert!(!verdict.is_sealed_eligible());
    }

    /// A sealed entry whose object cannot be verified is a *broken seal*, not a
    /// foreign fallback — the accounting language must match the philosophy.
    #[test]
    fn test_dispatch_court_separates_broken_seal_from_fallback() {
        let traces = vec![
            OracleTrace::single(&LIBC_TOUPPER, "0x61", &[0x61], &[0x41], "ok", &["compute"]),
            OracleTrace::single(&LIBC_TOUPPER, "0x62", &[0x62], &[0x42], "ok", &["compute"]),
        ];
        let mut index = SealedPortIndex::new();
        index.insert(entry("/nonexistent/candidate.o", "deadbeef"));
        let (verdict, _) =
            run_dispatch_court(&LIBC_TOUPPER, &traces, &index, &PortingAuthority::granted());
        assert_eq!(verdict.verdict, CourtVerdict::Inconsistent);
        assert_eq!(verdict.broken_seal_cases, 2);
        assert_eq!(verdict.fallback_cases, 0);
        assert_eq!(verdict.native_cases, 0);
        assert_eq!(verdict.cases_failed, 2);
        assert!(!verdict.is_sealed_eligible());
    }

    #[test]
    fn test_empty_corpus_is_inconclusive() {
        let (verdict, _) = run_dispatch_court(
            &LIBC_TOUPPER,
            &[],
            &SealedPortIndex::new(),
            &PortingAuthority::granted(),
        );
        assert_eq!(verdict.verdict, CourtVerdict::Inconclusive);
        assert!(!verdict.is_sealed_eligible());
    }

    /// A runtime miss emits data (a non-blocking demand), it does not block and
    /// does not change the fallback behavior.
    #[test]
    fn test_a_runtime_miss_emits_a_demand_without_blocking() {
        let sink = alloc::rc::Rc::new(crate::porting::demand::DemandSink::new(8));
        let mut d =
            NativeDispatcher::new(SealedPortIndex::new()).with_demand_sink(sink.clone(), "gen-1");
        let out = d
            .dispatch_port(
                "libc:atoi:c-locale:index:v1",
                &alloc::vec![alloc::vec![b'1']],
                &PortingAuthority::granted(),
            )
            .unwrap();
        assert!(!out.source.is_native());
        let demands = sink.snapshot();
        assert_eq!(demands.len(), 1);
        assert_eq!(demands[0].requested_surface, "libc:atoi:c-locale:index:v1");
        assert_eq!(demands[0].store_generation, "gen-1");
        assert_eq!(demands[0].demand_count, 1);
    }
}
