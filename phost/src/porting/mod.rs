// porting/mod.rs — JIT-Porting Court
//
// Observe a foreign API surface as a black box through a dialect cage, seal the
// observed behavior as oracle traces, replay a clean-room native candidate
// against those traces, and promote the candidate to `sealed` only when every
// case matches exactly and the compiled candidate artifact is bound.
//
// Scope: **API-surface** porting (byte-in/byte-out and small buffer functions).
// This is NOT arbitrary binary translation. The cage observes a foreign
// implementation; it does not copy it.
//
// Trust ladder (docs/REPLAY_COURTS.md):
//   Unknown(0) → Observed(1) → Replayed(2) → OracleCompared(3)
//              → ResidualStable(4) → Sealed(5)
//
// Residual map (one machine-readable artifact per stage):
//   observation / oracle traces → oracle_traces.json
//   behavior signature          → behavior_signature.json
//   candidate residual          → candidate_signature.json
//   replay + comparison         → replay_verdict.json
//   promotion                   → promotion_receipt.json
//   sealed package              → sealed_package.json
//   compiled candidate          → candidate.o + candidate.receipts.json (hashed)

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::kernel::CapabilitySet;

pub mod abi;
pub mod abi_v2;
pub mod autonomous_seal;
pub mod behavior_signature;
pub mod candidate;
pub mod challenge;
pub mod compiled;
pub mod composition;
pub mod composition_engine;
pub mod composition_ir;
pub mod composition_nested;
pub mod composition_pair;
pub mod composition_registry;
pub mod composition_slice_search;
pub mod composition_strlen_memchr;
pub mod composition_suffix;
pub mod composition_synth;
pub mod composition_toupper_each;
pub mod cross_impl;
pub mod demand;
pub mod dialect_cage;
pub mod dispatch;
pub mod evidence;
pub mod exec;
pub mod generation_session;
pub mod ident;
pub mod json;
pub mod oracle_trace;
pub mod oracle_witness;
pub mod portspec;
pub mod promotion;
pub mod registry;
pub mod replay_court;
pub mod service;
pub mod store;
pub mod store_generation;
pub mod supersession;
pub mod target;

pub use behavior_signature::BehaviorSignature;
pub use candidate::{CandidateArtifacts, CandidateSignature};
pub use composition::{CompositionTarget, CompositionVerdict, COMPOSITION_TOUPPER_MEMCHR};
pub use dispatch::{
    DispatchError, DispatchOutcome, DispatchSource, DispatchVerdict, NativeDispatcher,
};
pub use exec::{ExecError, ExecutionVerdict};
pub use oracle_trace::OracleTrace;
pub use promotion::{PromotionError, PromotionEvidence, PromotionReceipt, TrustState};
pub use replay_court::{CourtVerdict, Mismatch, ReplayVerdict};
pub use service::{SealedNativeService, SessionVerdict};
pub use target::{resolve_target, PortTarget, TestCase};

/// A chain runner, callable with the raw stage arguments and returning the encoded
/// stage output.
///
/// Phase 2 no longer resolves a sealed composition to one of these by id: a
/// composition is now **data** (`composition_registry`), evaluated by one
/// interpreter (`composition_ir::eval`) through `composition_engine`. The type is
/// retained only so the legacy Phase 1 courts (the historical `v1` evidence model)
/// keep compiling; the runtime never uses it.
pub type CompositionRunner =
    fn(&mut NativeDispatcher, &[Vec<u8>], &PortingAuthority) -> Result<Vec<u8>, DispatchError>;

/// SHA-256, lowercase hex. The strongest hash already used by the repo
/// (phorc's `receipts::hash_bytes`); no weak bootstrap hashes in court evidence.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out: [u8; 32] = hasher.finalize().into();
    hex::encode(out)
}

/// Minimal JSON string escaping for the writer (values are identifiers/hex).
pub fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Errors from the porting court. All failures fail *closed*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortError {
    /// The caller lacks `CapabilitySet::PORTING`.
    CapabilityDenied,
    /// No such port target.
    UnknownTarget(String),
    /// The dialect cage cannot observe this symbol.
    UnsupportedTarget(String),
    /// Compiling the clean-room candidate failed.
    Compile(String),
    /// The sealed object could not be loaded/executed (execution court).
    Execution(String),
    /// Evidence I/O failed (the court never proceeds on I/O error).
    Io(String),
    /// Promotion was refused by the court.
    PromotionRefused(PromotionError),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortError::CapabilityDenied => write!(f, "capability denied: PORTING required"),
            PortError::UnknownTarget(t) => write!(f, "unknown port target: {}", t),
            PortError::UnsupportedTarget(t) => write!(f, "target not observable by cage: {}", t),
            PortError::Compile(m) => write!(f, "candidate compilation failed: {}", m),
            PortError::Execution(m) => write!(f, "execution court failed: {}", m),
            PortError::Io(m) => write!(f, "evidence I/O error: {}", m),
            PortError::PromotionRefused(e) => write!(f, "promotion refused: {}", e.as_str()),
        }
    }
}

/// Authority to observe foreign behavior and to promote candidates.
///
/// Porting is capability-gated at both ends: there is no ambient authority to
/// observe a foreign implementation or to seal a native replacement.
#[derive(Clone, Copy, Debug)]
pub struct PortingAuthority {
    caps: CapabilitySet,
}

impl PortingAuthority {
    /// Authority granted the `PORTING` capability.
    pub fn granted() -> Self {
        Self {
            caps: CapabilitySet {
                bits: CapabilitySet::PORTING,
            },
        }
    }

    /// No authority (used to demonstrate that the court fails closed).
    pub fn none() -> Self {
        Self {
            caps: CapabilitySet::empty(),
        }
    }

    /// Authority from an existing capability set.
    pub fn with(caps: CapabilitySet) -> Self {
        Self { caps }
    }

    pub fn can_observe(&self) -> bool {
        self.caps.has(CapabilitySet::PORTING)
    }

    pub fn can_promote(&self) -> bool {
        self.caps.has(CapabilitySet::PORTING)
    }

    pub fn caps(&self) -> CapabilitySet {
        self.caps
    }
}

/// What a sealed port actually *is*.
///
/// A port is no longer necessarily a single object: a **composition** is a sealed
/// port too, with no object of its own — just a chain of other sealed ports plus the
/// chain hash that seals it. This is what lets a composition be a stage of another
/// chain: the runtime looks it up in the store, verifies its seal, and recurses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealedArtifact {
    /// A relocation-free ELF64 object with an ABI entry point (`_phor_<symbol>`).
    LeafObject {
        object_hash: String,
        object_path: String,
    },
    /// A composition: a chain over other sealed ports.
    Composition {
        /// The qualified composition id (also the store key / `target`).
        composition_id: String,
        /// The chain hash that seals it (from its committed composition verdict).
        chain_hash: String,
        /// The qualified leaf ids the chain dispatches to. Each must itself be
        /// sealed in the same store, so the recursion bottoms out in verified
        /// objects.
        leaves: Vec<String>,
    },
}

impl SealedArtifact {
    pub fn leaf_object(object_hash: String, object_path: String) -> Self {
        SealedArtifact::LeafObject {
            object_hash,
            object_path,
        }
    }

    pub fn composition(composition_id: String, chain_hash: String, leaves: Vec<String>) -> Self {
        SealedArtifact::Composition {
            composition_id,
            chain_hash,
            leaves,
        }
    }

    /// True for a single compiled object (which has a loadable entry point).
    pub fn is_leaf(&self) -> bool {
        matches!(self, SealedArtifact::LeafObject { .. })
    }
}

/// A sealed port entry — the artifact a promoted native implementation leaves in
/// the store. Lookups are gated: without `PORTING` the entry is invisible.
///
/// For a leaf target the artifact is the *compiled* candidate object, not just the
/// Rust mirror; for a composition it is the sealed chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedPortEntry {
    pub target: String,
    pub trust: TrustState,
    pub artifact: SealedArtifact,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub sealed_package: String,
}

/// In-memory index of sealed ports (the store lookup surface).
#[derive(Clone, Debug, Default)]
pub struct SealedPortIndex {
    entries: Vec<SealedPortEntry>,
}

impl SealedPortIndex {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, entry: SealedPortEntry) {
        self.entries.push(entry);
    }

    pub fn lookup(&self, target: &str) -> Option<&SealedPortEntry> {
        self.entries.iter().find(|e| e.target == target)
    }

    /// Capability-gated lookup: sealed entries are not revealed without
    /// `PORTING`. There is no ambient authority to read the store.
    pub fn lookup_gated(&self, target: &str, auth: &PortingAuthority) -> Option<&SealedPortEntry> {
        if !auth.can_observe() {
            return None;
        }
        self.lookup(target)
    }

    /// The compiled artifact the runtime would prefer, if sealed and authorized.
    ///
    /// Only leaf entries have a loadable object; a composition has no object of its
    /// own, so this returns `None` for it.
    pub fn native_artifact(&self, target: &str, auth: &PortingAuthority) -> Option<(&str, &str)> {
        self.lookup_gated(target, auth)
            .filter(|e| e.trust == TrustState::Sealed)
            .and_then(|e| match &e.artifact {
                SealedArtifact::LeafObject {
                    object_path,
                    object_hash,
                } => Some((object_path.as_str(), object_hash.as_str())),
                SealedArtifact::Composition { .. } => None,
            })
    }

    /// The sealed chain a composition target publishes, if sealed and authorized.
    pub fn sealed_chain(&self, target: &str, auth: &PortingAuthority) -> Option<(&str, &str)> {
        self.lookup_gated(target, auth)
            .filter(|e| e.trust == TrustState::Sealed)
            .and_then(|e| match &e.artifact {
                SealedArtifact::Composition {
                    composition_id,
                    chain_hash,
                    ..
                } => Some((composition_id.as_str(), chain_hash.as_str())),
                SealedArtifact::LeafObject { .. } => None,
            })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The sealed entries, in insertion order (store inspection/serialization).
    pub fn entries(&self) -> &[SealedPortEntry] {
        &self.entries
    }
}

impl SealedPortEntry {
    /// `"leaf-object"` or `"composition"` — the artifact kind this port publishes.
    pub fn artifact_kind(&self) -> &'static str {
        match &self.artifact {
            SealedArtifact::LeafObject { .. } => "leaf-object",
            SealedArtifact::Composition { .. } => "composition",
        }
    }

    /// The verified artifact hash this port publishes: the object hash for a leaf,
    /// the chain hash for a composition.
    pub fn artifact_hash(&self) -> &str {
        match &self.artifact {
            SealedArtifact::LeafObject { object_hash, .. } => object_hash,
            SealedArtifact::Composition { chain_hash, .. } => chain_hash,
        }
    }
}

/// How far the court should run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDepth {
    /// Observe foreign behavior, seal oracle traces.
    Observe,
    /// + replay the native candidate and compare.
    Replay,
    /// + promote to sealed (writes the full evidence set).
    Promote,
}

impl PortDepth {
    pub fn parse(s: &str) -> Option<PortDepth> {
        match s {
            "observe" => Some(PortDepth::Observe),
            "replay" => Some(PortDepth::Replay),
            "promote" | "court" => Some(PortDepth::Promote),
            _ => None,
        }
    }
}

/// Outcome of a court run, suitable for printing.
#[derive(Clone, Debug)]
pub struct PortCourtReport {
    pub symbol: String,
    pub target: String,
    pub dialect: String,
    pub version: String,
    pub locale_contract: String,
    pub observed_cases: u64,
    pub replay_cases: u64,
    pub passed: u64,
    pub failed: u64,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    pub candidate_object_hash: String,
    pub candidate_receipt_hash: String,
    pub compiler_version: String,
    pub candidate_object_path: String,
    pub verdict: String,
    pub promotion: String,
    pub sealed_package: String,
    pub evidence_dir: String,
    /// Execution-court results (the sealed object was loaded and replayed).
    pub abi_symbol: String,
    pub elf_symbol: String,
    pub execution_cases: u64,
    pub execution_passed: u64,
    pub execution_failed: u64,
    pub execution_hash: String,
    pub execution_verdict: String,
    /// Dispatch-court results (the runtime preferred the sealed object).
    pub dispatch_cases: u64,
    pub dispatch_native_cases: u64,
    pub dispatch_fallback_cases: u64,
    pub dispatch_broken_seal_cases: u64,
    pub dispatch_hash: String,
    pub dispatch_verdict: String,
}

/// Observe a target's complete domain through the dialect cage.
pub fn observe_target(
    target: &PortTarget,
    cases: &[TestCase],
    auth: &PortingAuthority,
) -> Result<Vec<OracleTrace>, PortError> {
    dialect_cage::observe_target(target, cases, auth)
}

/// A single sealed-native dispatch, as observed at a runtime call site.
#[derive(Clone, Debug)]
pub struct NativeCallReport {
    pub target: String,
    pub source: String,
    pub trust: String,
    pub object_hash: String,
    pub elf_symbol: String,
    pub input_hex: String,
    pub output_hex: String,
    pub mirror_output_hex: String,
    pub matches_mirror: bool,
    pub reason: String,
    pub promotion: String,
    pub sealed_package: String,
}

/// Parse the court's `:`-joined lowercase-hex argument framing.
pub fn parse_hex_args(framed: &str) -> Result<Vec<Vec<u8>>, PortError> {
    framed
        .split(':')
        .map(|part| {
            hex::decode(part).map_err(|e| PortError::Io(format!("bad hex argument: {}", e)))
        })
        .collect()
}

/// Perform **one** dispatch through the runtime dispatcher, using the seal loadable
/// from the **persistent store**.
///
/// This is the call-site path: the committed store is loaded and verified (no
/// compiler invocation and no oracle replay), the target is looked up, the sealed
/// object is loaded and called — or the call site reports a foreign fallback.
///
/// `auth` is the capability the call site really has: without `PORTING` the store
/// reveals nothing and the dispatch falls back, exactly as at a real call site.
pub fn run_native_call(
    symbol: &str,
    args_framed: Option<&str>,
    store_path: &str,
    auth: &PortingAuthority,
) -> Result<NativeCallReport, PortError> {
    let target =
        resolve_target(symbol).ok_or_else(|| PortError::UnknownTarget(symbol.to_string()))?;

    let args = match args_framed {
        Some(framed) => parse_hex_args(framed)?,
        None => default_args(&target),
    };
    let input_hex = args
        .iter()
        .map(hex::encode)
        .collect::<Vec<String>>()
        .join(":");

    // No ambient authority: without PORTING the store is not even read, and the
    // call site falls back to the foreign implementation.
    if !auth.can_observe() {
        return Ok(NativeCallReport {
            target: target.id.to_string(),
            source: DispatchSource::ForeignFallback.as_str().to_string(),
            trust: TrustState::Unknown.as_str().to_string(),
            object_hash: String::new(),
            elf_symbol: String::new(),
            input_hex,
            output_hex: String::new(),
            mirror_output_hex: String::new(),
            matches_mirror: false,
            reason: String::from("capability denied (no ambient authority)"),
            promotion: TrustState::Unknown.as_str().to_string(),
            sealed_package: String::new(),
        });
    }

    // Load the *committed* seal: no `phorc`, no cage, no replay.
    let index = store::load(store_path).map_err(|e| PortError::Io(e.to_string()))?;
    let sealed_package = index
        .lookup(target.id)
        .map(|e| e.sealed_package.clone())
        .unwrap_or_default();

    let mut dispatcher = dispatch::NativeDispatcher::new(index);
    let outcome = dispatcher
        .dispatch(&target, &args, auth)
        .map_err(|e| PortError::Execution(format!("{}", e)))?;

    let mirror = candidate::run_candidate(target.id, &args).ok();
    let mirror_output_hex = mirror.as_ref().map(hex::encode).unwrap_or_default();
    let output_hex = hex::encode(&outcome.output);
    let matches_mirror = outcome.source.is_native() && mirror_output_hex == output_hex;

    Ok(NativeCallReport {
        target: target.id.to_string(),
        source: outcome.source.as_str().to_string(),
        trust: outcome.trust,
        object_hash: outcome.object_hash,
        elf_symbol: outcome.elf_symbol,
        input_hex,
        output_hex,
        matches_mirror,
        mirror_output_hex,
        reason: outcome.reason.to_string(),
        promotion: if outcome.source.is_native() {
            TrustState::Sealed.as_str().to_string()
        } else {
            TrustState::Unknown.as_str().to_string()
        },
        sealed_package,
    })
}

fn default_args(target: &PortTarget) -> Vec<Vec<u8>> {
    match target.id {
        id if id == target::LIBC_TOUPPER.id => alloc::vec![alloc::vec![0x61]],
        id if id == target::LIBC_MEMCMP.id => alloc::vec![
            alloc::vec![0x61, 0x62, 0x63],
            alloc::vec![0x61, 0x62, 0x64],
            3u64.to_le_bytes().to_vec(),
        ],
        id if id == target::LIBC_MEMCHR.id => alloc::vec![
            alloc::vec![0x61, 0x62, 0x63],
            alloc::vec![0x62],
            3u64.to_le_bytes().to_vec(),
        ],
        id if id == target::LIBC_STRLEN.id => alloc::vec![
            // "abc\0" with the terminator inside the bound n = 4 -> length 3.
            alloc::vec![0x61, 0x62, 0x63, 0x00],
            4u64.to_le_bytes().to_vec(),
        ],
        id if id == target::LIBC_STRRCHR.id => alloc::vec![
            // "abc\0" search 'b' -> last occurrence at index 1.
            alloc::vec![0x61, 0x62, 0x63, 0x00],
            alloc::vec![0x62],
            4u64.to_le_bytes().to_vec(),
        ],
        _ => alloc::vec::Vec::new(),
    }
}

/// Run the porting court for `symbol`.
///
/// Deterministic: the case domain is enumerated in a fixed order, the cage is
/// pure black-box observation, and every hash is SHA-256 over a stable
/// canonical encoding. No wall-clock value participates in the verdict.
///
/// From `Replay` depth up, the clean-room candidate is *compiled* with `phorc`
/// and then **executed**: this module loads the emitted ELF64 object, verifies its
/// hash against the seal, locates the ABI entry symbol and replays the corpus
/// through the compiled code. Promotion additionally requires that execution to
/// match the oracle exactly.
pub fn run_port_court(
    symbol: &str,
    auth: &PortingAuthority,
    out_dir: &str,
    depth: PortDepth,
    phorc: Option<&str>,
) -> Result<PortCourtReport, PortError> {
    // Capability gate: observation requires PORTING.
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    let target =
        resolve_target(symbol).ok_or_else(|| PortError::UnknownTarget(symbol.to_string()))?;

    // 1. The target's deterministic case set.
    let cases = target::cases_for(&target);

    // 2. Observe foreign behavior through the cage.
    let traces = observe_target(&target, &cases, auth)?;

    // 3. Seal the observed behavior as a behavior signature.
    let signature = BehaviorSignature::from_traces(&target, &traces);

    // 4. Replay + compare the Rust mirror of the candidate (fail closed).
    let (verdict, mismatches) = replay_court::run_replay_court(&traces);

    // 5. Compiled candidate authority + sealed-object execution (from Replay
    //    depth up): compile the clean-room `.phor` candidate, hash the emitted
    //    object/receipts, then load and execute the sealed object.
    let (compiled, execution) = if depth == PortDepth::Observe {
        (None, None)
    } else {
        let c = compiled::compile_candidate(&target, out_dir, phorc)
            .map_err(|e| PortError::Compile(e.message()))?;
        // The execution court verifies the object hash against the seal before
        // mapping anything, and refuses non-leaf or relocated entry points.
        let (exec_verdict, exec_mismatches) =
            exec::execute_sealed_candidate(&target, &traces, &c.object_path, &c.object_hash, auth)
                .map_err(|e| PortError::Execution(format!("{}", e)))?;
        (Some(c), Some((exec_verdict, exec_mismatches)))
    };

    let artifacts = match &compiled {
        Some(c) => c.artifacts(),
        None => CandidateArtifacts {
            source_hash: String::new(),
            object_hash: String::new(),
            receipt_hash: String::new(),
            compiler_version: String::new(),
        },
    };
    let candidate_sig = CandidateSignature::from_traces(&target, &traces, &artifacts);

    // 6a. Build the prospective sealed store entry — exactly what promotion will
    //     publish — and run the dispatch court over it. This proves the runtime
    //     *prefers* the sealed object at a call site for every case (not merely
    //     that the object executes).
    let dispatch_verdict = match (depth, &execution) {
        (PortDepth::Promote, Some((exec_verdict, _))) => {
            let entry = SealedPortEntry {
                target: target.id.to_string(),
                trust: TrustState::Sealed,
                artifact: SealedArtifact::leaf_object(
                    artifacts.object_hash.clone(),
                    compiled
                        .as_ref()
                        .map(|c| c.object_path.clone())
                        .unwrap_or_default(),
                ),
                oracle_hash: verdict.oracle_hash.clone(),
                candidate_behavior_hash: verdict.candidate_behavior_hash.clone(),
                candidate_source_hash: artifacts.source_hash.clone(),
                sealed_package: "sealed_package.json".to_string(),
            };
            let _ = exec_verdict;
            let mut index = SealedPortIndex::new();
            index.insert(entry);
            Some(dispatch::run_dispatch_court(&target, &traces, &index, auth))
        }
        _ => None,
    };

    // 7. Promote only on a consistent replay, a consistent execution, and a
    //    consistent dispatch (the runtime served every case from the sealed object).
    let mut receipt: Option<PromotionReceipt> = None;
    let mut promotion_json: Option<String> = None;
    let mut sealed_json: Option<String> = None;

    if depth == PortDepth::Promote {
        let (exec_verdict, _) = execution.as_ref().expect("execution runs at Promote depth");
        let (disp_verdict, _) = dispatch_verdict
            .as_ref()
            .expect("dispatch runs at Promote depth");
        let sealed = evidence::sealed_package_json(
            &target,
            &verdict,
            &signature,
            &candidate_sig,
            exec_verdict,
            disp_verdict,
        );
        let promotion_evidence = PromotionEvidence {
            sealed_package_written: true,
            replay_residual_written: true,
        };
        let r = promotion::promote(
            &target,
            &verdict,
            &artifacts,
            exec_verdict,
            disp_verdict,
            &promotion_evidence,
            auth,
        )
        .map_err(PortError::PromotionRefused)?;
        promotion_json = Some(r.to_json());
        sealed_json = Some(sealed);
        receipt = Some(r);
    }

    // 8. Write the evidence set (deterministic; the raw court is reproducible).
    let paths = evidence::write_evidence_set(
        out_dir,
        &target,
        depth,
        &traces,
        &signature,
        &candidate_sig,
        &verdict,
        &mismatches,
        execution.as_ref().map(|(v, m)| (v, m.as_slice())),
        dispatch_verdict.as_ref().map(|(v, m)| (v, m.as_slice())),
        promotion_json.as_deref(),
        sealed_json.as_deref(),
    )
    .map_err(|e| PortError::Io(format!("{}", e)))?;

    let promotion_label = match receipt {
        Some(ref r) => r.to.as_str().to_string(),
        None => match depth {
            PortDepth::Observe => TrustState::Observed.as_str().to_string(),
            PortDepth::Replay => TrustState::OracleCompared.as_str().to_string(),
            PortDepth::Promote => TrustState::Unknown.as_str().to_string(),
        },
    };

    let exec_verdict = execution.map(|(v, _)| v);
    let disp_verdict = dispatch_verdict.map(|(v, _)| v);

    Ok(PortCourtReport {
        symbol: symbol.to_string(),
        target: target.id.to_string(),
        dialect: target.dialect.to_string(),
        version: target.version.to_string(),
        locale_contract: target.locale_contract.to_string(),
        observed_cases: traces.len() as u64,
        replay_cases: verdict.cases_run,
        passed: verdict.cases_passed,
        failed: verdict.cases_failed,
        oracle_hash: verdict.oracle_hash.clone(),
        candidate_behavior_hash: verdict.candidate_behavior_hash.clone(),
        candidate_source_hash: artifacts.source_hash.clone(),
        candidate_object_hash: artifacts.object_hash.clone(),
        candidate_receipt_hash: artifacts.receipt_hash.clone(),
        compiler_version: artifacts.compiler_version.clone(),
        candidate_object_path: compiled
            .as_ref()
            .map(|c| c.object_path.clone())
            .unwrap_or_default(),
        verdict: verdict.verdict.as_str().to_string(),
        promotion: promotion_label,
        sealed_package: paths.sealed_package.clone(),
        evidence_dir: paths.dir.clone(),
        abi_symbol: exec_verdict
            .as_ref()
            .map(|v| v.abi_symbol.clone())
            .unwrap_or_default(),
        elf_symbol: exec_verdict
            .as_ref()
            .map(|v| v.elf_symbol.clone())
            .unwrap_or_default(),
        execution_cases: exec_verdict.as_ref().map(|v| v.cases_run).unwrap_or(0),
        execution_passed: exec_verdict.as_ref().map(|v| v.cases_passed).unwrap_or(0),
        execution_failed: exec_verdict.as_ref().map(|v| v.cases_failed).unwrap_or(0),
        execution_hash: exec_verdict
            .as_ref()
            .map(|v| v.execution_hash.clone())
            .unwrap_or_default(),
        execution_verdict: exec_verdict
            .as_ref()
            .map(|v| v.verdict.as_str().to_string())
            .unwrap_or_default(),
        dispatch_cases: disp_verdict.as_ref().map(|v| v.cases_run).unwrap_or(0),
        dispatch_native_cases: disp_verdict.as_ref().map(|v| v.native_cases).unwrap_or(0),
        dispatch_fallback_cases: disp_verdict.as_ref().map(|v| v.fallback_cases).unwrap_or(0),
        dispatch_broken_seal_cases: disp_verdict
            .as_ref()
            .map(|v| v.broken_seal_cases)
            .unwrap_or(0),
        dispatch_hash: disp_verdict
            .as_ref()
            .map(|v| v.dispatch_hash.clone())
            .unwrap_or_default(),
        dispatch_verdict: disp_verdict
            .as_ref()
            .map(|v| v.verdict.as_str().to_string())
            .unwrap_or_default(),
    })
}

/// Where a composition court's sealed index comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexSource {
    /// **Derive** the seal: compile each leaf with `phorc` and replay any nested
    /// composition's court to obtain its chain hash. This is the court's
    /// independent derivation, and the only path that needs a compiler.
    Derived,
    /// **Load** the seal from the committed persistent store: no compiler
    /// invocation and no oracle replay. Every entry is verified as it loads.
    Persistent,
}

/// Which sealed composition court to run.
///
/// A composition is a qualified target built entirely from already-sealed leaf
/// ports, so it adds no new trusted code. The two differ in *shape*, not just in
/// stages: the first chains a map stage into a search stage, the second also feeds
/// a middle stage's **result** into the next stage's **argument**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionKind {
    /// `toupper ∘ memchr` — fold the haystack and needle, then search.
    ToupperMemchr,
    /// `toupper ∘ strlen ∘ memchr` — fold, derive the bound with `strlen`, search
    /// within that derived bound.
    ToupperStrlenMemchr,
    /// `toupper ∘ strlen ∘ memchr ∘ toupper ∘ memchr` — one derived bound consumed
    /// by two searches, the second non-adjacent to the stage that produced it.
    ToupperStrlenMemchrPair,
    /// `toupper_each` — the buffer-to-buffer map composition, which is also usable as
    /// a nested stage of another chain.
    ToupperEach,
    /// `toupper_each ∘ strlen ∘ memchr` — a chain whose fold stage is the sealed
    /// **composition** `toupper_each`, resolved from the store and dispatched
    /// recursively.
    ToupperEachStrlenMemchr,
    /// `toupper ∘ memchr ∘ slice ∘ memchr` — the first chain where a derived value
    /// selects a **buffer**: the folded haystack is sliced at the origin the first
    /// `memchr` derived, and the second `memchr` searches that slice.
    ToupperMemchrSuffix,
    /// `toupper_each ∘ memchr ∘ slice ∘ toupper_memchr` — the first chain where a
    /// sealed **composition consumes a buffer another composition selected**: the
    /// nested `toupper_each` produces the folded view, a leaf derives the origin in
    /// it, and the nested `toupper_memchr` consumes the derived slice — folding it
    /// itself, because the slice is taken from the *unfolded* haystack.
    ToupperEachSliceSearch,
}

const TOUPPER_MEMCHR_LEAVES: [PortTarget; 2] = [target::LIBC_TOUPPER, target::LIBC_MEMCHR];
const TOUPPER_STRLEN_MEMCHR_LEAVES: [PortTarget; 3] = [
    target::LIBC_TOUPPER,
    target::LIBC_STRLEN,
    target::LIBC_MEMCHR,
];
const TOUPPER_STRLEN_MEMCHR_PAIR_LEAVES: [PortTarget; 3] = [
    target::LIBC_TOUPPER,
    target::LIBC_STRLEN,
    target::LIBC_MEMCHR,
];
const TOUPPER_EACH_LEAVES: [PortTarget; 1] = [target::LIBC_TOUPPER];
/// The nested chain dispatches `strlen` and `memchr` directly, and needs the
/// `toupper` leaf present because the nested `toupper_each` port requires it.
const TOUPPER_EACH_STRLEN_MEMCHR_LEAVES: [PortTarget; 3] = [
    target::LIBC_TOUPPER,
    target::LIBC_STRLEN,
    target::LIBC_MEMCHR,
];
/// The slice chain dispatches `toupper` (the fold) and `memchr` (both searches).
const TOUPPER_MEMCHR_SUFFIX_LEAVES: [PortTarget; 2] = [target::LIBC_TOUPPER, target::LIBC_MEMCHR];
/// The slice-search chain dispatches the leaf `memchr` directly (the origin search)
/// and delegates its folds and its suffix search to nested compositions, which
/// resolve the `toupper` and `memchr` leaves through the same index.
const TOUPPER_EACH_SLICE_SEARCH_LEAVES: [PortTarget; 2] =
    [target::LIBC_TOUPPER, target::LIBC_MEMCHR];

/// The nested ports `toupper_each_strlen_memchr` dispatches as stages.
const NESTED_OF_TOUPPER_EACH_STRLEN_MEMCHR: [CompositionTarget; 1] =
    [composition_toupper_each::COMPOSITION_TOUPPER_EACH];

/// The nested ports `toupper_each_slice_search` dispatches as stages: the fold that
/// produces the buffer, and the composition that consumes the derived slice.
const NESTED_OF_TOUPPER_EACH_SLICE_SEARCH: [CompositionTarget; 2] = [
    composition_toupper_each::COMPOSITION_TOUPPER_EACH,
    composition::COMPOSITION_TOUPPER_MEMCHR,
];

impl CompositionKind {
    /// Parse a short name or a qualified composition id.
    pub fn parse(name: &str) -> Option<Self> {
        if name == "toupper_memchr" || name == composition::COMPOSITION_TOUPPER_MEMCHR.id {
            return Some(CompositionKind::ToupperMemchr);
        }
        if name == "toupper_strlen_memchr"
            || name == composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR.id
        {
            return Some(CompositionKind::ToupperStrlenMemchr);
        }
        if name == "toupper_strlen_memchr_pair"
            || name == composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id
        {
            return Some(CompositionKind::ToupperStrlenMemchrPair);
        }
        if name == "toupper_each" || name == composition_toupper_each::COMPOSITION_TOUPPER_EACH.id {
            return Some(CompositionKind::ToupperEach);
        }
        if name == "toupper_each_strlen_memchr"
            || name == composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id
        {
            return Some(CompositionKind::ToupperEachStrlenMemchr);
        }
        if name == "toupper_memchr_suffix"
            || name == composition_suffix::COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id
        {
            return Some(CompositionKind::ToupperMemchrSuffix);
        }
        if name == "toupper_each_slice_search"
            || name == composition_slice_search::COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id
        {
            return Some(CompositionKind::ToupperEachSliceSearch);
        }
        None
    }

    pub fn name(&self) -> &'static str {
        match self {
            CompositionKind::ToupperMemchr => "toupper_memchr",
            CompositionKind::ToupperStrlenMemchr => "toupper_strlen_memchr",
            CompositionKind::ToupperStrlenMemchrPair => "toupper_strlen_memchr_pair",
            CompositionKind::ToupperEach => "toupper_each",
            CompositionKind::ToupperEachStrlenMemchr => "toupper_each_strlen_memchr",
            CompositionKind::ToupperMemchrSuffix => "toupper_memchr_suffix",
            CompositionKind::ToupperEachSliceSearch => "toupper_each_slice_search",
        }
    }

    pub fn target_id(&self) -> &'static str {
        match self {
            CompositionKind::ToupperMemchr => composition::COMPOSITION_TOUPPER_MEMCHR.id,
            CompositionKind::ToupperStrlenMemchr => {
                composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR.id
            }
            CompositionKind::ToupperStrlenMemchrPair => {
                composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id
            }
            CompositionKind::ToupperEach => composition_toupper_each::COMPOSITION_TOUPPER_EACH.id,
            CompositionKind::ToupperEachStrlenMemchr => {
                composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id
            }
            CompositionKind::ToupperMemchrSuffix => {
                composition_suffix::COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id
            }
            CompositionKind::ToupperEachSliceSearch => {
                composition_slice_search::COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id
            }
        }
    }

    /// The default evidence directory for this composition.
    pub fn evidence_dir(&self) -> &'static str {
        match self {
            CompositionKind::ToupperMemchr => "phost/evidence/composition/toupper_memchr",
            CompositionKind::ToupperStrlenMemchr => {
                "phost/evidence/composition/toupper_strlen_memchr"
            }
            CompositionKind::ToupperStrlenMemchrPair => {
                "phost/evidence/composition/toupper_strlen_memchr_pair"
            }
            CompositionKind::ToupperEach => "phost/evidence/composition/toupper_each",
            CompositionKind::ToupperEachStrlenMemchr => {
                "phost/evidence/composition/toupper_each_strlen_memchr"
            }
            CompositionKind::ToupperMemchrSuffix => {
                "phost/evidence/composition/toupper_memchr_suffix"
            }
            CompositionKind::ToupperEachSliceSearch => {
                "phost/evidence/composition/toupper_each_slice_search"
            }
        }
    }

    fn leaf_targets(&self) -> &'static [PortTarget] {
        match self {
            CompositionKind::ToupperMemchr => &TOUPPER_MEMCHR_LEAVES,
            CompositionKind::ToupperStrlenMemchr => &TOUPPER_STRLEN_MEMCHR_LEAVES,
            CompositionKind::ToupperStrlenMemchrPair => &TOUPPER_STRLEN_MEMCHR_PAIR_LEAVES,
            CompositionKind::ToupperEach => &TOUPPER_EACH_LEAVES,
            CompositionKind::ToupperEachStrlenMemchr => &TOUPPER_EACH_STRLEN_MEMCHR_LEAVES,
            CompositionKind::ToupperMemchrSuffix => &TOUPPER_MEMCHR_SUFFIX_LEAVES,
            CompositionKind::ToupperEachSliceSearch => &TOUPPER_EACH_SLICE_SEARCH_LEAVES,
        }
    }

    /// Compositions this chain dispatches as nested stages.
    fn nested_compositions(&self) -> &'static [CompositionTarget] {
        match self {
            CompositionKind::ToupperEachStrlenMemchr => &NESTED_OF_TOUPPER_EACH_STRLEN_MEMCHR,
            CompositionKind::ToupperEachSliceSearch => &NESTED_OF_TOUPPER_EACH_SLICE_SEARCH,
            _ => &[],
        }
    }
}

/// Build the sealed store index a composition dispatches through.
///
/// With [`IndexSource::Derived`], the leaf ports are compiled fresh
/// (deterministically) and marked sealed, and any composition dispatched as a
/// **nested stage** is sealed with the chain hash its own court deterministically
/// produces — the same way a leaf's object hash is re-derived from source rather
/// than trusted.
///
/// With [`IndexSource::Persistent`], the seal is **loaded** from the committed
/// store instead: no `phorc`, no nested replay. The store is verified as it loads,
/// and every port the chain needs must be present, or this fails closed.
///
/// A composition adds no new trusted code either way: its implementation *is*
/// these already-sealed ports.
fn build_composition_index(
    kind: CompositionKind,
    auth: &PortingAuthority,
    phorc: Option<&str>,
    source: IndexSource,
) -> Result<SealedPortIndex, PortError> {
    if source == IndexSource::Persistent {
        return load_persistent_index(kind);
    }

    let mut index = SealedPortIndex::new();
    for target in kind.leaf_targets() {
        let dir = format!("phost/evidence/porting/{}", target.symbol);
        let c = compiled::compile_candidate(target, &dir, phorc)
            .map_err(|e| PortError::Compile(e.message()))?;
        index.insert(SealedPortEntry {
            target: target.id.to_string(),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::leaf_object(c.object_hash, c.object_path),
            oracle_hash: String::new(),
            candidate_behavior_hash: String::new(),
            candidate_source_hash: c.source_hash,
            sealed_package: format!("{}/sealed_package.json", dir),
        });
    }

    // Nested composition ports. Each is sealed with the chain hash its own court
    // derives, so a nested dispatch is checked against a real seal.
    for nested in kind.nested_compositions() {
        let chain_hash = nested_composition_chain_hash(nested.id, auth)?;
        index.insert(SealedPortEntry {
            target: nested.id.to_string(),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::composition(
                nested.id.to_string(),
                chain_hash,
                nested.stages.iter().map(|s| (*s).to_string()).collect(),
            ),
            oracle_hash: String::new(),
            candidate_behavior_hash: String::new(),
            candidate_source_hash: String::new(),
            sealed_package: format!(
                "{}/composition_verdict.json",
                nested_evidence_dir(nested.id)
            ),
        });
    }

    Ok(index)
}

/// Load a composition's sealed index from the committed persistent store.
///
/// No compiler is invoked and no oracle is replayed. The store is verified as it
/// loads (object bytes hashed against their seals, stages resolved), and every
/// port this chain dispatches must be present and sealed — otherwise the chain
/// would silently lose a stage, so it fails closed.
fn load_persistent_index(kind: CompositionKind) -> Result<SealedPortIndex, PortError> {
    let index = store::load_default().map_err(|e| PortError::Io(e.to_string()))?;

    let mut required: Vec<&str> = kind.leaf_targets().iter().map(|t| t.id).collect();
    for nested in kind.nested_compositions() {
        required.push(nested.id);
    }
    for id in required {
        let sealed = index
            .lookup(id)
            .map(|e| e.trust == TrustState::Sealed)
            .unwrap_or(false);
        if !sealed {
            return Err(PortError::Execution(format!(
                "persistent store has no sealed port {}",
                id
            )));
        }
    }
    Ok(index)
}

/// The committed evidence directory for a composition id.
fn nested_evidence_dir(composition_id: &str) -> String {
    match CompositionKind::parse(composition_id) {
        Some(k) => k.evidence_dir().to_string(),
        None => String::from("phost/evidence/composition"),
    }
}

/// Derive a nested composition's chain hash by replaying its own court.
///
/// This is the composition analogue of compiling a leaf candidate fresh: the seal is
/// re-derived deterministically from source and oracle rather than trusted, and the
/// outer verifier cross-checks the recorded hash against the committed verdict.
fn nested_composition_chain_hash(
    composition_id: &str,
    auth: &PortingAuthority,
) -> Result<String, PortError> {
    let kind = CompositionKind::parse(composition_id)
        .ok_or_else(|| PortError::UnknownTarget(composition_id.to_string()))?;
    // A nested port's own chain must not itself nest (kept simple and acyclic).
    let index = build_composition_index(kind, auth, None, IndexSource::Derived)?;
    match kind {
        CompositionKind::ToupperEach => {
            let cases = composition_toupper_each::composition_corpus();
            let traces = dialect_cage::observe_composition_toupper_each(&cases, auth)?;
            let (v, _) = composition_toupper_each::run_composition_court(&traces, &index, auth);
            if !v.is_sealed_eligible() {
                return Err(PortError::Execution(format!(
                    "nested composition {} is not internally consistent",
                    composition_id
                )));
            }
            Ok(v.chain_hash)
        }
        CompositionKind::ToupperMemchr => {
            let cases = composition::composition_corpus();
            let traces = dialect_cage::observe_composition(&cases, auth)?;
            let (v, _) = composition::run_composition_court(&traces, &index, auth);
            if !v.is_sealed_eligible() {
                return Err(PortError::Execution(format!(
                    "nested composition {} is not internally consistent",
                    composition_id
                )));
            }
            Ok(v.chain_hash)
        }
        other => Err(PortError::UnsupportedTarget(other.target_id().to_string())),
    }
}

/// One chain link, as reported to the CLI.
#[derive(Clone, Debug)]
pub struct StageReport {
    pub label: String,
    /// The qualified leaf id this link dispatches to.
    pub leaf: String,
    pub native_cases: u64,
    pub object_hash: String,
    pub elf_symbol: String,
}

/// Outcome of a composition court run, suitable for printing.
#[derive(Clone, Debug)]
pub struct CompositionReport {
    pub target: String,
    pub stages: Vec<String>,
    pub cases_run: u64,
    pub stage_reports: Vec<StageReport>,
    /// Free-form notes the CLI prints after the stage table (e.g. a stage that is
    /// data-dependently skipped).
    pub notes: Vec<String>,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    pub chain_hash: String,
    pub verdict: String,
    pub sealed: bool,
    pub evidence_dir: String,
}

/// Run a Sealed Composition Dispatch Court.
///
/// The oracle comes from the foreign runtime (the cage); the implementation is the
/// chain of sealed objects executed through `NativeDispatcher` — no Rust mirror is
/// consulted. Both compositions write the same evidence set into their own
/// directory, so composition #1's committed evidence is untouched.
pub fn run_composition_court(
    kind: CompositionKind,
    auth: &PortingAuthority,
    out_dir: &str,
    phorc: Option<&str>,
    source: IndexSource,
) -> Result<CompositionReport, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }

    let index = build_composition_index(kind, auth, phorc, source)?;

    match kind {
        CompositionKind::ToupperMemchr => {
            let cases = composition::composition_corpus();
            let traces = dialect_cage::observe_composition(&cases, auth)?;
            let (v, m) = composition::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("toupper(haystack)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_hay_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("toupper(needle)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_needle_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                ],
                notes: alloc::vec::Vec::new(),
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperStrlenMemchr => {
            let cases = composition_strlen_memchr::composition_corpus();
            let traces = dialect_cage::observe_composition_strlen_memchr(&cases, auth)?;
            let (v, m) = composition_strlen_memchr::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("toupper(haystack)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_hay_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("strlen(derived bound)"),
                        leaf: target::LIBC_STRLEN.id.to_string(),
                        native_cases: v.strlen_native_cases,
                        object_hash: v.strlen_object_hash.clone(),
                        elf_symbol: v.strlen_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("toupper(needle)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_needle_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                ],
                notes: alloc::vec::Vec::new(),
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperStrlenMemchrPair => {
            let cases = composition_pair::composition_corpus();
            let traces = dialect_cage::observe_composition_pair(&cases, auth)?;
            let (v, m) = composition_pair::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("toupper(haystack)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_hay_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("strlen(derived bound)"),
                        leaf: target::LIBC_STRLEN.id.to_string(),
                        native_cases: v.strlen_native_cases,
                        object_hash: v.strlen_object_hash.clone(),
                        elf_symbol: v.strlen_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("toupper(needle A)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_needle_a_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr(needle A)"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_a_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("toupper(needle B)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_needle_b_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr(needle B)"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_b_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                ],
                notes: alloc::vec::Vec::new(),
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperEach => {
            let cases = composition_toupper_each::composition_corpus();
            let traces = dialect_cage::observe_composition_toupper_each(&cases, auth)?;
            let (v, m) = composition_toupper_each::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![StageReport {
                    label: String::from("toupper(each byte)"),
                    leaf: target::LIBC_TOUPPER.id.to_string(),
                    native_cases: v.toupper_native_cases,
                    object_hash: v.toupper_object_hash.clone(),
                    elf_symbol: v.toupper_elf_symbol.clone(),
                }],
                notes: alloc::vec::Vec::new(),
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperEachStrlenMemchr => {
            let cases = composition_nested::nested_corpus();
            let traces = dialect_cage::observe_composition_nested(&cases, auth)?;
            let (v, m) = composition_nested::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("fold(haystack) [composition]"),
                        leaf: v.fold_composition_id.clone(),
                        native_cases: v.fold_hay_native_cases,
                        object_hash: v.fold_composition_chain_hash.clone(),
                        elf_symbol: format!("compose:{}", v.fold_composition_id),
                    },
                    StageReport {
                        label: String::from("strlen(derived bound)"),
                        leaf: target::LIBC_STRLEN.id.to_string(),
                        native_cases: v.strlen_native_cases,
                        object_hash: v.strlen_object_hash.clone(),
                        elf_symbol: v.strlen_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("fold(needle) [composition]"),
                        leaf: v.fold_composition_id.clone(),
                        native_cases: v.fold_needle_native_cases,
                        object_hash: v.fold_composition_chain_hash.clone(),
                        elf_symbol: format!("compose:{}", v.fold_composition_id),
                    },
                    StageReport {
                        label: String::from("memchr"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                ],
                notes: alloc::vec::Vec::new(),
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperMemchrSuffix => {
            let cases = composition_suffix::composition_corpus();
            let traces = dialect_cage::observe_composition_suffix(&cases, auth)?;
            let (v, m) = composition_suffix::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("toupper(haystack)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_hay_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        // Both needles are folded before the origin is derived.
                        label: String::from("toupper(needles A+B)"),
                        leaf: target::LIBC_TOUPPER.id.to_string(),
                        native_cases: v.toupper_needle_a_native_cases
                            + v.toupper_needle_b_native_cases,
                        object_hash: v.toupper_object_hash.clone(),
                        elf_symbol: v.toupper_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr(origin)"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_a_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                    StageReport {
                        label: String::from("memchr(suffix)"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_b_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                ],
                notes: alloc::vec![format!(
                    "memchr(suffix) is data-dependent: it did not run on {} of {} cases (needleA absent, so no origin and no slice)",
                    v.memchr_b_not_reached_cases, v.cases_run
                )],
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
        CompositionKind::ToupperEachSliceSearch => {
            let cases = composition_slice_search::composition_corpus();
            let traces = dialect_cage::observe_composition_suffix(&cases, auth)?;
            let (v, m) = composition_slice_search::run_composition_court(&traces, &index, auth);
            let evidence_dir = evidence::write_composition_evidence(out_dir, &traces, &v, &m)
                .map_err(|e| PortError::Io(format!("{}", e)))?;

            Ok(CompositionReport {
                target: v.target.clone(),
                stages: v.stages.clone(),
                cases_run: v.cases_run,
                stage_reports: alloc::vec![
                    StageReport {
                        label: String::from("fold(haystack) [composition]"),
                        leaf: v.fold_composition_id.clone(),
                        native_cases: v.fold_hay_native_cases,
                        object_hash: v.fold_composition_chain_hash.clone(),
                        elf_symbol: format!("compose:{}", v.fold_composition_id),
                    },
                    StageReport {
                        label: String::from("fold(needle A) [composition]"),
                        leaf: v.fold_composition_id.clone(),
                        native_cases: v.fold_needle_a_native_cases,
                        object_hash: v.fold_composition_chain_hash.clone(),
                        elf_symbol: format!("compose:{}", v.fold_composition_id),
                    },
                    StageReport {
                        label: String::from("memchr(origin)"),
                        leaf: target::LIBC_MEMCHR.id.to_string(),
                        native_cases: v.memchr_a_native_cases,
                        object_hash: v.memchr_object_hash.clone(),
                        elf_symbol: v.memchr_elf_symbol.clone(),
                    },
                    StageReport {
                        // The derived slice is consumed by a sealed *composition*,
                        // which folds it and searches it — the outer runner holds no
                        // fold logic here.
                        label: String::from("search(slice) [composition]"),
                        leaf: v.search_composition_id.clone(),
                        native_cases: v.search_native_cases,
                        object_hash: v.search_composition_chain_hash.clone(),
                        elf_symbol: format!("compose:{}", v.search_composition_id),
                    },
                ],
                notes: alloc::vec![format!(
                    "search(slice) is data-dependent: it did not run on {} of {} cases (needleA absent, so no origin and no slice)",
                    v.search_not_reached_cases, v.cases_run
                )],
                fallback_cases: v.fallback_cases,
                broken_seal_cases: v.broken_seal_cases,
                cases_passed: v.cases_passed,
                cases_failed: v.cases_failed,
                dispatches_run: v.dispatches_run,
                oracle_hash: v.oracle_hash.clone(),
                chain_hash: v.chain_hash.clone(),
                verdict: v.verdict.as_str().to_string(),
                sealed: v.is_sealed_eligible(),
                evidence_dir,
            })
        }
    }
}

/// Run the **generic** (Phase 2) Sealed Composition Dispatch Court for a named
/// composition.
///
/// This is the same court as [`run_composition_court`], but it is driven entirely
/// by the composition's `CompositionIR`: the oracle and the implementation are the
/// *same* graph evaluated through the foreign and sealed backends. It writes the
/// **v2** evidence schema (four identities), in its own evidence directory, so the
/// historical v1 evidence is never touched.
pub fn run_ir_composition_court(
    name: &str,
    auth: &PortingAuthority,
    out_dir: &str,
    phorc: Option<&str>,
    source: IndexSource,
) -> Result<CompositionReport, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }
    let def = composition_registry::by_name(name)
        .ok_or_else(|| PortError::UnknownTarget(name.to_string()))?;

    // The seal index is derived the same way the legacy court derives it
    // (deterministic compile of every leaf; a nested composition sealed with its
    // own committed chain hash), or loaded from the persistent store.
    let kind = CompositionKind::parse(def.target.id)
        .ok_or_else(|| PortError::UnknownTarget(def.target.id.to_string()))?;
    let index = build_composition_index(kind, auth, phorc, source)?;

    let cases = (def.cases)();
    let traces = (def.observe)(&cases, auth)?;
    let (v, mismatches) = composition_engine::run_ir_court(def, &traces, &index, auth);
    let evidence_dir = evidence::write_ir_composition_evidence(out_dir, &traces, &v, &mismatches)
        .map_err(|e| PortError::Io(format!("{}", e)))?;

    let stage_reports: Vec<StageReport> = v
        .stages
        .iter()
        .map(|s| StageReport {
            label: format!("{}#{}", s.port, s.node),
            leaf: s.port.clone(),
            native_cases: s.native_cases,
            object_hash: s.seal.clone(),
            elf_symbol: s.symbol.clone(),
        })
        .collect();

    let mut notes: Vec<String> = Vec::new();
    for s in &v.stages {
        if s.not_reached_cases > 0 {
            notes.push(format!(
                "{} is data-dependent: it did not run on {} of {} cases",
                s.port, s.not_reached_cases, v.cases_run
            ));
        }
    }
    notes.push(format!("composition_ir_hash={}", v.composition_ir_hash));
    notes.push(format!(
        "dependency_binding_hash={}",
        v.dependency_binding_hash
    ));
    notes.push(format!("behavior_hash={}", v.behavior_hash));
    notes.push(format!(
        "composition_artifact_hash={}",
        v.composition_artifact_hash
    ));

    Ok(CompositionReport {
        target: v.target.clone(),
        stages: def.target.stages.iter().map(|s| (*s).to_string()).collect(),
        cases_run: v.cases_run,
        stage_reports,
        notes,
        fallback_cases: v.fallback_cases,
        broken_seal_cases: v.broken_seal_cases,
        cases_passed: v.cases_passed,
        cases_failed: v.cases_failed,
        dispatches_run: v.dispatches_run,
        oracle_hash: v.oracle_hash.clone(),
        chain_hash: v.composition_artifact_hash.clone(),
        verdict: v.verdict.as_str().to_string(),
        sealed: v.is_sealed_eligible(),
        evidence_dir,
    })
}

/// The committed v2 evidence directory for a composition name.
pub fn ir_evidence_dir(name: &str) -> String {
    format!("phost/evidence/composition_ir/{}", name)
}

/// Run the **court-sensitivity** court (Phase 3) for a leaf symbol or a
/// composition name, and write its evidence.
///
/// This asks whether the court can actually *see* the declared defect families,
/// over the same corpus and oracle the real court uses. A leaf is observed through
/// the cage; a composition is challenged against the persistent store.
pub fn run_challenge_court(
    name: &str,
    auth: &PortingAuthority,
    out_dir: &str,
) -> Result<challenge::ChallengeReport, PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }
    let report = if let Some(target) = target::resolve_target(name) {
        challenge::run_leaf_challenge(&target, auth)?
    } else if composition_registry::by_name(name).is_some() {
        let index = store::load_default().map_err(|e| PortError::Io(e.to_string()))?;
        challenge::run_composition_challenge(name, &index, auth)?
    } else {
        return Err(PortError::UnknownTarget(name.to_string()));
    };
    evidence::write_challenge_evidence(out_dir, &report)
        .map_err(|e| PortError::Io(format!("{}", e)))?;
    Ok(report)
}

/// The committed challenge-evidence directory for a target name.
pub fn challenge_evidence_dir(name: &str) -> String {
    format!("phost/evidence/challenge/{}", name)
}

/// One composed call, as reported to the CLI.
#[derive(Clone, Debug)]
pub struct CompositionCallReport {
    pub target: String,
    /// `(label, stage status)` per link, in chain order.
    pub stages: Vec<(String, String)>,
    pub hay_norm: Vec<u8>,
    /// The bound a `strlen` stage derived, when the chain has one.
    pub derived_len: Option<usize>,
    /// The buffer a derived origin selected for a composition to consume, when the
    /// chain has one.
    pub slice: Option<Vec<u8>>,
    /// `(label, folded needle)` per needle the chain folds.
    pub needles: Vec<(String, Option<u8>)>,
    /// `(label, index)` per search result the chain produces.
    pub indexes: Vec<(String, Option<i32>)>,
    pub dispatches: u64,
}

/// Run one composed call for the CLI.
///
/// `needle_b` is used only by the pair composition; the single-needle chains ignore
/// it, so the CLI can pass a neutral placeholder.
pub fn run_composition_call(
    kind: CompositionKind,
    hay: &[u8],
    needle_a: u8,
    needle_b: u8,
    n: usize,
    auth: &PortingAuthority,
) -> Result<CompositionCallReport, PortError> {
    // The runtime call site loads the committed seal: no compiler, no replay.
    // Without `PORTING` the store is not read at all and every stage falls back.
    let index = if auth.can_observe() {
        store::load_default().map_err(|e| PortError::Io(e.to_string()))?
    } else {
        SealedPortIndex::new()
    };

    match kind {
        CompositionKind::ToupperMemchr => {
            let c = composition::run_composition_call(&index, hay, needle_a, n, auth);
            Ok(CompositionCallReport {
                target: composition::COMPOSITION_TOUPPER_MEMCHR.id.to_string(),
                stages: alloc::vec![
                    (String::from("toupper(haystack)"), String::from(c.hay_stage)),
                    (
                        String::from("toupper(needle)"),
                        String::from(c.needle_stage)
                    ),
                    (String::from("memchr"), String::from(c.memchr_stage)),
                ],
                hay_norm: c.hay_norm,
                derived_len: None,
                slice: None,
                needles: alloc::vec![(String::from("Needle"), c.needle_norm)],
                indexes: alloc::vec![(String::from("Index"), c.index)],
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperStrlenMemchr => {
            let c = composition_strlen_memchr::run_composition_call(&index, hay, needle_a, n, auth);
            Ok(CompositionCallReport {
                target: composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR
                    .id
                    .to_string(),
                stages: alloc::vec![
                    (String::from("toupper(haystack)"), String::from(c.hay_stage)),
                    (
                        String::from("strlen(derived bound)"),
                        String::from(c.strlen_stage)
                    ),
                    (
                        String::from("toupper(needle)"),
                        String::from(c.needle_stage)
                    ),
                    (String::from("memchr"), String::from(c.memchr_stage)),
                ],
                hay_norm: c.hay_norm,
                derived_len: c.derived_len,
                slice: None,
                needles: alloc::vec![(String::from("Needle"), c.needle_norm)],
                indexes: alloc::vec![(String::from("Index"), c.index)],
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperStrlenMemchrPair => {
            let c =
                composition_pair::run_composition_call(&index, hay, needle_a, needle_b, n, auth);
            Ok(CompositionCallReport {
                target: composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR
                    .id
                    .to_string(),
                stages: alloc::vec![
                    (String::from("toupper(haystack)"), String::from(c.hay_stage)),
                    (
                        String::from("strlen(derived bound)"),
                        String::from(c.strlen_stage)
                    ),
                    (
                        String::from("toupper(needle A)"),
                        String::from(c.needle_a_stage)
                    ),
                    (
                        String::from("memchr(needle A)"),
                        String::from(c.memchr_a_stage)
                    ),
                    (
                        String::from("toupper(needle B)"),
                        String::from(c.needle_b_stage)
                    ),
                    (
                        String::from("memchr(needle B)"),
                        String::from(c.memchr_b_stage)
                    ),
                ],
                hay_norm: c.hay_norm,
                derived_len: c.derived_len,
                slice: None,
                needles: alloc::vec![
                    (String::from("Needle A"), c.needle_a_norm),
                    (String::from("Needle B"), c.needle_b_norm),
                ],
                indexes: alloc::vec![
                    (String::from("Index A"), c.index_a),
                    (String::from("Index B"), c.index_b),
                ],
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperEach => {
            let c = composition_toupper_each::run_composition_call(&index, hay, n, auth);
            Ok(CompositionCallReport {
                target: composition_toupper_each::COMPOSITION_TOUPPER_EACH
                    .id
                    .to_string(),
                stages: alloc::vec![(String::from("toupper(each byte)"), String::from(c.stage))],
                hay_norm: c.folded,
                derived_len: None,
                slice: None,
                needles: alloc::vec::Vec::new(),
                indexes: alloc::vec::Vec::new(),
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperEachStrlenMemchr => {
            let c = composition_nested::run_composition_call(&index, hay, needle_a, n, auth);
            Ok(CompositionCallReport {
                target: composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR
                    .id
                    .to_string(),
                stages: alloc::vec![
                    (
                        String::from("fold(haystack) [composition]"),
                        String::from(c.fold_hay_stage)
                    ),
                    (
                        String::from("strlen(derived bound)"),
                        String::from(c.strlen_stage)
                    ),
                    (
                        String::from("fold(needle) [composition]"),
                        String::from(c.fold_needle_stage)
                    ),
                    (String::from("memchr"), String::from(c.memchr_stage)),
                ],
                hay_norm: c.hay_norm,
                derived_len: c.derived_len,
                slice: None,
                needles: alloc::vec![(String::from("Needle"), c.needle_norm)],
                indexes: alloc::vec![(String::from("Index"), c.index)],
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperMemchrSuffix => {
            let c =
                composition_suffix::run_composition_call(&index, hay, needle_a, needle_b, n, auth);
            Ok(CompositionCallReport {
                target: composition_suffix::COMPOSITION_TOUPPER_MEMCHR_SUFFIX
                    .id
                    .to_string(),
                stages: alloc::vec![
                    (String::from("toupper(haystack)"), String::from(c.hay_stage)),
                    (
                        String::from("memchr(origin)"),
                        String::from(c.memchr_a_stage)
                    ),
                    // "-" when needleA was absent, so no slice existed and the
                    // second search was never dispatched.
                    (
                        String::from("memchr(suffix)"),
                        String::from(c.memchr_b_stage)
                    ),
                ],
                hay_norm: c.hay_norm,
                derived_len: None,
                slice: None,
                needles: alloc::vec![
                    (String::from("Needle A"), c.needle_a_norm),
                    (String::from("Needle B"), c.needle_b_norm),
                ],
                indexes: alloc::vec![
                    (String::from("Origin"), c.origin),
                    (String::from("Suffix offset"), c.suffix_index),
                    (String::from("Index"), c.index),
                ],
                dispatches: c.dispatches,
            })
        }
        CompositionKind::ToupperEachSliceSearch => {
            let c = composition_slice_search::run_composition_call(
                &index, hay, needle_a, needle_b, n, auth,
            );
            Ok(CompositionCallReport {
                target: composition_slice_search::COMPOSITION_TOUPPER_EACH_SLICE_SEARCH
                    .id
                    .to_string(),
                stages: alloc::vec![
                    (
                        String::from("fold(haystack) [composition]"),
                        String::from(c.fold_hay_stage)
                    ),
                    (
                        String::from("memchr(origin)"),
                        String::from(c.memchr_a_stage)
                    ),
                    // "-" when needleA was absent, so no slice existed and the
                    // consumer composition was never dispatched.
                    (
                        String::from("search(slice) [composition]"),
                        String::from(c.search_stage)
                    ),
                ],
                hay_norm: c.hay_norm,
                derived_len: None,
                slice: Some(c.slice),
                needles: alloc::vec![(String::from("Needle A"), c.needle_a_norm)],
                indexes: alloc::vec![
                    (String::from("Origin"), c.origin),
                    (String::from("Suffix offset"), c.suffix_index),
                    (String::from("Index"), c.index),
                ],
                dispatches: c.dispatches,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn traces_for_domain() -> Vec<OracleTrace> {
        let target = target::LIBC_TOUPPER;
        let cases = target::byte_domain_cases();
        dialect_cage::observe_target(&target, &cases, &PortingAuthority::granted()).unwrap()
    }

    #[test]
    fn test_complete_input_domain() {
        let cases = target::byte_domain_cases();
        assert_eq!(cases.len(), 256);
        assert_eq!(cases[0].case_id, "0x00");
        assert_eq!(cases[0].args, vec![vec![0u8]]);
        assert_eq!(cases[255].case_id, "0xff");
        assert_eq!(cases[255].args, vec![vec![255u8]]);
        for (i, c) in cases.iter().enumerate() {
            assert_eq!(c.args[0][0] as usize, i);
        }
    }

    #[test]
    fn test_capability_denied_without_porting() {
        let target = target::LIBC_TOUPPER;
        let cases = target::byte_domain_cases();
        let denied = dialect_cage::observe_target(&target, &cases, &PortingAuthority::none());
        assert_eq!(denied, Err(PortError::CapabilityDenied));
    }

    #[test]
    fn test_sealed_index_is_gated_and_points_at_compiled_artifact() {
        let mut index = SealedPortIndex::new();
        index.insert(SealedPortEntry {
            target: "libc:toupper:c-locale:u8:v1".to_string(),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::leaf_object(
                "dd".to_string(),
                "phost/evidence/porting/toupper/candidate.o".to_string(),
            ),
            oracle_hash: "aa".to_string(),
            candidate_behavior_hash: "bb".to_string(),
            candidate_source_hash: "cc".to_string(),
            sealed_package: "sealed_package.json".to_string(),
        });
        let id = "libc:toupper:c-locale:u8:v1";

        // No ambient authority: invisible without PORTING.
        assert!(index.lookup_gated(id, &PortingAuthority::none()).is_none());
        assert!(index
            .native_artifact(id, &PortingAuthority::none())
            .is_none());

        // With authority, the runtime artifact is the compiled object.
        let (path, hash) = index
            .native_artifact(id, &PortingAuthority::granted())
            .unwrap();
        assert!(path.ends_with("candidate.o"));
        assert_eq!(hash, "dd");
    }

    #[test]
    fn test_native_call_memchr_finds_the_first_match_index() {
        let r = run_native_call(
            "memchr",
            // "abc" search 'b' over n = 3 -> index 1
            Some("616263:62:0300000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "01000000");
        assert!(r.matches_mirror);
        assert_eq!(r.elf_symbol, "_phor_phor_memchr_index");
    }

    #[test]
    fn test_native_call_memchr_absent_is_minus_one() {
        let r = run_native_call(
            "memchr",
            // "abc" search 'z' over n = 3 -> -1
            Some("616263:7a:0300000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "ffffffff");
        assert!(r.matches_mirror);
    }

    #[test]
    fn test_native_call_strlen_returns_the_terminator_index() {
        let r = run_native_call(
            "strlen",
            // "abc\0" with the terminator inside the bound n = 4 -> length 3.
            Some("61626300:0400000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "0300000000000000");
        assert!(r.matches_mirror);
        assert_eq!(r.elf_symbol, "_phor_phor_strlen_len");
    }

    #[test]
    fn test_native_call_strlen_empty_string_is_zero() {
        let r = run_native_call(
            "strlen",
            // The very first byte is NUL -> length 0, and the tail is ignored.
            Some("0061626300:0500000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "0000000000000000");
        assert!(r.matches_mirror);
    }

    #[test]
    fn test_native_call_strrchr_returns_the_last_match() {
        let r = run_native_call(
            "strrchr",
            // "ababc\0" search 'b' -> last occurrence at index 3.
            Some("616261626300:62:0600000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "03000000");
        assert!(r.matches_mirror);
        assert_eq!(r.elf_symbol, "_phor_phor_strrchr_index");
    }

    #[test]
    fn test_native_call_strrchr_nul_needle_is_the_length() {
        let r = run_native_call(
            "strrchr",
            // "abc\0" search NUL -> the terminator index (the length) = 3.
            Some("61626300:00:0400000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "03000000");
        assert!(r.matches_mirror);
    }

    #[test]
    fn test_native_call_strrchr_absent_is_minus_one() {
        let r = run_native_call(
            "strrchr",
            // "abc\0" search 'z' -> absent.
            Some("61626300:7a:0400000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "ffffffff");
        assert!(r.matches_mirror);
    }

    #[test]
    fn test_observation_is_deterministic() {
        let a = traces_for_domain();
        let b = traces_for_domain();
        assert_eq!(a, b);
        assert_eq!(
            oracle_trace::combined_oracle_hash(&a),
            oracle_trace::combined_oracle_hash(&b)
        );
    }

    // ---- runtime dispatch (end-to-end through the committed persistent store) ----

    #[test]
    fn test_native_call_prefers_the_sealed_object() {
        let r = run_native_call(
            "toupper",
            Some("61"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "41");
        assert!(r.matches_mirror);
        assert_eq!(r.elf_symbol, "_phor_phor_toupper");
        assert!(r.sealed_package.ends_with("toupper/sealed_package.json"));
    }

    #[test]
    fn test_native_call_without_capability_falls_back_to_foreign() {
        let r = run_native_call(
            "toupper",
            Some("61"),
            store::STORE_PATH,
            &PortingAuthority::none(),
        )
        .unwrap();
        assert_eq!(r.source, "foreign-fallback");
        assert!(r.output_hex.is_empty());
        assert!(r.reason.contains("capability"));
    }

    #[test]
    fn test_native_call_memcmp_orders_two_buffers() {
        let r = run_native_call(
            "memcmp",
            // "abc" vs "abd", n = 3 (little-endian u64)
            Some("616263:616264:0300000000000000"),
            store::STORE_PATH,
            &PortingAuthority::granted(),
        )
        .unwrap();
        assert_eq!(r.source, "sealed-object");
        assert_eq!(r.output_hex, "ffffffff"); // less
        assert!(r.matches_mirror);
    }

    #[test]
    fn test_native_call_without_a_store_fails_closed() {
        // The committed store is the seal. If it is missing the call site must not
        // pretend to have native code: it is an error, not a silent fallback.
        let err = run_native_call(
            "toupper",
            Some("61"),
            "phost/evidence/store/does-not-exist.json",
            &PortingAuthority::granted(),
        )
        .unwrap_err();
        assert!(matches!(err, PortError::Io(_)));
    }

    #[test]
    fn test_persistent_composition_index_carries_every_sealed_port() {
        // The runtime load must carry both leaf objects and composition chain
        // hashes, and must need no compiler to build.
        let index = load_persistent_index(CompositionKind::ToupperEach).unwrap();
        let auth = PortingAuthority::granted();
        assert!(index
            .native_artifact(target::LIBC_TOUPPER.id, &auth)
            .is_some());
        let each = "phor:compose:toupper_each:c-locale:u8s:v1";
        let (id, chain_hash) = index.sealed_chain(each, &auth).unwrap();
        assert_eq!(id, each);
        assert_eq!(chain_hash.len(), 64);
    }

    #[test]
    fn test_nested_persistent_index_needs_its_inner_composition() {
        // A nested chain needs the inner composition in the store, not just leaves.
        let index = load_persistent_index(CompositionKind::ToupperEachStrlenMemchr).unwrap();
        let auth = PortingAuthority::granted();
        assert!(index
            .sealed_chain("phor:compose:toupper_each:c-locale:u8s:v1", &auth)
            .is_some());
        assert!(index.sealed_chain(target::LIBC_MEMCHR.id, &auth).is_none());
    }

    /// Phase 2 acceptance: the generic IR court and the legacy bespoke court agree
    /// on **every** composition — the same external answers, the same
    /// native/fallback accounting, and the same dispatch count. This is the
    /// equivalence proof that lets the runtime use the IR and lets the legacy
    /// court remain only as the historical v1 evidence model.
    #[test]
    fn test_ir_court_agrees_with_the_legacy_court_for_every_composition() {
        let auth = PortingAuthority::granted();
        for def in crate::porting::composition_registry::ALL.iter() {
            let kind = CompositionKind::parse(def.target.id).expect("known kind");
            let index = load_persistent_index(kind).expect("persistent index");
            let cases = (def.cases)();
            let traces = (def.observe)(&cases, &auth).expect("oracle");

            // Legacy v1 court (bespoke runner).
            let legacy = match kind {
                CompositionKind::ToupperMemchr => {
                    let (v, _) = composition::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperStrlenMemchr => {
                    let (v, _) =
                        composition_strlen_memchr::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperStrlenMemchrPair => {
                    let (v, _) = composition_pair::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperEach => {
                    let (v, _) =
                        composition_toupper_each::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperEachStrlenMemchr => {
                    let (v, _) = composition_nested::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperMemchrSuffix => {
                    let (v, _) = composition_suffix::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
                CompositionKind::ToupperEachSliceSearch => {
                    let (v, _) =
                        composition_slice_search::run_composition_court(&traces, &index, &auth);
                    legacy_tuple(
                        v.cases_run,
                        v.fallback_cases,
                        v.broken_seal_cases,
                        v.cases_passed,
                        v.cases_failed,
                        v.dispatches_run,
                        v.is_sealed_eligible(),
                    )
                }
            };

            let (v2, mismatches) = composition_engine::run_ir_court(def, &traces, &index, &auth);
            let generic = legacy_tuple(
                v2.cases_run,
                v2.fallback_cases,
                v2.broken_seal_cases,
                v2.cases_passed,
                v2.cases_failed,
                v2.dispatches_run,
                v2.is_sealed_eligible(),
            );

            // A chain that dispatches a nested **composition** stage has two
            // legitimate dispatch-count definitions: the legacy court counts only
            // the chain's own stage calls, the generic court counts every stage
            // including the nested composition's internals (the runtime/service
            // definition). Compare behavior and non-dispatch accounting always, and
            // the dispatch count only where the two definitions coincide.
            let nested = def
                .target
                .stages
                .iter()
                .any(|s| s.starts_with("phor:compose:"));
            let (lc, lf, lb, lp, lfa, ld, ls) = legacy;
            let (gc, gf, gb, gp, gfa, gd, gs) = generic;
            assert_eq!(
                (lc, lf, lb, lp, lfa, ls),
                (gc, gf, gb, gp, gfa, gs),
                "{}: legacy and generic courts disagree ({:?})",
                def.target.id,
                mismatches
            );
            if nested {
                // The generic count includes the nested composition's internals.
                assert!(gd >= ld, "{}: generic dispatches < legacy", def.target.id);
            } else {
                assert_eq!(ld, gd, "{}: dispatch count differs", def.target.id);
            }
        }
    }

    /// `(cases_run, fallback, broken, passed, failed, dispatches, sealed)`.
    #[allow(clippy::too_many_arguments)]
    fn legacy_tuple(
        cases: u64,
        fallback: u64,
        broken: u64,
        passed: u64,
        failed: u64,
        dispatches: u64,
        sealed: bool,
    ) -> (u64, u64, u64, u64, u64, u64, bool) {
        (cases, fallback, broken, passed, failed, dispatches, sealed)
    }
}
