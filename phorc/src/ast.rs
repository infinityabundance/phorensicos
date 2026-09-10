// Phorc — Phorensic Bootstrap Compiler
// AST definitions — PHORENSIC_LANGUAGE.md §3, §4, §5

use crate::lex::Loc;

/// Visibility modifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    Pub,     // fully public
    Private, // private (default)
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Private
    }
}

/// A strongly-typed identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    pub name: String,
    pub loc: Loc,
}

/// Literal values
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(u64, Loc),
    Float(f64, Loc),
    Bool(bool, Loc),
    Char(char, Loc),
    Str(String, Loc),
}

/// Array/ringbuf/str size — literal or constant identifier
#[derive(Debug, Clone, PartialEq)]
pub enum ArraySize {
    Literal(u64, Loc),
    ConstIdent(String, Loc),
}

impl TypeExpr {
    /// Extract the type name from a type expression (e.g., "Str" from Str(64), "Array" from Array(u64, 10)).
    /// Returns None for Prim types, tuples, and unnamed types.
    pub fn type_name(&self) -> Option<String> {
        match self {
            TypeExpr::Named(ident) => Some(ident.name.clone()),
            TypeExpr::Str(_, _) => Some("Str".to_string()),
            TypeExpr::Array(_, _, _) => Some("Array".to_string()),
            TypeExpr::RingBuf(_, _, _) => Some("RingBuf".to_string()),
            TypeExpr::Slice(_, _) => Some("Slice".to_string()),
            TypeExpr::Cap(_, _) => Some("Cap".to_string()),
            TypeExpr::Handle(_, _) => Some("Handle".to_string()),
            TypeExpr::Option(_, _) => Some("Option".to_string()),
            TypeExpr::Result(_, _, _) => Some("Result".to_string()),
            _ => None,
        }
    }
}

/// Primitive types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrimType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    Char,
    Usize,
    Isize,
    Void,
    Never,
}

/// Type expressions
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Prim(PrimType, Loc),
    Named(Ident),                           // user-defined type
    Array(Box<TypeExpr>, ArraySize, Loc),   // Array(T, N)
    RingBuf(Box<TypeExpr>, ArraySize, Loc), // RingBuf(T, N)
    Slice(Box<TypeExpr>, Loc),              // Slice(T)
    Str(ArraySize, Loc),                    // Str(N)
    Cap(Box<TypeExpr>, Loc),                // Cap(T)
    Handle(Box<TypeExpr>, Loc),             // Handle(T)
    Effect(Loc),                            // Effect (token type)
    Provenance(Loc),
    Receipt(Loc),
    TrustState(Loc),
    ResidualTy(Loc),
    FnType(Box<FnTypeSig>, Loc), // function type
    Tuple(Vec<TypeExpr>, Loc),
    Option(Box<TypeExpr>, Loc),                // Option(T)
    Result(Box<TypeExpr>, Box<TypeExpr>, Loc), // Result(T, E)
    Never(Loc),
}

/// Function type signature
#[derive(Debug, Clone, PartialEq)]
pub struct FnTypeSig {
    pub params: Vec<(Ident, TypeExpr)>,
    pub effects: Vec<Effect>,
    pub return_type: Box<TypeExpr>,
    pub court: Option<String>,
}

/// Effect declarations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Named(String),
    IORead,
    IOWrite,
    Compute,
    Blocking,
    IrqHandle,
    Residual,
    MachineIoport,
    MemoryMMIO,
    CageTranslate,
    CourtRequest,
    Dma,
}

/// Pattern for match arms
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wild(Loc),
    Ident(Ident),
    Lit(Literal),
    EnumVariant(Option<Ident>, Ident, Vec<Pattern>, Loc), // (owner, variant, subpatterns)
    Tuple(Vec<Pattern>, Loc),
}

/// Expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Ident(Ident),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        loc: Loc,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        loc: Loc,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        loc: Loc,
    },
    FieldAccess {
        obj: Box<Expr>,
        field: Ident,
        loc: Loc,
    },
    MethodCall {
        obj: Box<Expr>,
        method: Ident,
        args: Vec<Expr>,
        loc: Loc,
        /// Set by checker after resolving the receiver type.
        /// Used by lowerer to emit `TypeName_method` instead of guessing.
        resolved_owner: Option<String>,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
        loc: Loc,
    },
    Block(Block, Loc),
    If {
        cond: Box<Expr>,
        then: Box<Block>,
        else_: Option<Box<Expr>>,
        loc: Loc,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
        loc: Loc,
    },
    Loop {
        body: Box<Block>,
        bound: Option<u64>,
        proven: bool,
        loc: Loc,
    },
    ForLoop {
        var: Ident,
        range: Box<Expr>,
        body: Box<Block>,
        loc: Loc,
    },
    WhileLoop {
        cond: Box<Expr>,
        body: Box<Block>,
        proven: bool,
        loc: Loc,
    },
    Return(Option<Box<Expr>>, Loc),
    Break(Loc),
    Continue(Loc),
    Yield(Box<Expr>, Loc),
    Spawn(Box<Expr>, Loc),
    Trusted {
        reason: String,
        body: Box<Block>,
        loc: Loc,
    },
    ResidualEmit {
        op: String,
        fields: Vec<(String, Expr)>,
        loc: Loc,
    },
    HandleCast(Box<Expr>, Loc),
    CapMove(Box<Expr>, Loc),
    StructLit {
        type_name: Ident,
        fields: Vec<(Ident, Expr)>,
        loc: Loc,
    },
    /// Type cast: expr as Type
    Cast {
        expr: Box<Expr>,
        type_: Box<TypeExpr>,
        loc: Loc,
    },
    /// Postfix `!` operator (expr!)
    PostfixBang {
        expr: Box<Expr>,
        loc: Loc,
    },
    /// Array literal: [expr, expr, ...]
    ArrayLit(Vec<Expr>, Loc),
    /// A type identifier used in expression context (e.g. Handle, Slice, Cap)
    TypeIdent(Ident),
    Error(Loc),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    AndAssign,
    OrAssign,
    XorAssign,
    ShlAssign,
    ShrAssign,
    Range,
    RangeInclusive,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
    Deref,
    Ref,
}

/// Block — sequence of statements
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub loc: Loc,
}

/// Match arm
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Box<Expr>,
    pub loc: Loc,
}

/// Statements
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: Ident,
        type_ann: Option<TypeExpr>,
        init: Option<Expr>,
        mut_: bool,
        loc: Loc,
    },
    Expr(Expr, Loc),
    Return(Option<Expr>, Loc),
    Assignment {
        target: Box<Expr>,
        value: Box<Expr>,
        loc: Loc,
    },
}

/// Function declaration
#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: Ident,
    pub params: Vec<(Ident, TypeExpr)>,
    pub effects: Vec<Effect>,
    pub return_type: Option<TypeExpr>,
    pub court: Option<String>,
    pub bound: Option<(String, u64)>, // (what, bound_value)
    pub body: Block,
    pub visibility: Visibility,
    pub loc: Loc,
}

/// Struct field
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub type_: TypeExpr,
    pub loc: Loc,
}

/// Struct declaration
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub layout: Option<String>, // "packed" | "default"
    pub visibility: Visibility,
    pub loc: Loc,
}

/// Enum variant
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<TypeExpr>,
    pub value: Option<u64>, // explicit numeric value (variant = 0xC0)
    pub loc: Loc,
}

/// Enum declaration
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<EnumVariant>,
    pub visibility: Visibility,
    pub loc: Loc,
}

/// Type alias
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAlias {
    pub name: Ident,
    pub type_: TypeExpr,
    pub loc: Loc,
}

/// Service declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceDecl {
    pub name: Ident,
    pub capabilities: Vec<TypeExpr>,
    pub effects: Vec<Effect>,
    pub court_threshold: Option<String>,
    pub provides: Vec<TypeExpr>,
    pub imports: Vec<TypeExpr>,
    pub loc: Loc,
}

/// Constant declaration (module-level const)
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub type_: Option<TypeExpr>,
    pub value: Option<Expr>,
    pub visibility: Visibility,
    pub loc: Loc,
}

/// Impl block — methods attached to a type
#[derive(Debug, Clone, PartialEq)]
pub struct ImplBlock {
    pub owner: Ident,
    pub methods: Vec<FnDecl>,
    pub visibility: Visibility,
    pub loc: Loc,
}

/// Top-level items
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    TypeAlias(TypeAlias),
    Service(ServiceDecl),
    Import(Ident, Loc),
    Package(Ident, Loc),
    Machine(Vec<MachineBlock>),
    Impl(ImplBlock),
    Const(ConstDecl),
}

/// Machine block (ASM entry)
#[derive(Debug, Clone, PartialEq)]
pub struct MachineBlock {
    pub reason: String,
    pub arch: Vec<String>,
    pub instructions: Vec<String>,
    pub courts: Vec<String>,
    pub receipts: Vec<String>,
    pub body: Block,
    pub loc: Loc,
}

/// A complete Phorensic source file
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
    pub loc: Loc,
}
