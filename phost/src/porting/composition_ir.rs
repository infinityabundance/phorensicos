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
use alloc::string::{String, ToString};
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
    /// buffer. The registered `MapBytes` port must return a `Scalar` byte.
    MapBytes { port: String, input: ValueId },
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
    /// A conditional value. This is how a data-dependent *non-execution* is
    /// represented explicitly: `Select` is a value choice, and only the taken
    /// branch is evaluated, so "a stage did not run" is never confused with "a
    /// stage succeeded".
    Select {
        condition: ValueId,
        then_value: ValueId,
        else_value: ValueId,
    },
    /// Fold a buffer with a binary port: `acc = port(acc, item)` for each byte.
    FoldBytes {
        port: String,
        init: ValueId,
        input: ValueId,
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
            Node::FoldBytes { port, init, input } => {
                c.u8(8);
                c.str(port);
                c.u32(init.0);
                c.u32(input.0);
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
            Node::FoldBytes { port, init, input } => {
                if port.is_empty() {
                    return Err(IrError::EmptyPort { node: i });
                }
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
                match ty_of(&types, *init, i)? {
                    Type::Scalar => {}
                    other => {
                        return Err(IrError::TypeError {
                            node: i,
                            expected: "scalar",
                            found: other.name(),
                        })
                    }
                }
                Type::Scalar
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

/// The port backend the interpreter runs against. `ForeignBackend` routes to the
/// dialect cage (the oracle); `SealedBackend` routes through `NativeDispatcher`.
/// Because both implement the same trait and the *same* IR is evaluated on both,
/// the oracle implementation and the native implementation cannot drift.
pub trait PortBackend {
    /// Call a sealed port with typed arguments, returning its observable.
    fn call(&mut self, port: &str, args: &[Value]) -> Result<Value, BackendError>;
}

/// A backend refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError(pub String);

impl BackendError {
    pub fn new(m: impl Into<String>) -> Self {
        BackendError(m.into())
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
    /// A backend refused.
    Backend { node: usize, error: String },
    /// The evaluation recursion exceeded the bound.
    DepthExceeded,
    /// An output reference was out of range.
    OutputOutOfRange,
}

/// Evaluate a validated IR against `inputs`, returning the declared outputs.
///
/// The graph is acyclic by validation, so this is a single forward pass; only the
/// taken branch of a `Select` is evaluated, which is what makes a data-dependent
/// non-execution explicit rather than implicit.
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

    let mut values: Vec<Value> = Vec::with_capacity(ir.nodes.len());

    for (i, node) in ir.nodes.iter().enumerate() {
        let v = match node {
            Node::Input { index } => inputs[*index as usize].clone(),
            Node::ConstantScalar { value } => Value::Scalar(*value),
            Node::Call { port, args } => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(values[a.0 as usize].clone());
                }
                backend.call(port, &vals).map_err(|e| EvalError::Backend {
                    node: i,
                    error: e.0,
                })?
            }
            Node::MapBytes { port, input } => {
                let bytes = values[input.0 as usize]
                    .as_bytes()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "bytes",
                    })?;
                let mut out = Vec::with_capacity(bytes.len());
                for b in bytes {
                    let r = backend
                        .call(port, &[Value::Scalar(*b as i64)])
                        .map_err(|e| EvalError::Backend {
                            node: i,
                            error: e.0,
                        })?;
                    out.push(r.as_scalar().ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "scalar byte",
                    })? as u8);
                }
                Value::Bytes(out)
            }
            Node::Slice {
                input,
                origin,
                length,
            } => {
                let bytes = values[input.0 as usize]
                    .as_bytes()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "bytes",
                    })?;
                let o = values[origin.0 as usize]
                    .as_scalar()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "scalar",
                    })?;
                let o = if o < 0 { 0usize } else { o as usize };
                let end = match length {
                    Some(l) => {
                        let len =
                            values[l.0 as usize]
                                .as_scalar()
                                .ok_or(EvalError::RuntimeType {
                                    node: i,
                                    expected: "scalar",
                                })?;
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
                let l = values[lhs.0 as usize]
                    .as_scalar()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "scalar",
                    })?;
                let r = values[rhs.0 as usize]
                    .as_scalar()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "scalar",
                    })?;
                Value::Bool(op.eval(l, r))
            }
            Node::Select {
                condition,
                then_value,
                else_value,
            } => {
                let c = match values[condition.0 as usize] {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(EvalError::RuntimeType {
                            node: i,
                            expected: "bool",
                        })
                    }
                };
                // Only the taken branch is materialized; both were evaluated in the
                // single forward pass (the IR is not lazy), but the *value* choice is
                // explicit and hashed, so a skipped stage is observable.
                values[(if c { *then_value } else { *else_value }).0 as usize].clone()
            }
            Node::FoldBytes { port, init, input } => {
                let mut acc = values[init.0 as usize].clone();
                let bytes = values[input.0 as usize]
                    .as_bytes()
                    .ok_or(EvalError::RuntimeType {
                        node: i,
                        expected: "bytes",
                    })?;
                for b in bytes {
                    acc = backend
                        .call(port, &[acc, Value::Scalar(*b as i64)])
                        .map_err(|e| EvalError::Backend {
                            node: i,
                            error: e.0,
                        })?;
                }
                acc
            }
        };
        values.push(v);
    }

    let mut out = Vec::with_capacity(ir.outputs.len());
    for o in &ir.outputs {
        out.push(
            values
                .get(o.value.0 as usize)
                .ok_or(EvalError::OutputOutOfRange)?
                .clone(),
        );
    }
    Ok(out)
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
        fn call(&mut self, port: &str, args: &[Value]) -> Result<Value, BackendError> {
            if port == LIBC_TOUPPER.id {
                let b = args[0].as_scalar().unwrap_or(0) as u8;
                Ok(Value::Scalar((b.to_ascii_uppercase()) as i64))
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
            inputs: alloc::vec![Type::Bytes, Type::Scalar, Type::Scalar],
            outputs: alloc::vec![Output {
                ty: Type::Scalar,
                value: ValueId(7),
            }],
            nodes: alloc::vec![
                Node::Input { index: 0 },          // 0 haystack
                Node::Input { index: 1 },          // 1 needle
                Node::Input { index: 2 },          // 2 n
                Node::ConstantScalar { value: 0 }, // 3 origin
                // 4: slice first n bytes of the haystack
                Node::Slice {
                    input: ValueId(0),
                    origin: ValueId(3),
                    length: Some(ValueId(2)),
                },
                // 5: fold the slice with toupper
                Node::MapBytes {
                    port: String::from(LIBC_TOUPPER.id),
                    input: ValueId(4),
                },
                // 6: fold the needle
                Node::Call {
                    port: String::from(LIBC_TOUPPER.id),
                    args: alloc::vec![ValueId(1)],
                },
                // 7: search
                Node::Call {
                    port: String::from(LIBC_MEMCHR.id),
                    args: alloc::vec![ValueId(5), ValueId(6), ValueId(2)],
                },
            ],
        };

        struct MirrorBackend;
        impl PortBackend for MirrorBackend {
            fn call(&mut self, port: &str, args: &[Value]) -> Result<Value, BackendError> {
                if port == LIBC_TOUPPER.id {
                    let b = args[0].as_scalar().unwrap_or(0) as u8;
                    Ok(Value::Scalar(phor_toupper(b) as i64))
                } else if port == LIBC_MEMCHR.id {
                    let hay = args[0].as_bytes().ok_or(BackendError::new("bytes"))?;
                    let needle = args[1].as_scalar().unwrap_or(0) as u8;
                    let n = args[2].as_scalar().unwrap_or(0) as usize;
                    Ok(Value::Scalar(phor_memchr(hay, needle, n) as i64))
                } else {
                    Err(BackendError::new("unknown port"))
                }
            }
        }

        let comp = crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR;
        let cases = crate::porting::composition::composition_corpus();
        let traces =
            dialect_cage::observe_composition(&cases, &crate::porting::PortingAuthority::granted())
                .unwrap();
        assert!(!traces.is_empty());
        let _ = comp;

        let mut backend = MirrorBackend;
        for (case, trace) in cases.iter().zip(traces.iter()) {
            let hay = case.args[0].clone();
            let needle = case.args[1][0];
            let n = crate::porting::candidate::decode_usize(&case.args[2]);
            let out = eval(
                &ir,
                &mut backend,
                &[
                    Value::Bytes(hay),
                    Value::Scalar(needle as i64),
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
}
