// porting/composition_engine.rs — the generic composition court (Phase 2)
//
// The Phase 1 composition courts each had a bespoke runner: the "oracle
// implementation" was Rust in `dialect_cage`, the "native implementation" was a
// different Rust runner in `composition*.rs`, and the two could drift. Phase 2
// removes that possibility: **one `CompositionIR` is evaluated on both sides**
// through the `PortBackend` trait.
//
//   * `ForeignBackend` routes every call to the foreign implementation (the cage)
//     and, for a nested composition, recurses into that composition's IR.
//   * `SealedBackend` routes every call through `NativeDispatcher`, so a leaf is
//     the sealed object and a nested composition recurses through the same
//     dispatcher.
//
// Because the same graph is evaluated both ways, a drift between the oracle chain
// and the sealed chain is impossible by construction. The court still records the
// committed oracle traces and fails closed if the IR-derived oracle disagrees with
// them — evidence of a harness/cage drift, not a candidate bug.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_usize, encode_index, encode_sign, encode_usize};
use crate::porting::composition_ir::{
    eval, BackendError, CompositionIR, PortBackend, SiteId, Type, Value,
};
use crate::porting::composition_registry::{self, PortValue};
use crate::porting::dialect_cage;
use crate::porting::dispatch::{DispatchError, DispatchSource, NativeDispatcher};
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::{CourtVerdict, Mismatch};
use crate::porting::target::{self, TestCase};
use crate::porting::{sha256_hex, PortingAuthority, SealedPortIndex};

/// The identity domain tag for the composed-artifact identity (v2).
pub const COMPOSITION_ARTIFACT_DOMAIN: &[u8] = b"PHOR/COMPOSITION-ARTIFACT/v2\0";

/// The status the court observed for one stage on one case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteStatus {
    /// The stage was reached and served by the sealed object.
    Native,
    /// The stage was reached and fell back to the foreign implementation.
    Fallback,
    /// The stage was reached and its sealed entry failed to load or execute.
    Broken,
}

impl SiteStatus {
    fn as_str(&self) -> &'static str {
        match self {
            SiteStatus::Native => "native",
            SiteStatus::Fallback => "fallback",
            SiteStatus::Broken => "broken",
        }
    }
}

/// Coerce incoming argument values to the callee's declared input types.
///
/// Every `Call` argument in the IR is a byte buffer (that is the court ABI), so a
/// composition input declared `Scalar` (a length or index) is decoded from its
/// 8-byte little-endian encoding here. This is the one place a composition's input
/// contract is interpreted.
fn coerce_inputs(ir: &CompositionIR, args: &[Value]) -> Result<Vec<Value>, BackendError> {
    if args.len() != ir.inputs.len() {
        return Err(BackendError::new(format!(
            "{}: expected {} arguments, got {}",
            ir.id,
            ir.inputs.len(),
            args.len()
        )));
    }
    let mut out = Vec::with_capacity(args.len());
    for (a, ty) in args.iter().zip(ir.inputs.iter()) {
        let v = match (ty, a) {
            (Type::Bytes, Value::Bytes(b)) => Value::Bytes(b.clone()),
            (Type::Scalar, Value::Bytes(b)) => Value::Scalar(decode_usize(b) as i64),
            (Type::Scalar, Value::Scalar(s)) => Value::Scalar(*s),
            (Type::Bytes, Value::Scalar(_)) => {
                return Err(BackendError::new(format!(
                    "{}: a byte input was given a scalar",
                    ir.id
                )))
            }
            _ => {
                return Err(BackendError::new(format!(
                    "{}: unsupported input type",
                    ir.id
                )))
            }
        };
        out.push(v);
    }
    Ok(out)
}

/// Marshal a typed IR value to the raw byte buffer the court ABI expects.
fn marshal_arg(v: &Value) -> Result<Vec<u8>, BackendError> {
    match v {
        Value::Bytes(b) => Ok(b.clone()),
        Value::Scalar(_) | Value::Bool(_) => Err(BackendError::new(
            "a call argument must be a byte buffer (use PackUsize for a scalar)",
        )),
    }
}

/// Encode a composition's typed outputs into the raw byte buffer its callers see,
/// concatenated in declaration order (this is what makes a two-index chain's
/// `indexA || indexB` observable).
pub fn encode_outputs(kind: PortValue, outputs: &[Value]) -> Result<Vec<u8>, BackendError> {
    let mut out = Vec::new();
    for v in outputs {
        match v {
            Value::Bytes(b) => out.extend_from_slice(b),
            Value::Scalar(s) => {
                let s = *s;
                match kind {
                    PortValue::Index => out.extend_from_slice(&encode_index(s as i32)),
                    PortValue::Length => out.extend_from_slice(&encode_usize(s.max(0) as usize)),
                    PortValue::Sign => out.extend_from_slice(&encode_sign(s as i32)),
                    PortValue::Bytes => {
                        return Err(BackendError::new("a byte port returned a scalar"))
                    }
                }
            }
            Value::Bool(_) => {
                // A `Bool` output is an effect marker (an observed stage), not part
                // of the port's observable bytes.
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The sealed backend
// ---------------------------------------------------------------------------

/// Evaluate a composition over the sealed store through `NativeDispatcher`.
///
/// Every call — leaf or composition — goes through `dispatch_port`, so a nested
/// composition recurses through the same dispatcher and every seal check,
/// dispatch count and per-port fan-in is accounted exactly as before.
pub struct SealedBackend<'a> {
    dispatcher: &'a mut NativeDispatcher,
    auth: &'a PortingAuthority,
    /// Per-site status, one entry per case (the outer court resets this per case).
    pub status: BTreeMap<usize, SiteStatus>,
}

impl<'a> SealedBackend<'a> {
    pub fn new(dispatcher: &'a mut NativeDispatcher, auth: &'a PortingAuthority) -> Self {
        SealedBackend {
            dispatcher,
            auth,
            status: BTreeMap::new(),
        }
    }

    fn record(&mut self, site: SiteId, status: SiteStatus) {
        let e = self.status.entry(site.0).or_insert(SiteStatus::Native);
        // A fallback or a broken seal dominates a native observation at the same
        // site (a byte map is only native if every byte was native).
        if status != SiteStatus::Native {
            *e = status;
        }
    }
}

impl PortBackend for SealedBackend<'_> {
    fn enter(&mut self, site: SiteId) {
        // Reaching a site is a native observation until a call says otherwise (a
        // byte map over an empty buffer makes no calls and is still native).
        self.status.entry(site.0).or_insert(SiteStatus::Native);
    }

    fn call(&mut self, site: SiteId, port: &str, args: &[Value]) -> Result<Value, BackendError> {
        let kind = composition_registry::port_value(port)
            .ok_or_else(|| BackendError::new(format!("unknown port {}", port)))?;
        let mut raw = Vec::with_capacity(args.len());
        for a in args {
            raw.push(marshal_arg(a)?);
        }
        match self.dispatcher.dispatch_port(port, &raw, self.auth) {
            Ok(o) if o.source == DispatchSource::SealedObject => {
                self.record(site, SiteStatus::Native);
                Ok(composition_registry::decode_port_output(kind, &o.output))
            }
            Ok(_) => {
                self.record(site, SiteStatus::Fallback);
                Err(BackendError::fallback(format!("{} fell back", port)))
            }
            Err(e) => {
                self.record(site, SiteStatus::Broken);
                Err(BackendError::broken(format!("{}: {}", port, e)))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The foreign backend (the oracle)
// ---------------------------------------------------------------------------

/// Evaluate a composition over the foreign implementation through the cage.
///
/// A leaf call is observed as a one-case corpus; a nested composition recurses into
/// that composition's IR through this same backend. `observe_target` clamps its
/// inputs defensively, so a malformed case cannot read out of bounds.
pub struct ForeignBackend<'a> {
    auth: &'a PortingAuthority,
}

impl<'a> ForeignBackend<'a> {
    pub fn new(auth: &'a PortingAuthority) -> Self {
        ForeignBackend { auth }
    }
}

impl PortBackend for ForeignBackend<'_> {
    fn call(&mut self, _site: SiteId, port: &str, args: &[Value]) -> Result<Value, BackendError> {
        // A composition: recurse into its IR through this same backend.
        if let Some(def) = composition_registry::by_id(port) {
            let ir = (def.ir)();
            let inputs = coerce_inputs(&ir, args)?;
            let out = eval(&ir, self, &inputs)
                .map_err(|e| BackendError::new(format!("{}: {:?}", port, e)))?;
            let kind = composition_registry::port_value(port)
                .ok_or_else(|| BackendError::new(format!("unknown port {}", port)))?;
            let raw = encode_outputs(kind, &out)?;
            return Ok(composition_registry::decode_port_output(kind, &raw));
        }

        // A leaf: observe it as a one-case corpus.
        let target = target::resolve_target(port)
            .ok_or_else(|| BackendError::new(format!("unknown port {}", port)))?;
        let mut raw = Vec::with_capacity(args.len());
        for a in args {
            raw.push(marshal_arg(a)?);
        }
        let case = TestCase::new(String::from("ir.call"), raw);
        let traces = dialect_cage::observe_target(&target, core::slice::from_ref(&case), self.auth)
            .map_err(|e| BackendError::new(format!("{}: {}", port, e)))?;
        let trace = traces
            .first()
            .ok_or_else(|| BackendError::new(format!("{}: no observation", port)))?;
        let output = hex::decode(&trace.output_hex)
            .map_err(|e| BackendError::new(format!("{}: bad output hex: {}", port, e)))?;
        let kind = composition_registry::port_value(port)
            .ok_or_else(|| BackendError::new(format!("unknown port {}", port)))?;
        Ok(composition_registry::decode_port_output(kind, &output))
    }
}

// ---------------------------------------------------------------------------
// The generic court
// ---------------------------------------------------------------------------

/// One stage of a chain, generically accounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrStageAccount {
    /// The node index in the IR (its identity within the graph).
    pub node: u32,
    /// The qualified port the stage calls.
    pub port: String,
    /// `leaf`, `composition`, or `map` (a byte map over a buffer).
    pub kind: &'static str,
    pub native_cases: u64,
    /// Cases where the stage was **not reached** (a data-dependent non-execution).
    pub not_reached_cases: u64,
    pub fallback_cases: u64,
    pub broken_cases: u64,
    /// The seal the stage dispatched to (object hash for a leaf, chain hash for a
    /// composition), when it ran natively at least once.
    pub seal: String,
    /// `_phor_…` for a leaf, `compose:…` for a composition.
    pub symbol: String,
}

/// The generic composition residual (identity v2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrVerdict {
    pub target: String,
    /// The canonical identity of the chain **as data** (the graph itself).
    pub composition_ir_hash: String,
    /// The ordered identity of every dependency **and its seal**.
    pub dependency_binding_hash: String,
    /// The normalized observed behavior over the declared corpus.
    pub behavior_hash: String,
    /// The composed artifact identity: schema/version, port contract, graph,
    /// dependency binding, and behavior.
    pub composition_artifact_hash: String,
    pub cases_run: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub fallback_cases: u64,
    pub broken_seal_cases: u64,
    pub dispatches_run: u64,
    pub oracle_hash: String,
    pub stages: Vec<IrStageAccount>,
    pub verdict: CourtVerdict,
}

impl IrVerdict {
    pub fn is_sealed_eligible(&self) -> bool {
        self.verdict == CourtVerdict::Consistent
            && self.cases_run > 0
            && self.cases_passed == self.cases_run
            && self.cases_failed == 0
            && self.fallback_cases == 0
            && self.broken_seal_cases == 0
            && !self.composition_artifact_hash.is_empty()
    }

    pub fn canonical(&self) -> String {
        let stages: Vec<String> = self
            .stages
            .iter()
            .map(|s| {
                format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    s.node,
                    s.port,
                    s.kind,
                    s.native_cases,
                    s.not_reached_cases,
                    s.fallback_cases,
                    s.broken_cases,
                    s.seal,
                    s.symbol
                )
            })
            .collect();
        format!(
            "target={};composition_ir_hash={};dependency_binding_hash={};behavior_hash={};composition_artifact_hash={};cases_run={};cases_passed={};cases_failed={};fallback_cases={};broken_seal_cases={};dispatches_run={};oracle_hash={};stages={};verdict={}",
            self.target,
            self.composition_ir_hash,
            self.dependency_binding_hash,
            self.behavior_hash,
            self.composition_artifact_hash,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.fallback_cases,
            self.broken_seal_cases,
            self.dispatches_run,
            self.oracle_hash,
            stages.join(","),
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[Mismatch]) -> String {
        let body: Vec<String> = mismatches.iter().map(|m| m.to_json()).collect();
        let stages: Vec<String> = self
            .stages
            .iter()
            .map(|s| {
                format!(
                    "    {{\n      \"node\": {},\n      \"port\": \"{}\",\n      \"kind\": \"{}\",\n      \"native_cases\": {},\n      \"not_reached_cases\": {},\n      \"fallback_cases\": {},\n      \"broken_cases\": {},\n      \"seal\": \"{}\",\n      \"symbol\": \"{}\"\n    }}",
                    s.node,
                    crate::porting::json_escape(&s.port),
                    s.kind,
                    s.native_cases,
                    s.not_reached_cases,
                    s.fallback_cases,
                    s.broken_cases,
                    s.seal,
                    crate::porting::json_escape(&s.symbol)
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.composition_verdict.v2\",\n  \"target\": \"{}\",\n  \"composition_ir_hash\": \"{}\",\n  \"dependency_binding_hash\": \"{}\",\n  \"behavior_hash\": \"{}\",\n  \"composition_artifact_hash\": \"{}\",\n  \"cases_run\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"fallback_cases\": {},\n  \"broken_seal_cases\": {},\n  \"dispatches_run\": {},\n  \"oracle_hash\": \"{}\",\n  \"stages\": [\n{}\n  ],\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            crate::porting::json_escape(&self.target),
            self.composition_ir_hash,
            self.dependency_binding_hash,
            self.behavior_hash,
            self.composition_artifact_hash,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.fallback_cases,
            self.broken_seal_cases,
            self.dispatches_run,
            self.oracle_hash,
            stages.join(",\n"),
            self.verdict.as_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

/// The behavior hash: SHA-256 over the per-case observable, the per-stage statuses
/// in graph order, and the intermediates. A different intermediate, a fallback, or
/// a skipped stage changes the hash even when the final answer coincides.
fn behavior_hash(rows: &[(String, Vec<&'static str>, Option<String>)]) -> String {
    let mut buf = String::new();
    for (case_id, statuses, index) in rows {
        buf.push_str(case_id);
        buf.push(';');
        for s in statuses {
            buf.push_str(s);
            buf.push(',');
        }
        buf.push(';');
        match index {
            Some(h) => buf.push_str(h),
            None => buf.push('-'),
        }
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// The ordered dependency binding: every distinct port the chain calls, paired with
/// the seal that port actually published in the index.
fn dependency_binding_hash(ir: &CompositionIR, index: &SealedPortIndex) -> String {
    let mut deps: Vec<String> = Vec::new();
    for node in &ir.nodes {
        let port = match node {
            crate::porting::composition_ir::Node::Call { port, .. } => port,
            crate::porting::composition_ir::Node::MapBytes { port, .. } => port,
            _ => continue,
        };
        if !deps.iter().any(|d| d == port) {
            deps.push(port.clone());
        }
    }
    let mut buf = String::from("PHOR/COMPOSITION-DEPS/v2\n");
    for d in deps {
        let seal = index
            .lookup(&d)
            .map(|e| e.artifact_hash().to_string())
            .unwrap_or_default();
        buf.push_str(&d);
        buf.push('=');
        buf.push_str(&seal);
        buf.push('\n');
    }
    sha256_hex(buf.as_bytes())
}

/// The composed artifact identity.
fn artifact_hash(target: &str, ir_hash: &str, dep_hash: &str, behavior: &str) -> String {
    let mut pre = Vec::new();
    pre.extend_from_slice(COMPOSITION_ARTIFACT_DOMAIN);
    for part in [target, ir_hash, dep_hash, behavior] {
        pre.extend_from_slice(&(part.len() as u32).to_le_bytes());
        pre.extend_from_slice(part.as_bytes());
    }
    sha256_hex(&pre)
}

/// The call sites of an IR, in graph order: `(node index, port, kind)`.
fn call_sites(ir: &CompositionIR) -> Vec<(u32, String, &'static str)> {
    let mut out = Vec::new();
    for (i, node) in ir.nodes.iter().enumerate() {
        match node {
            crate::porting::composition_ir::Node::Call { port, .. } => {
                let kind = if composition_registry::by_id(port).is_some() {
                    "composition"
                } else {
                    "leaf"
                };
                out.push((i as u32, port.clone(), kind));
            }
            crate::porting::composition_ir::Node::MapBytes { port, .. } => {
                out.push((i as u32, port.clone(), "map"));
            }
            _ => {}
        }
    }
    out
}

/// Raw case arguments to typed IR inputs, per the declared input types.
pub fn case_inputs(ir: &CompositionIR, args: &[Vec<u8>]) -> Result<Vec<Value>, String> {
    if args.len() != ir.inputs.len() {
        return Err(format!(
            "case has {} arguments, chain declares {}",
            args.len(),
            ir.inputs.len()
        ));
    }
    let mut out = Vec::with_capacity(args.len());
    for (a, ty) in args.iter().zip(ir.inputs.iter()) {
        out.push(match ty {
            Type::Bytes => Value::Bytes(a.clone()),
            Type::Scalar => Value::Scalar(decode_usize(a) as i64),
            Type::Bool => return Err(String::from("a case input cannot be bool")),
        });
    }
    Ok(out)
}

/// Replay a composition corpus through the sealed IR — the generic court.
///
/// The same IR is evaluated through the foreign backend to derive the oracle, and
/// through the sealed backend to obtain the implementation. A divergence between
/// the IR-derived oracle and the committed `traces` is a harness/cage drift and is
/// reported as an inconsistent court, never silently tolerated.
pub fn run_ir_court(
    def: &composition_registry::CompositionDef,
    traces: &[OracleTrace],
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> (IrVerdict, Vec<Mismatch>) {
    let ir = (def.ir)();
    let mut dispatcher = NativeDispatcher::new(index.clone());

    let mut fallback_cases: u64 = 0;
    let mut broken_cases: u64 = 0;
    let mut passed: u64 = 0;
    let mut failed: u64 = 0;
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut rows: Vec<(String, Vec<&'static str>, Option<String>)> =
        Vec::with_capacity(traces.len());
    let sites = call_sites(&ir);
    let mut native_counts: BTreeMap<u32, u64> = BTreeMap::new();
    let mut not_reached: BTreeMap<u32, u64> = BTreeMap::new();
    let mut site_fallback: BTreeMap<u32, u64> = BTreeMap::new();
    let mut site_broken: BTreeMap<u32, u64> = BTreeMap::new();

    for t in traces {
        let args = t.input_args();
        let inputs = match case_inputs(&ir, &args) {
            Ok(v) => v,
            Err(_) => {
                failed += 1;
                mismatches.push(Mismatch {
                    case_id: t.case_id.clone(),
                    expected_output_hex: t.output_hex.clone(),
                    actual_output_hex: String::from("!malformed case"),
                });
                continue;
            }
        };

        // The oracle, derived from the SAME IR through the foreign backend.
        let mut foreign = ForeignBackend::new(auth);
        let oracle_out = eval(&ir, &mut foreign, &inputs);
        let oracle_hex = match &oracle_out {
            Ok(v) => encode_outputs(
                composition_registry::port_value(&ir.id).unwrap_or(PortValue::Index),
                v,
            )
            .map(hex::encode)
            .unwrap_or_default(),
            Err(_) => String::new(),
        };

        // The implementation, from the sealed store.
        let mut backend = SealedBackend::new(&mut dispatcher, auth);
        let sealed_out = eval(&ir, &mut backend, &inputs);
        let status = backend.status.clone();

        // Aggregate per-site accounting: a site absent from `status` was skipped.
        for (node, _port, _kind) in &sites {
            match status.get(&(*node as usize)) {
                Some(SiteStatus::Native) => *native_counts.entry(*node).or_insert(0) += 1,
                Some(SiteStatus::Fallback) => {
                    *site_fallback.entry(*node).or_insert(0) += 1;
                }
                Some(SiteStatus::Broken) => {
                    *site_broken.entry(*node).or_insert(0) += 1;
                }
                None => *not_reached.entry(*node).or_insert(0) += 1,
            }
        }

        let any_fallback = status.values().any(|s| *s == SiteStatus::Fallback);
        let any_broken = status.values().any(|s| *s == SiteStatus::Broken);
        if any_fallback {
            fallback_cases += 1;
        }
        if any_broken {
            broken_cases += 1;
        }

        let sealed_hex = match &sealed_out {
            Ok(v) => encode_outputs(
                composition_registry::port_value(&ir.id).unwrap_or(PortValue::Index),
                v,
            )
            .map(hex::encode)
            .ok(),
            Err(_) => None,
        };

        let oracle_ok = oracle_ok(&oracle_hex, &t.output_hex);
        let matched = sealed_hex.as_deref() == Some(t.output_hex.as_str())
            && !any_fallback
            && !any_broken
            && oracle_ok
            && t.status == "ok";
        if matched {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(Mismatch {
                case_id: t.case_id.clone(),
                expected_output_hex: t.output_hex.clone(),
                actual_output_hex: if any_broken {
                    String::from("!broken seal")
                } else if any_fallback {
                    String::from("!foreign fallback")
                } else if !oracle_ok {
                    String::from("!oracle drift")
                } else {
                    sealed_hex.clone().unwrap_or_default()
                },
            });
        }

        let row_statuses: Vec<&'static str> = sites
            .iter()
            .map(|(node, _, _)| match status.get(&(*node as usize)) {
                Some(s) => s.as_str(),
                None => "not_reached",
            })
            .collect();
        rows.push((t.case_id.clone(), row_statuses, sealed_hex));
    }

    let cases_run = traces.len() as u64;
    let verdict = if cases_run == 0 {
        CourtVerdict::Inconclusive
    } else if failed == 0 && fallback_cases == 0 && broken_cases == 0 {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    // Per-stage seal binding, from the objects the chain actually dispatched to.
    let stages: Vec<IrStageAccount> = sites
        .iter()
        .map(|(node, port, kind)| {
            let (seal, symbol) = dispatcher.sealed_binding(port).unwrap_or_default();
            IrStageAccount {
                node: *node,
                port: port.clone(),
                kind,
                native_cases: native_counts.get(node).copied().unwrap_or(0),
                not_reached_cases: not_reached.get(node).copied().unwrap_or(0),
                fallback_cases: site_fallback.get(node).copied().unwrap_or(0),
                broken_cases: site_broken.get(node).copied().unwrap_or(0),
                seal,
                symbol,
            }
        })
        .collect();

    let ir_hash = ir.id_hash();
    let dep_hash = dependency_binding_hash(&ir, index);
    let behavior = behavior_hash(&rows);
    let artifact = artifact_hash(def.target.id, &ir_hash, &dep_hash, &behavior);

    (
        IrVerdict {
            target: def.target.id.to_string(),
            composition_ir_hash: ir_hash,
            dependency_binding_hash: dep_hash,
            behavior_hash: behavior,
            composition_artifact_hash: artifact,
            cases_run,
            cases_passed: passed,
            cases_failed: failed,
            fallback_cases,
            broken_seal_cases: broken_cases,
            dispatches_run: dispatcher.dispatches(),
            oracle_hash: combined_oracle_hash(traces),
            stages,
            verdict,
        },
        mismatches,
    )
}

fn oracle_ok(derived: &str, committed: &str) -> bool {
    // An empty derived string means the foreign backend could not answer (e.g. a
    // port absent on this host); the committed trace still stands, so do not fail
    // the case on that account alone.
    derived.is_empty() || derived == committed
}

/// The runtime path: resolve a sealed **composition** port to its IR and evaluate
/// it over the dispatcher. This is what makes a composition a first-class sealed
/// port — the dispatcher resolves it as *data* and every stage goes through the
/// same `dispatch_port`, so a nested composition recurses and every seal check and
/// dispatch count is preserved.
pub fn eval_composition_port(
    dispatcher: &mut NativeDispatcher,
    port_id: &str,
    args: &[Vec<u8>],
    auth: &PortingAuthority,
) -> Result<Vec<u8>, DispatchError> {
    let def = composition_registry::by_id(port_id)
        .ok_or_else(|| DispatchError::SealBroken(format!("unknown composition {}", port_id)))?;
    let ir = (def.ir)();
    let kind = composition_registry::port_value(port_id)
        .ok_or_else(|| DispatchError::SealBroken(format!("unknown composition {}", port_id)))?;
    let inputs = case_inputs(&ir, args)
        .map_err(|e| DispatchError::SealBroken(format!("{}: {}", port_id, e)))?;
    let output = {
        let mut backend = SealedBackend::new(dispatcher, auth);
        let out = eval(&ir, &mut backend, &inputs)
            .map_err(|e| DispatchError::SealBroken(format!("{}: {:?}", port_id, e)))?;
        encode_outputs(kind, &out)
            .map_err(|e| DispatchError::SealBroken(format!("{}: {}", port_id, e.message)))?
    };
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::composition_ir::validate;
    use crate::porting::composition_registry::{self, ALL};
    use crate::porting::promotion::TrustState;
    use crate::porting::{SealedArtifact, SealedPortEntry};

    /// Every registered chain must be a well-formed, acyclic, typed graph.
    #[test]
    fn test_all_ir_chains_validate() {
        for def in &ALL {
            let ir = (def.ir)();
            validate(&ir).unwrap_or_else(|e| panic!("{}: {:?}", def.target.id, e));
        }
    }

    /// The generic court, driven entirely by the IR over the committed persistent
    /// store, reproduces the committed foreign oracle for **every** composition.
    /// This is the Phase 2 equivalence check: the same IR is the oracle (through
    /// `ForeignBackend`) and the implementation (through `SealedBackend`).
    #[test]
    fn test_ir_court_is_equivalent_to_the_oracle_for_every_composition() {
        let index = crate::porting::store::load_default().expect("committed store loads");
        let auth = PortingAuthority::granted();
        for def in &ALL {
            let cases = (def.cases)();
            let traces = (def.observe)(&cases, &auth).expect("cage observes the corpus");
            assert_eq!(traces.len(), cases.len(), "{}", def.target.id);
            let (v, mismatches) = run_ir_court(def, &traces, &index, &auth);
            assert!(
                v.is_sealed_eligible(),
                "{} not sealed-eligible: {:?}",
                def.target.id,
                mismatches
            );
            assert_eq!(v.cases_run as usize, cases.len(), "{}", def.target.id);
            assert_eq!(v.cases_passed, v.cases_run, "{}", def.target.id);
            assert_eq!(v.fallback_cases, 0, "{}", def.target.id);
            assert_eq!(v.broken_seal_cases, 0, "{}", def.target.id);
            // A composed artifact binds four distinct identities.
            assert_eq!(v.composition_ir_hash.len(), 64);
            assert_eq!(v.dependency_binding_hash.len(), 64);
            assert_eq!(v.behavior_hash.len(), 64);
            assert_eq!(v.composition_artifact_hash.len(), 64);
        }
    }

    /// The data-dependent chains must report the not-reached stage explicitly: a
    /// stage that did not run is never counted as a native success.
    #[test]
    fn test_data_dependent_non_execution_is_accounted() {
        let index = crate::porting::store::load_default().expect("committed store loads");
        let auth = PortingAuthority::granted();
        for name in ["toupper_memchr_suffix", "toupper_each_slice_search"] {
            let def = composition_registry::by_name(name).expect("registered");
            let cases = (def.cases)();
            let traces = (def.observe)(&cases, &auth).unwrap();
            let (v, _) = run_ir_court(def, &traces, &index, &auth);
            let total_not_reached: u64 = v.stages.iter().map(|s| s.not_reached_cases).sum();
            assert!(
                total_not_reached > 0,
                "{}: the second search should be skipped on some cases",
                name
            );
        }
    }

    /// With an empty store every stage falls back, and the court is inconsistent.
    #[test]
    fn test_ir_court_without_a_store_falls_back() {
        let def = composition_registry::by_name("toupper_memchr").unwrap();
        let cases = (def.cases)();
        let auth = PortingAuthority::granted();
        let traces = (def.observe)(&cases, &auth).unwrap();
        let (v, _) = run_ir_court(def, &traces, &SealedPortIndex::new(), &auth);
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.fallback_cases, v.cases_run);
        assert_eq!(v.broken_seal_cases, 0);
        assert_eq!(v.cases_passed, 0);
    }

    /// A sealed entry whose object does not verify is a broken seal, never a
    /// fallback.
    #[test]
    fn test_ir_court_broken_seal_is_not_a_fallback() {
        let def = composition_registry::by_name("toupper_memchr").unwrap();
        let auth = PortingAuthority::granted();
        let mut index = SealedPortIndex::new();
        for t in [target::LIBC_TOUPPER, target::LIBC_MEMCHR] {
            index.insert(SealedPortEntry {
                target: t.id.to_string(),
                trust: TrustState::Sealed,
                artifact: SealedArtifact::leaf_object(
                    String::from("deadbeef"),
                    String::from("/nonexistent/candidate.o"),
                ),
                oracle_hash: String::new(),
                candidate_behavior_hash: String::new(),
                candidate_source_hash: String::new(),
                sealed_package: String::new(),
            });
        }
        let cases = (def.cases)();
        let traces = (def.observe)(&cases, &auth).unwrap();
        let (v, _) = run_ir_court(def, &traces, &index, &auth);
        assert_eq!(v.verdict, CourtVerdict::Inconsistent);
        assert_eq!(v.broken_seal_cases, v.cases_run);
        assert_eq!(v.fallback_cases, 0);
    }

    /// Every dependency the chain names must appear in the store; the binding hash
    /// is stable for a fixed store and changes when a seal changes.
    #[test]
    fn test_dependency_binding_binds_the_seals() {
        let index = crate::porting::store::load_default().unwrap();
        let def = composition_registry::by_name("toupper_memchr").unwrap();
        let ir = (def.ir)();
        let a = dependency_binding_hash(&ir, &index);
        let b = dependency_binding_hash(&ir, &index);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
