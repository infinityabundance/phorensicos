// Phorc IR — Phorensic Intermediate Representation (PHIR)
// Lowered, flat representation between AST check and codegen.

use crate::ast::{Effect, PrimType};

// ============================================================================
// PHIR Types
// ============================================================================

/// PHIR type representation (simplified from AST TypeExpr)
#[derive(Debug, Clone, PartialEq)]
pub enum PhirType {
    Prim(PrimType),
    Ptr(Box<PhirType>),
    Array(Box<PhirType>, u64),
    Struct(String, Vec<PhirType>),
    Cap(Box<PhirType>),
    Handle(Box<PhirType>),
    FnPtr,
    Void,
    Never,
}

/// A PHIR value — the result of an expression
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Const(Constant),
    Temp(usize, PhirType),
    Arg(usize, PhirType),
    Global(String, PhirType),
    /// A stack-allocated local variable slot (index into current function's locals)
    Local(usize, PhirType),
}

/// Constants in PHIR
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(u64, usize),      // value, bit width
    Float(u64, usize),    // bits, width
    Bool(bool),
    String(Vec<u8>),
    Null,
}

/// PHIR operations (machine-independent)
#[derive(Debug, Clone)]
pub enum Op {
    /// No-op
    Nop,
    /// Integer binary operation
    BinOp {
        op: BinOpKind,
        lhs: Value,
        rhs: Value,
        ty: PhirType,
        out: Value,
    },
    /// Unary operation
    UnOp {
        op: UnOpKind,
        val: Value,
        ty: PhirType,
        out: Value,
    },
    /// Load from memory
    Load {
        addr: Value,
        ty: PhirType,
        out: Value,
    },
    /// Store to memory
    Store {
        addr: Value,
        val: Value,
    },
    /// Move a capability
    CapMove {
        src: Value,
        dst: Value,
    },
    /// Create a handle
    HandleCreate {
        obj: Value,
        out: Value,
    },
    /// Validate a handle
    HandleValidate {
        handle: Value,
        expected_gen: Value,
        out: Value,
    },
    /// Emit a residual record
    ResidualEmit {
        operation: String,
        fields: Vec<(String, Value)>,
    },
    /// Cast a value
    Cast {
        val: Value,
        to: PhirType,
        out: Value,
    },
    /// Call a function
    Call {
        callee: String,
        args: Vec<Value>,
        effects: Vec<Effect>,
        out: Value,
    },
    /// Call via function pointer
    CallIndirect {
        callee: Value,
        args: Vec<Value>,
        out: Value,
    },
    /// Return from function
    Return(Option<Value>),
    /// Conditional branch
    Branch {
        cond: Value,
        true_block: usize,
        false_block: usize,
    },
    /// Unconditional jump
    Jump(usize),
    /// Phi node (SSA)
    Phi {
        incoming: Vec<(Value, usize)>, // (value, from_block)
        out: Value,
    },
}

/// Binary operation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    Add, Sub, Mul, Div, Rem,
    And, Or, Xor,
    Shl, Shr,
    Eq, Ne, Lt, Le, Gt, Ge,
}

/// Unary operation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOpKind {
    Neg, Not,
}

/// A basic block in PHIR
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub ops: Vec<Op>,
    pub predecessors: Vec<usize>,
}

/// A PHIR function
#[derive(Debug, Clone)]
pub struct PhirFunction {
    pub name: String,
    pub params: Vec<(String, PhirType)>,
    pub return_type: PhirType,
    pub effects: Vec<Effect>,
    pub blocks: Vec<BasicBlock>,
    pub entry_block: usize,
    /// Number of stack-allocated local variable slots
    pub local_count: usize,
}

/// A PHIR module (compilation unit)
#[derive(Debug, Clone)]
pub struct PhirModule {
    pub functions: Vec<PhirFunction>,
    pub globals: Vec<(String, PhirType, Option<Constant>)>,
    pub source_name: String,
}

// ============================================================================
// Byte Attribution (provenance tracking)
// ============================================================================

/// Maps emitted bytes back to their source
#[derive(Debug, Clone)]
pub struct ByteAttribution {
    pub section: String,
    pub offset: u64,
    pub len: u64,
    pub source_span: Option<(String, usize, usize)>, // file, line, col
    pub phir_node: Option<String>,
    pub lowering_rule: Option<String>,
    pub abi_rule: Option<String>,
    pub instruction: Option<String>,
    pub relocation: Option<String>,
    pub receipt_hash: Option<[u8; 32]>,
}

/// A complete receipt for a compiled artifact
#[derive(Debug, Clone)]
pub struct CompileReceipt {
    pub source_file: String,
    pub object_file: String,
    pub function_receipts: Vec<FunctionReceipt>,
    pub byte_map: Vec<ByteAttribution>,
}

#[derive(Debug, Clone)]
pub struct FunctionReceipt {
    pub name: String,
    pub source_span: (usize, usize),
    pub byte_offset: u64,
    pub byte_len: u64,
    pub effect_set: Vec<Effect>,
    pub verified: bool,
}

/// ABI manifest for a compiled module
#[derive(Debug, Clone)]
pub struct AbiManifest {
    pub functions: Vec<AbiFunction>,
    pub globals: Vec<AbiGlobal>,
}

#[derive(Debug, Clone)]
pub struct AbiFunction {
    pub name: String,
    pub linkage: AbiLinkage,
    pub calling_convention: String,
    pub stack_frame_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiLinkage {
    Global,
    Local,
    Extern,
}

#[derive(Debug, Clone)]
pub struct AbiGlobal {
    pub name: String,
    pub size: u64,
    pub alignment: u64,
    pub section: String,
}
