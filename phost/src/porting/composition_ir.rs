// porting/composition_ir.rs — typed composition IR (Phase 2)
//
// Phase 1 proved sealed ports compose. It did so with six *bespoke Rust runners*
// (`composition.rs`, `composition_strlen_memchr.rs`, …), each a hand-written
// interpreter of one chain shape. That is the opposite of generic: a new
// composition means a new runner, and the oracle implementation and the native
// implementation can drift because they are different code.
//
// Phase 2 makes composition **data**. A `CompositionIR` is a small, typed,
// versioned, acyclic program over already-sealed ports. One interpreter evaluates
// it; the same interpreter runs on the foreign side and the sealed side through a
// `PortBackend`, so there is exactly one meaning of a chain.
//
// This module is the IR core: the types, validation, the canonical content
// identity, and the generic evaluator. Migrating the six bespoke runners onto it
// (and removing `CompositionKind` dispatch) is the remainder of Phase 2, and is
// versioned because it must preserve every committed composition verdict.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The identity domain tag for canonical encoding.
pub const COMPOSITION_IR_DOMAIN: &[u8] = b"PHOR/COMPOSITION-IR/v1\0";

/// Bounds that make a graph auditable and guarantee the interpreter terminates.
pub const MAX_NODES: usize = 256;
pub const MAX_INPUTS: usize = 16;
pub const MAX_OUTPUTS: usize = 8;
pub const MAX_DEPTH: usize = 32;

/// A value's type in the IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A byte string.
    Bytes,
    /// A scalar (length, index, sign, or a byte) carried as an integer.
    Scalar,
    /// A boolean (from a comparison or a presence test).
    Bool,
}

impl Type {
    fn tag(&self) -> u8 {
        match self {
            Type::Bytes => 1,
            Type::Scalar => 2,
            Type::Bool => 3,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Type::Bytes => "bytes",
            Type::Scalar => "scalar",
            Type::Bool => "bool",
        }
    }
}

/// A value reference: the index of the node that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueId(pub u32);

/// A runtime value flowing between nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bytes(Vec<u8>),
    Scalar(i64),
    Bool(bool),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Bytes(_) => Type::Bytes,
            Value::Scalar(_) => Type::Scalar,
            Value::Bool(_) => Type::Bool,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_scalar(&self) -> Option<i64> {
        match self {
            Value::Scalar(s) => Some(*s),
            _ => None,
        }
    }
}

/// One node of a composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// The `i`-th declared input; its type is `CompositionIR::inputs[i]`.
    Input { index: u32 },
    /// A literal scalar.
    ConstantScalar { value: i64 },
    /// A call to a sealed port, returning its observable as a typed value.
    Call { port: String, args: Vec<ValueId> },
    /// Apply a unary byte-to-byte port (e.g. `toupper`) across every byte of a
    /// buffer. The registered port takes a one-byte buffer and returns a one-byte
    /// buffer, matching the court ABI (`toupper` takes `[byte]`).
    MapBytes { port: String, input: ValueId },
    /// Encode a scalar as an 8-byte little-endian buffer. This is the ABI packing
    /// boundary made explicit: a derived length or index that is passed to a port
    /// as an argument must first become the byte buffer that port's ABI expects.
    PackUsize { value: ValueId },
    /// Observe a value for its **effect**: force evaluation of `value` (so a stage
    /// whose *status* is observed runs even when its result is not consumed on this
    /// path) and yield `Bool(true)`. A `Bool` output is a non-observable effect
    /// marker: it is never encoded into the port's observable bytes.
    Observe { value: ValueId },
    /// A sub-slice of a byte buffer: `input[origin .. origin+length]`.
    Slice {
        input: ValueId,
        origin: ValueId,
        length: Option<ValueId>,
    },
    /// A boolean predicate over a scalar: `lhs <op> rhs`.
    Compare {
        lhs: ValueId,
        op: CompareOp,
        rhs: ValueId,
    },
    /// Integer arithmetic on scalars, used to derive a slice's length from the
    /// producer's output (`window - origin`). Saturating, so a window that a
    /// correct `memchr` never violates still cannot panic.
    Binary {
        op: BinOp,
        lhs: ValueId,
        rhs: ValueId,
    },
    /// A conditional value. This is how a data-dependent *non-execution* is
    /// represented explicitly: `Select` is a value choice, and only the taken
    /// branch is evaluated, so "a stage did not run" is never confused with "a
    /// stage succeeded".
    Select {
        condition: ValueId,
        then_value: ValueId,
        else_value: ValueId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CompareOp {
    fn tag(&self) -> u8 {
        match self {
            CompareOp::Eq => 1,
            CompareOp::Ne => 2,
            CompareOp::Lt => 3,
            CompareOp::Le => 4,
            CompareOp::Gt => 5,
            CompareOp::Ge => 6,
        }
    }

    fn eval(self, l: i64, r: i64) -> bool {
        match self {
            CompareOp::Eq => l == r,
            CompareOp::Ne => l != r,
            CompareOp::Lt => l < r,
            CompareOp::Le => l <= r,
            CompareOp::Gt => l > r,
            CompareOp::Ge => l >= r,
        }
    }
}

/// Integer arithmetic on scalars. Only the two operations the real chains need
/// are admitted; a node earns its place by being load-bearing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
}

impl BinOp {
    fn tag(&self) -> u8 {
        match self {
            BinOp::Add => 1,
            BinOp::Sub => 2,
        }
    }

    fn eval(self, l: i64, r: i64) -> i64 {
        match self {
            // Saturating: a length cannot go negative, and an overflow cannot panic.
            BinOp::Add => l.saturating_add(r),
            BinOp::Sub => l.saturating_sub(r),
        }
    }
}

/// A declared output: its type and the value that carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Output {
    pub ty: Type,
    pub value: ValueId,
}

/// A typed, acyclic composition over sealed ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionIR {
    /// The qualified composition id, e.g. `phor:compose:toupper_memchr:…`.
    pub id: String,
    /// The locale contract the chain is defined under.
    pub locale_contract: String,
    pub inputs: Vec<Type>,
    pub outputs: Vec<Output>,
    pub nodes: Vec<Node>,
}

// ---------------------------------------------------------------------------
// Canonical encoding and identity
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Canon {
    buf: Vec<u8>,
}

impl Canon {
    fn new() -> Self {
        Canon { buf: Vec::new() }
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

/// The canonical byte encoding of an IR, **excluding** the domain tag. Node
/// order is significant (it is the data-flow order), so it is encoded verbatim.
pub fn canonical_bytes(ir: &CompositionIR) -> Vec<u8> {
    let mut c = Canon::new();
    c.str(&ir.id);
    c.str(&ir.locale_contract);
    c.u32(ir.inputs.len() as u32);
    for t in &ir.inputs {
        c.u8(t.tag());
    }
    c.u32(ir.outputs.len() as u32);
    for o in &ir.outputs {
        c.u8(o.ty.tag());
        c.u32(o.value.0);
    }
    c.u32(ir.nodes.len() as u32);
    for n in &ir.nodes {
        match n {
            Node::Input { index } => {
                c.u8(1);
                c.u32(*index);
            }
            Node::ConstantScalar { value } => {
                c.u8(2);
                c.i64(*value);
            }
            Node::Call { port, args } => {
                c.u8(3);
                c.str(port);
                c.u32(args.len() as u32);
                for a in args {
                    c.u32(a.0);
                }
            }
            Node::MapBytes { port, input } => {
                c.u8(4);
                c.str(port);
                c.u32(input.0);
            }
            Node::PackUsize { value } => {
                c.u8(8);
                c.u32(value.0);
            }
            Node::Observe { value } => {
                c.u8(10);
                c.u32(value.0);
            }
            Node::Slice {
                input,
                origin,
                length,
            } => {
                c.u8(5);
                c.u32(input.0);
                c.u32(origin.0);
                match length {
                    Some(l) => {
                        c.u8(1);
                        c.u32(l.0);
                    }
                    None => c.u8(0),
                }
            }
            Node::Compare { lhs, op, rhs } => {
                c.u8(6);
                c.u32(lhs.0);
                c.u8(op.tag());
                c.u32(rhs.0);
            }
            Node::Binary { op, lhs, rhs } => {
                c.u8(9);
                c.u8(op.tag());
                c.u32(lhs.0);
                c.u32(rhs.0);
            }
            Node::Select {
                condition,
                then_value,
                else_value,
            } => {
                c.u8(7);
                c.u32(condition.0);
                c.u32(then_value.0);
                c.u32(else_value.0);
            }
        }
    }
    c.buf
}

impl CompositionIR {
    /// The content identity: `SHA-256(PHOR/COMPOSITION-IR/v1\0 || canonical_bytes)`.
    pub fn id_hash(&self) -> String {
        let mut pre = Vec::with_capacity(COMPOSITION_IR_DOMAIN.len() + 512);
        pre.extend_from_slice(COMPOSITION_IR_DOMAIN);
        pre.extend_from_slice(&canonical_bytes(self));
        crate::porting::sha256_hex(&pre)
    }

    /// A human-readable projection. Never the authority; never hashed.
    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.composition_ir.v1\",\n  \"id\": \"{}\",\n  \"locale_contract\": \"{}\",\n  \"inputs\": {},\n  \"outputs\": {},\n  \"nodes\": {},\n  \"ir_hash\": \"{}\"\n}}\n",
            self.id,
            self.locale_contract,
            self.inputs.len(),
            self.outputs.len(),
            self.nodes.len(),
            self.id_hash()
        )
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Why an IR is not a well-formed composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrError {
    /// The node count exceeds the auditable bound.
    TooManyNodes {
        count: usize,
    },
    /// Too many inputs or outputs.
    TooManyInputs {
        count: usize,
    },
    TooManyOutputs {
        count: usize,
    },
    /// A tuple node exceeds the bound.
    TooManyOutputs1 {
        size: usize,
    },
    /// A value reference does not point at an earlier node (this is what makes the
    /// graph acyclic and the evaluation a single forward pass).
    ForwardReference {
        node: usize,
        value: u32,
    },
    /// A node's operand has the wrong type.
    TypeError {
        node: usize,
        expected: &'static str,
        found: &'static str,
    },
    /// An `Input` index is out of range.
    InputOutOfRange {
        node: usize,
        index: u32,
    },
    /// A port name is empty.
    EmptyPort {
        node: usize,
    },
}

/// Validate a graph: bounded, well-typed, acyclic by construction (every operand
/// must reference an earlier node). Returns the type of each node on success.
pub fn validate(ir: &CompositionIR) -> Result<Vec<Type>, IrError> {
    if ir.nodes.len() > MAX_NODES {
        return Err(IrError::TooManyNodes {
            count: ir.nodes.len(),
        });
    }
    if ir.inputs.len() > MAX_INPUTS {
        return Err(IrError::TooManyInputs {
            count: ir.inputs.len(),
        });
    }
    if ir.outputs.len() > MAX_OUTPUTS {
        return Err(IrError::TooManyOutputs {
            count: ir.outputs.len(),
        });
    }

    let mut types: Vec<Type> = Vec::with_capacity(ir.nodes.len());

    let ty_of = |types: &[Type], v: ValueId, node: usize| -> Result<Type, IrError> {
        let idx = v.0 as usize;
        if idx >= node {
            return Err(IrError::ForwardReference { node, value: v.0 });
        }
        Ok(types[idx])
    };

    for (i, node) in ir.nodes.iter().enumerate() {
        let t = match node {
            Node::Input { index } => {
                let idx = *index as usize;
                if idx >= ir.inputs.len() {
                    return Err(IrError::InputOutOfRange {
                        node: i,
                        index: *index,
                    });
                }
                ir.inputs[idx]
            }
            Node::ConstantScalar { .. } => Type::Scalar,
            Node::Call { port, args } => {
                if port.is_empty() {
                    return Err(IrError::EmptyPort { node: i });
                }
                for a in args {
                    // Operands may be any type; the backend marshals them.
                    ty_of(&types, *a, i)?;
                }
                // A call returns a scalar observable (index / sign / length / byte).
                Type::Scalar
            }
            Node::MapBytes { port, input } => {
                if port.is_empty() {
                    return Err(IrError::EmptyPort { node: i });
                }
                match ty_of(&types, *input, i)? {
                    Type::Bytes => Type::Bytes,
                    other => {
                        return Err(IrError::TypeError {
                            node: i,
                            expected: "bytes",
                            found: other.name(),
                        })
                    }
                }
            }
            Node::PackUsize { value } => match ty_of(&types, *value, i)? {
                Type::Scalar => Type::Bytes,
                other => {
                    return Err(IrError::TypeError {
                        node: i,
                        expected: "scalar",
                        found: other.name(),
                    })
                }
            },
            Node::Observe { value } => {
                // Any value may be observed; the node forces its evaluation.
                ty_of(&types, *value, i)?;
                Type::Bool
            }
            Node::Slice {
                input,
                origin,
                length,
            } => {
                match ty_of(&types, *input, i)? {
                    Type::Bytes => {}
                    other => {
                        return Err(IrError::TypeError {
                            node: i,
                            expected: "bytes",
                            found: other.name(),
                        })
                    }
                }
                match ty_of(&types, *origin, i)? {
                    Type::Scalar => {}
                    other => {
                        return Err(IrError::TypeError {
                            node: i,
                            expected: "scalar",
                            found: other.name(),
                        })
                    }
                }
                if let Some(l) = length {
                    match ty_of(&types, *l, i)? {
                        Type::Scalar => {}
                        other => {
                            return Err(IrError::TypeError {
                                node: i,
                                expected: "scalar",
                                found: other.name(),
                            })
                        }
                    }
                }
                Type::Bytes
            }
            Node::Compare { lhs, op: _, rhs } => {
                for v in [lhs, rhs] {
                    match ty_of(&types, *v, i)? {
                        Type::Scalar => {}
                        other => {
                            return Err(IrError::TypeError {
                                node: i,
                                expected: "scalar",
                                found: other.name(),
                            })
                        }
                    }
                }
                Type::Bool
            }
            Node::Binary { op: _, lhs, rhs } => {
                for v in [lhs, rhs] {
                    match ty_of(&types, *v, i)? {
                        Type::Scalar => {}
                        other => {
                            return Err(IrError::TypeError {
                                node: i,
                                expected: "scalar",
                                found: other.name(),
                            })
                        }
                    }
                }
                Type::Scalar
            }
            Node::Select {
                condition,
                then_value,
                else_value,
            } => {
                match ty_of(&types, *condition, i)? {
                    Type::Bool => {}
                    other => {
                        return Err(IrError::TypeError {
                            node: i,
                            expected: "bool",
                            found: other.name(),
                        })
                    }
                }
                let a = ty_of(&types, *then_value, i)?;
                let b = ty_of(&types, *else_value, i)?;
                if a != b {
                    return Err(IrError::TypeError {
                        node: i,
                        expected: a.name(),
                        found: b.name(),
                    });
                }
                a
            }
        };
        types.push(t);
    }

    for o in &ir.outputs {
        let idx = o.value.0 as usize;
        match types.get(idx) {
            None => {
                return Err(IrError::ForwardReference {
                    node: ir.nodes.len(),
                    value: o.value.0,
                })
            }
            Some(t) if *t != o.ty => {
                return Err(IrError::TypeError {
                    node: ir.nodes.len(),
                    expected: o.ty.name(),
                    found: t.name(),
                })
            }
            Some(_) => {}
        }
    }
    Ok(types)
}

// ---------------------------------------------------------------------------
// The generic interpreter
// ---------------------------------------------------------------------------

/// Identifies the IR node that made a call. A chain may call the same port from
/// more than one stage (the haystack fold and the needle fold both call
/// `toupper`), so per-stage accounting keys on the *site*, not the port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SiteId(pub usize);

/// The port backend the interpreter runs against. `ForeignBackend` routes to the
/// dialect cage (the oracle); `SealedBackend` routes through `NativeDispatcher`.
/// Because both implement the same trait and the *same* IR is evaluated on both,
/// the oracle implementation and the native implementation cannot drift.
pub trait PortBackend {
    /// Notify the backend that IR `site` was **reached** and is about to dispatch,
    /// even if it turns out to make no calls (a byte map over an empty buffer is
    /// vacuously native, not unreached). The default is a no-op.
    fn enter(&mut self, _site: SiteId) {}

    /// Call a port at IR `site` with typed arguments, returning its observable.
    fn call(&mut self, site: SiteId, port: &str, args: &[Value]) -> Result<Value, BackendError>;
}

/// How a backend call failed. The distinction is load-bearing: a foreign
/// fallback is not a broken seal, and neither is a refused call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendFailure {
    /// The port resolved to the foreign implementation instead of a sealed one.
    Fallback,
    /// A sealed entry existed but failed to load, verify, or execute.
    Broken,
    /// The backend could not answer (unknown port, malformed argument).
    Refused,
}

/// A backend refusal, tagged with its failure class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub kind: BackendFailure,
    pub message: String,
}

impl BackendError {
    pub fn new(m: impl Into<String>) -> Self {
        BackendError {
            kind: BackendFailure::Refused,
            message: m.into(),
        }
    }

    pub fn fallback(m: impl Into<String>) -> Self {
        BackendError {
            kind: BackendFailure::Fallback,
            message: m.into(),
        }
    }

    pub fn broken(m: impl Into<String>) -> Self {
        BackendError {
            kind: BackendFailure::Broken,
            message: m.into(),
        }
    }
}

/// Why evaluation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// The graph is ill-formed.
    Invalid(IrError),
    /// A value had the wrong runtime type.
    RuntimeType { node: usize, expected: &'static str },
    /// A slice went out of bounds.
    SliceOutOfBounds { node: usize },
    /// A backend call failed, carrying the failure class.
    Backend {
        node: usize,
        kind: BackendFailure,
        error: String,
    },
    /// The evaluation recursion exceeded the bound.
    DepthExceeded,
    /// An output reference was out of range.
    OutputOutOfRange,
}

/// Per-node evaluation state, so a shared value is evaluated once and a cycle is
/// rejected rather than recursed into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Idle,
    InProgress,
    Done,
}

fn expect_bytes(v: Value, node: usize) -> Result<Vec<u8>, EvalError> {
    match v {
        Value::Bytes(b) => Ok(b),
        _ => Err(EvalError::RuntimeType {
            node,
            expected: "bytes",
        }),
    }
}

fn expect_scalar(v: Value, node: usize) -> Result<i64, EvalError> {
    match v {
        Value::Scalar(s) => Ok(s),
        _ => Err(EvalError::RuntimeType {
            node,
            expected: "scalar",
        }),
    }
}

fn expect_bool(v: Value, node: usize) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(b),
        _ => Err(EvalError::RuntimeType {
            node,
            expected: "bool",
        }),
    }
}

/// Evaluate a validated IR against `inputs`, returning the declared outputs.
///
/// Evaluation is **lazy**: only nodes reachable from an output through the taken
/// branch of each `Select` are evaluated. That is what makes a data-dependent
/// *non-execution* real rather than nominal — a stage on the untaken branch is
/// never dispatched, so "a stage did not run" can never be confused with "a stage
/// succeeded". The graph is acyclic by validation; a shared node is evaluated once
/// (memoized) so no port is called twice for one logical value.
pub fn eval(
    ir: &CompositionIR,
    backend: &mut dyn PortBackend,
    inputs: &[Value],
) -> Result<Vec<Value>, EvalError> {
    validate(ir).map_err(EvalError::Invalid)?;
    if inputs.len() != ir.inputs.len() {
        return Err(EvalError::RuntimeType {
            node: 0,
            expected: "declared input arity",
        });
    }

    let n = ir.nodes.len();
    let mut cache: Vec<Option<Value>> = alloc::vec![None; n];
    let mut visit: Vec<Visit> = alloc::vec![Visit::Idle; n];

    let mut out = Vec::with_capacity(ir.outputs.len());
    for o in &ir.outputs {
        let idx = o.value.0 as usize;
        if idx >= n {
            return Err(EvalError::OutputOutOfRange);
        }
        out.push(eval_node(
            ir, backend, inputs, &mut cache, &mut visit, idx, 0,
        )?);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn eval_node(
    ir: &CompositionIR,
    backend: &mut dyn PortBackend,
    inputs: &[Value],
    cache: &mut [Option<Value>],
    visit: &mut [Visit],
    i: usize,
    depth: usize,
) -> Result<Value, EvalError> {
    if depth > MAX_DEPTH {
        return Err(EvalError::DepthExceeded);
    }
    match visit[i] {
        Visit::Done => return Ok(cache[i].clone().expect("Done implies cached")),
        Visit::InProgress => {
            return Err(EvalError::Invalid(IrError::ForwardReference {
                node: i,
                value: i as u32,
            }))
        }
        Visit::Idle => {}
    }
    visit[i] = Visit::InProgress;

    let node = &ir.nodes[i];
    let v = match node {
        Node::Input { index } => inputs[*index as usize].clone(),
        Node::ConstantScalar { value } => Value::Scalar(*value),
        Node::Call { port, args } => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    a.0 as usize,
                    depth + 1,
                )?);
            }
            backend.enter(SiteId(i));
            backend
                .call(SiteId(i), port, &vals)
                .map_err(|e| EvalError::Backend {
                    node: i,
                    kind: e.kind,
                    error: e.message,
                })?
        }
        Node::MapBytes { port, input } => {
            let bytes = expect_bytes(
                eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    input.0 as usize,
                    depth + 1,
                )?,
                i,
            )?;
            backend.enter(SiteId(i));
            let mut out = Vec::with_capacity(bytes.len());
            for b in bytes {
                // The unit port is called with a one-byte buffer, matching the court
                // ABI (`toupper` takes `[byte]`), not a packed scalar.
                let r = backend
                    .call(SiteId(i), port, &[Value::Bytes(alloc::vec![b])])
                    .map_err(|e| EvalError::Backend {
                        node: i,
                        kind: e.kind,
                        error: e.message,
                    })?;
                match r {
                    Value::Bytes(v) => out.push(v.first().copied().unwrap_or(b)),
                    _ => {
                        return Err(EvalError::RuntimeType {
                            node: i,
                            expected: "bytes byte",
                        })
                    }
                }
            }
            Value::Bytes(out)
        }
        Node::PackUsize { value } => {
            let s = expect_scalar(
                eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    value.0 as usize,
                    depth + 1,
                )?,
                i,
            )?;
            // A negative length or index cannot be a legitimate argument; encode it
            // as 0 rather than wrapping to an enormous u64.
            Value::Bytes((s.max(0) as u64).to_le_bytes().to_vec())
        }
        Node::Observe { value } => {
            // Force the observed value's evaluation (and hence its dispatch), then
            // yield a non-observable effect marker.
            eval_node(
                ir,
                backend,
                inputs,
                cache,
                visit,
                value.0 as usize,
                depth + 1,
            )?;
            Value::Bool(true)
        }
        Node::Slice {
            input,
            origin,
            length,
        } => {
            let bytes = expect_bytes(
                eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    input.0 as usize,
                    depth + 1,
                )?,
                i,
            )?;
            let o = expect_scalar(
                eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    origin.0 as usize,
                    depth + 1,
                )?,
                i,
            )?;
            let o = if o < 0 { 0usize } else { o as usize };
            let end = match length {
                Some(l) => {
                    let len = expect_scalar(
                        eval_node(ir, backend, inputs, cache, visit, l.0 as usize, depth + 1)?,
                        i,
                    )?;
                    let len = if len < 0 { 0usize } else { len as usize };
                    o.saturating_add(len)
                }
                None => bytes.len(),
            };
            let end = end.min(bytes.len());
            if o > bytes.len() {
                return Err(EvalError::SliceOutOfBounds { node: i });
            }
            Value::Bytes(bytes[o..end].to_vec())
        }
        Node::Compare { lhs, op, rhs } => {
            let l = expect_scalar(
                eval_node(ir, backend, inputs, cache, visit, lhs.0 as usize, depth + 1)?,
                i,
            )?;
            let r = expect_scalar(
                eval_node(ir, backend, inputs, cache, visit, rhs.0 as usize, depth + 1)?,
                i,
            )?;
            Value::Bool(op.eval(l, r))
        }
        Node::Binary { op, lhs, rhs } => {
            let l = expect_scalar(
                eval_node(ir, backend, inputs, cache, visit, lhs.0 as usize, depth + 1)?,
                i,
            )?;
            let r = expect_scalar(
                eval_node(ir, backend, inputs, cache, visit, rhs.0 as usize, depth + 1)?,
                i,
            )?;
            Value::Scalar(op.eval(l, r))
        }
        Node::Select {
            condition,
            then_value,
            else_value,
        } => {
            let c = expect_bool(
                eval_node(
                    ir,
                    backend,
                    inputs,
                    cache,
                    visit,
                    condition.0 as usize,
                    depth + 1,
                )?,
                i,
            )?;
            // Only the taken branch is evaluated: the other branch (and everything
            // it depends on) is never dispatched.
            let taken = if c { *then_value } else { *else_value };
            eval_node(
                ir,
                backend,
                inputs,
                cache,
                visit,
                taken.0 as usize,
                depth + 1,
            )?
        }
    };

    cache[i] = Some(v.clone());
    visit[i] = Visit::Done;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::{LIBC_MEMCHR, LIBC_TOUPPER};

    fn simple_ir() -> CompositionIR {
        CompositionIR {
            id: String::from("phor:compose:test:v1"),
            locale_contract: String::from("C"),
            inputs: alloc::vec![Type::Bytes, Type::Scalar],
            outputs: alloc::vec![Output {
                ty: Type::Bytes,
                value: ValueId(3),
            }],
            nodes: alloc::vec![
                // 0: input 0 (bytes)
                Node::Input { index: 0 },
                // 1: input 1 (scalar n)
                Node::Input { index: 1 },
                // 2: origin 0
                Node::ConstantScalar { value: 0 },
                // 3: slice the first n bytes
                Node::Slice {
                    input: ValueId(0),
                    origin: ValueId(2),
                    length: Some(ValueId(1)),
                },
            ],
        }
    }

    struct UpperBackend;
    impl PortBackend for UpperBackend {
        fn call(
            &mut self,
            _site: SiteId,
            port: &str,
            args: &[Value],
        ) -> Result<Value, BackendError> {
            if port == LIBC_TOUPPER.id {
                let b = args[0]
                    .as_bytes()
                    .and_then(|b| b.first())
                    .copied()
                    .unwrap_or(0);
                Ok(Value::Bytes(alloc::vec![b.to_ascii_uppercase()]))
            } else {
                Err(BackendError::new("unknown port"))
            }
        }
    }

    #[test]
    fn test_validate_accepts_a_well_formed_graph() {
        assert!(validate(&simple_ir()).is_ok());
    }

    #[test]
    fn test_forward_reference_is_rejected() {
        let mut ir = simple_ir();
        ir.nodes[0] = Node::Slice {
            input: ValueId(3),
            origin: ValueId(2),
            length: None,
        };
        assert!(matches!(
            validate(&ir),
            Err(IrError::ForwardReference { .. })
        ));
    }

    #[test]
    fn test_type_error_is_rejected() {
        let mut ir = simple_ir();
        // MapBytes needs a Bytes input, but ValueId(1) is a Scalar.
        ir.nodes.push(Node::MapBytes {
            port: String::from("p"),
            input: ValueId(1),
        });
        assert!(matches!(validate(&ir), Err(IrError::TypeError { .. })));
    }

    #[test]
    fn test_input_out_of_range_is_rejected() {
        let mut ir = simple_ir();
        ir.nodes[0] = Node::Input { index: 7 };
        assert!(matches!(
            validate(&ir),
            Err(IrError::InputOutOfRange { .. })
        ));
    }

    #[test]
    fn test_node_bound_is_enforced() {
        let mut ir = simple_ir();
        for _ in 0..(MAX_NODES + 1) {
            ir.nodes.push(Node::ConstantScalar { value: 0 });
        }
        assert!(matches!(validate(&ir), Err(IrError::TooManyNodes { .. })));
    }

    #[test]
    fn test_ir_hash_is_stable_and_sensitive() {
        let a = simple_ir();
        assert_eq!(a.id_hash(), simple_ir().id_hash());
        assert_eq!(a.id_hash().len(), 64);

        let mut b = simple_ir();
        b.nodes.push(Node::ConstantScalar { value: 1 });
        assert_ne!(a.id_hash(), b.id_hash());

        // A different input arity changes the identity.
        let mut c = simple_ir();
        c.inputs.push(Type::Bool);
        assert_ne!(a.id_hash(), c.id_hash());

        // Node order is significant.
        let mut d = simple_ir();
        d.nodes.swap(1, 2);
        assert_ne!(a.id_hash(), d.id_hash());
    }

    #[test]
    fn test_interpreter_evaluates_a_slice() {
        let ir = simple_ir();
        let mut b = UpperBackend;
        let out = eval(
            &ir,
            &mut b,
            &[
                Value::Bytes(alloc::vec![0x61, 0x62, 0x63]),
                Value::Scalar(0),
            ],
        )
        .unwrap();
        // origin 0, length n = 0 -> empty
        assert_eq!(out, alloc::vec![Value::Bytes(alloc::vec![])]);

        let out = eval(
            &ir,
            &mut b,
            &[
                Value::Bytes(alloc::vec![0x61, 0x62, 0x63]),
                Value::Scalar(2),
            ],
        )
        .unwrap();
        assert_eq!(out, alloc::vec![Value::Bytes(alloc::vec![0x61, 0x62])]);

        // A length beyond the buffer clamps to the buffer end.
        let out = eval(
            &ir,
            &mut b,
            &[
                Value::Bytes(alloc::vec![0x61, 0x62, 0x63]),
                Value::Scalar(9),
            ],
        )
        .unwrap();
        assert_eq!(
            out,
            alloc::vec![Value::Bytes(alloc::vec![0x61, 0x62, 0x63])]
        );
    }

    /// The IR can express the first composition (`toupper ∘ memchr`) and, run
    /// through a backend backed by the clean-room mirrors, reproduce the foreign
    /// oracle over the whole corpus. This is the interpreter equivalence check.
    #[test]
    fn test_ir_reproduces_the_toupper_memchr_oracle() {
        use crate::porting::candidate::{phor_memchr, phor_toupper};
        use crate::porting::dialect_cage;

        // toupper_memchr: (haystack, needle, n) -> index
        let ir = CompositionIR {
            id: String::from("phor:compose:toupper_memchr:c-locale:index:v1"),
            locale_contract: String::from("C"),
            inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Scalar],
            outputs: alloc::vec![Output {
                ty: Type::Scalar,
                value: ValueId(8),
            }],
            nodes: alloc::vec![
                Node::Input { index: 0 }, // 0 haystack
                Node::Input { index: 1 }, // 1 needle (one byte)
                Node::Input { index: 2 }, // 2 n
                // 3: fold the whole haystack with toupper
                Node::MapBytes {
                    port: String::from(LIBC_TOUPPER.id),
                    input: ValueId(0),
                },
                // 4: fold the needle with the same port
                Node::MapBytes {
                    port: String::from(LIBC_TOUPPER.id),
                    input: ValueId(1),
                },
                // 5: pack n as the search bound
                Node::PackUsize { value: ValueId(2) },
                // 6: search the folded haystack for the folded needle
                Node::Call {
                    port: String::from(LIBC_MEMCHR.id),
                    args: alloc::vec![ValueId(3), ValueId(4), ValueId(5)],
                },
            ],
        };
        // The output reference is the memchr call.
        let mut ir = ir;
        ir.outputs[0].value = ValueId(6);

        struct MirrorBackend;
        impl PortBackend for MirrorBackend {
            fn call(
                &mut self,
                _site: SiteId,
                port: &str,
                args: &[Value],
            ) -> Result<Value, BackendError> {
                if port == LIBC_TOUPPER.id {
                    let b = args[0]
                        .as_bytes()
                        .and_then(|b| b.first())
                        .copied()
                        .unwrap_or(0);
                    Ok(Value::Bytes(alloc::vec![phor_toupper(b)]))
                } else if port == LIBC_MEMCHR.id {
                    let hay = args[0].as_bytes().ok_or(BackendError::new("bytes"))?;
                    let needle = args[1]
                        .as_bytes()
                        .and_then(|b| b.first())
                        .copied()
                        .unwrap_or(0);
                    let n = args[2]
                        .as_bytes()
                        .and_then(|b| b.first())
                        .copied()
                        .unwrap_or(0) as usize;
                    Ok(Value::Scalar(phor_memchr(hay, needle, n) as i64))
                } else {
                    Err(BackendError::new("unknown port"))
                }
            }
        }

        let cases = crate::porting::composition::composition_corpus();
        let traces =
            dialect_cage::observe_composition(&cases, &crate::porting::PortingAuthority::granted())
                .unwrap();
        assert!(!traces.is_empty());

        let mut backend = MirrorBackend;
        for (case, trace) in cases.iter().zip(traces.iter()) {
            let hay = case.args[0].clone();
            let needle = alloc::vec![case.args[1][0]];
            let n = crate::porting::candidate::decode_usize(&case.args[2]);
            let out = eval(
                &ir,
                &mut backend,
                &[
                    Value::Bytes(hay),
                    Value::Bytes(needle),
                    Value::Scalar(n as i64),
                ],
            )
            .unwrap();
            let got = out[0].as_scalar().unwrap() as i32;
            let want =
                crate::porting::candidate::decode_index(&hex::decode(&trace.output_hex).unwrap());
            assert_eq!(got, want, "case {} diverged from the oracle", case.case_id);
        }
    }

    /// A `Select` on an untaken branch must not evaluate that branch, so a stage
    /// that should not run is never dispatched. This is the interpreter property
    /// that makes data-dependent non-execution real.
    #[test]
    fn test_select_is_lazy_and_does_not_evaluate_the_untaken_branch() {
        use core::cell::Cell;

        // (condition_scalar) -> scalar: if cond >= 0 { call(a) } else { -1 }
        let ir = CompositionIR {
            id: String::from("phor:compose:lazy-test:v1"),
            locale_contract: String::from("C"),
            inputs: alloc::vec![Type::Scalar],
            outputs: alloc::vec![Output {
                ty: Type::Scalar,
                value: ValueId(5),
            }],
            nodes: alloc::vec![
                Node::Input { index: 0 },          // 0 cond
                Node::ConstantScalar { value: 0 }, // 1 zero
                Node::Compare {
                    lhs: ValueId(0),
                    op: CompareOp::Ge,
                    rhs: ValueId(1),
                }, // 2 cond >= 0
                Node::Call {
                    port: String::from("counting"),
                    args: alloc::vec![ValueId(0)],
                }, // 3 the side-effecting stage
                Node::ConstantScalar { value: -1 }, // 4 else value
                Node::Select {
                    condition: ValueId(2),
                    then_value: ValueId(3),
                    else_value: ValueId(4),
                }, // 5 result
            ],
        };

        struct CountingBackend<'a>(&'a Cell<u64>);
        impl PortBackend for CountingBackend<'_> {
            fn call(
                &mut self,
                _site: SiteId,
                _port: &str,
                _args: &[Value],
            ) -> Result<Value, BackendError> {
                self.0.set(self.0.get() + 1);
                Ok(Value::Scalar(7))
            }
        }

        let calls = Cell::new(0u64);
        let mut b = CountingBackend(&calls);
        let out = eval(&ir, &mut b, &[Value::Scalar(-1)]).unwrap();
        assert_eq!(out, alloc::vec![Value::Scalar(-1)]);
        assert_eq!(calls.get(), 0, "the untaken branch must not be dispatched");

        let out = eval(&ir, &mut b, &[Value::Scalar(3)]).unwrap();
        assert_eq!(out, alloc::vec![Value::Scalar(7)]);
        assert_eq!(calls.get(), 1, "the taken branch must be dispatched once");
    }

    /// A shared node is memoized: a fan-in value dispatches once, not once per
    /// consumer.
    #[test]
    fn test_a_shared_node_is_evaluated_once() {
        use core::cell::Cell;

        // (n) -> (call(n), call(n)) as a tuple of two outputs sharing one node.
        let ir = CompositionIR {
            id: String::from("phor:compose:share-test:v1"),
            locale_contract: String::from("C"),
            inputs: alloc::vec![Type::Scalar],
            outputs: alloc::vec![
                Output {
                    ty: Type::Scalar,
                    value: ValueId(2),
                },
                Output {
                    ty: Type::Scalar,
                    value: ValueId(2),
                },
            ],
            nodes: alloc::vec![
                Node::Input { index: 0 },
                Node::PackUsize { value: ValueId(0) },
                Node::Call {
                    port: String::from("counting"),
                    args: alloc::vec![ValueId(1)],
                },
            ],
        };

        struct CountingBackend<'a>(&'a Cell<u64>);
        impl PortBackend for CountingBackend<'_> {
            fn call(
                &mut self,
                _site: SiteId,
                _port: &str,
                _args: &[Value],
            ) -> Result<Value, BackendError> {
                self.0.set(self.0.get() + 1);
                Ok(Value::Scalar(1))
            }
        }

        let calls = Cell::new(0u64);
        let mut b = CountingBackend(&calls);
        let out = eval(&ir, &mut b, &[Value::Scalar(4)]).unwrap();
        assert_eq!(out, alloc::vec![Value::Scalar(1), Value::Scalar(1)]);
        assert_eq!(calls.get(), 1, "a shared node must be evaluated once");
    }
}
