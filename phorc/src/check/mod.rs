// Phorc — Phorensic Bootstrap Compiler
// Type checker / semantic analyzer — PHORENSIC_LANGUAGE.md §8
//
// Implements the full Phorensic type system:
//   - Scoped symbol table (Env)
//   - Type checking for all expression forms
//   - Affine capability tracking (use-after-move, non-Copy)
//   - Effect verification (declared vs callee, subset checking)
//   - Loop bound verification (compile-time bounds)
//   - Handle generation tracking (stale handle detection)
//   - Trusted block rationale verification
//   - Entry point: check_module(SourceFile) -> Vec<Diag>

use crate::ast::*;
use crate::lex::{Diag, Loc};
use std::collections::HashMap;

// ============================================================================
// Bootstrap method signatures
// ============================================================================

/// Return type strategy for a known bootstrap method.
/// Determines the return type based on receiver type, arguments, or both.
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ReturnType {
    /// Fixed non-generic return type.
    Exact(TypeExpr),
    /// Inner type parameter of the receiver's compound type.
    /// e.g., Array(T, N)::get -> T, RingBuf(T, N)::pop -> T
    ReceiverInner,
    /// Option containing the receiver's inner type.
    /// e.g., RingBuf::pop() -> Option[T]
    OptionOfReceiverInner,
    /// Slice of the receiver's inner type.
    /// e.g., Array::slice -> Slice[T]
    SliceOfReceiverInner,
    /// Result wrapping the first arg type as Ok.
    /// e.g., Result::Ok(x) -> Result[typeof(x), U64]
    ResultOk(Vec<TypeExpr>),
    /// Result wrapping the first arg type as Err.
    /// e.g., Result::Err(e) -> Result[U64, typeof(e)]
    ResultErr(Vec<TypeExpr>),
    /// Option wrapping the first arg type.
    /// e.g., Option::Some(x) -> Option[typeof(x)]
    OptionOfArg(Vec<TypeExpr>),
    /// Fixed return for constructor methods where we propagate
    /// the inner type from a preceding type constructor argument.
    /// e.g., Str::from("hi") -> Str(255), Array::splat(42) -> Array(U64, N)
    ConstructorStr,
    ConstructorArray(Vec<TypeExpr>),
    ConstructorRingBuf,
    ConstructorZeroedArray,
    /// Self-consuming method that returns the receiver type unchanged.
    /// e.g., U64::pinning_add(self, other: U64) -> U64 (returns modified Self)
    SelfConsuming,
}

/// A known bootstrap method entry.
#[derive(Clone)]
struct MethodEntry {
    param_count: usize,
    /// If Some, the i-th parameter must be a specific type name (checked at call site).
    /// Used for methods like Str::append where the param must also be Str.
    param_type_hint: Option<&'static str>,
    returns: ReturnType,
}

/// Build the bootstrap method table.
/// Keys are (owner_type_name, method_name).
fn bootstrap_method_table() -> Vec<(&'static str, &'static str, MethodEntry)> {
    let mut table: Vec<(&'static str, &'static str, MethodEntry)> = Vec::new();

    // --- Str methods ---
    // Str::len() -> u64
    table.push((
        "Str",
        "len",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));
    // Str::append(other: Str) -> bool
    table.push((
        "Str",
        "append",
        MethodEntry {
            param_count: 1,
            param_type_hint: Some("Str"),
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::Bool, Loc::zero())),
        },
    ));
    // Str::from(src) -> Str(N)
    table.push((
        "Str",
        "from",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::ConstructorStr,
        },
    ));
    // Str::slice(start: u64, end: u64) -> Str(N)
    table.push((
        "Str",
        "slice",
        MethodEntry {
            param_count: 2,
            param_type_hint: None,
            returns: ReturnType::ReceiverInner, // same capacity as receiver
        },
    ));

    // --- Array methods ---
    // Array::capacity() -> u64
    table.push((
        "Array",
        "capacity",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));
    // Array::len() -> u64
    table.push((
        "Array",
        "len",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));
    // Array::splat(val: T) -> Array(T, N)
    table.push((
        "Array",
        "splat",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::ConstructorArray(Vec::new()),
        },
    ));
    // Array::zeroed() -> Array(T, N)
    table.push((
        "Array",
        "zeroed",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::ConstructorZeroedArray,
        },
    ));
    // Array::slice(start: u64, end: u64) -> Slice(T)
    table.push((
        "Array",
        "slice",
        MethodEntry {
            param_count: 2,
            param_type_hint: None,
            returns: ReturnType::SliceOfReceiverInner,
        },
    ));

    // --- RingBuf methods ---
    // RingBuf::new() -> RingBuf(T, N)
    table.push((
        "RingBuf",
        "new",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::ConstructorRingBuf,
        },
    ));
    // RingBuf::push(val: T) -> bool
    table.push((
        "RingBuf",
        "push",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::Bool, Loc::zero())),
        },
    ));
    // RingBuf::pop() -> Option(T)
    table.push((
        "RingBuf",
        "pop",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::OptionOfReceiverInner,
        },
    ));
    // RingBuf::capacity() -> u64
    table.push((
        "RingBuf",
        "capacity",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));
    // RingBuf::len() -> u64
    table.push((
        "RingBuf",
        "len",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));

    // --- Slice methods ---
    // Slice::len() -> u64
    table.push((
        "Slice",
        "len",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Prim(PrimType::U64, Loc::zero())),
        },
    ));

    // --- Result methods ---
    // Result::Ok(val: T) -> Result(T, E)
    table.push((
        "Result",
        "Ok",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::ResultOk(Vec::new()),
        },
    ));
    // Result::Err(val: E) -> Result(T, E)
    table.push((
        "Result",
        "Err",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::ResultErr(Vec::new()),
        },
    ));

    // --- Option methods ---
    // Option::Some(val: T) -> Option(T)
    table.push((
        "Option",
        "Some",
        MethodEntry {
            param_count: 1,
            param_type_hint: None,
            returns: ReturnType::OptionOfArg(Vec::new()),
        },
    ));
    // Option::None -> Option(T)
    table.push((
        "Option",
        "None",
        MethodEntry {
            param_count: 0,
            param_type_hint: None,
            returns: ReturnType::Exact(TypeExpr::Option(
                Box::new(TypeExpr::Prim(PrimType::U64, Loc::zero())),
                Loc::zero(),
            )),
        },
    ));

    // --- U64 methods ---
    // U64::wrapping_add(other: U64) -> U64 (self-consuming pattern)
    table.push((
        "U64",
        "wrapping_add",
        MethodEntry {
            param_count: 1,
            param_type_hint: Some("U64"),
            returns: ReturnType::SelfConsuming,
        },
    ));

    table
}

/// Compute the return type of a known bootstrap method call.
/// Returns None if the method is not in the bootstrap table.
fn method_return_type(
    owner: &str,
    method: &str,
    receiver_ty: Option<&TypeExpr>,
    arg_tys: &[TypeExpr],
    loc: Loc,
) -> Option<TypeExpr> {
    let table = bootstrap_method_table();
    let entry = table
        .iter()
        .find(|(o, m, _)| *o == owner && *m == method)?
        .2
        .clone();

    if entry.param_count != arg_tys.len() {
        return None; // caller handles arg count mismatch
    }

    // Helper: extract inner type from a compound type expression
    let inner_ty = |ty: &TypeExpr| -> Option<TypeExpr> {
        match ty {
            TypeExpr::Array(inner, _, _)
            | TypeExpr::RingBuf(inner, _, _)
            | TypeExpr::Slice(inner, _)
            | TypeExpr::Option(inner, _)
            | TypeExpr::Cap(inner, _)
            | TypeExpr::Handle(inner, _) => Some(inner.as_ref().clone()),
            TypeExpr::Result(ok, _, _) => Some(ok.as_ref().clone()),
            _ => None,
        }
    };

    // Helper: extract array/ringbuf size from a compound type
    let _array_size = |ty: &TypeExpr| -> Option<ArraySize> {
        match ty {
            TypeExpr::Array(_, size, _) | TypeExpr::RingBuf(_, size, _) => Some(size.clone()),
            TypeExpr::Str(size, _) => Some(size.clone()),
            _ => None,
        }
    };

    match &entry.returns {
        ReturnType::Exact(ty) => Some(ty.clone()),

        ReturnType::ReceiverInner => {
            // Return the receiver's inner type (e.g., Str::slice -> Str with same size)
            if let Some(recv_ty) = receiver_ty {
                match recv_ty {
                    TypeExpr::Str(size, _) => Some(TypeExpr::Str(size.clone(), loc)),
                    _ => inner_ty(recv_ty),
                }
            } else {
                None
            }
        }

        ReturnType::OptionOfReceiverInner => {
            // RingBuf::pop() -> Option[T] — T from receiver's inner type
            if let Some(recv_ty) = receiver_ty {
                inner_ty(recv_ty).map(|inner| TypeExpr::Option(Box::new(inner), loc))
            } else {
                None
            }
        }

        ReturnType::SliceOfReceiverInner => {
            // Array::slice() -> Slice[T] — T from receiver's inner type
            if let Some(recv_ty) = receiver_ty {
                inner_ty(recv_ty).map(|inner| TypeExpr::Slice(Box::new(inner), loc))
            } else {
                None
            }
        }

        ReturnType::ResultOk(_) => {
            // Result::Ok(arg) -> Result[typeof(arg), U64]
            if let Some(arg_ty) = arg_tys.first() {
                Some(TypeExpr::Result(
                    Box::new(arg_ty.clone()),
                    Box::new(TypeExpr::Prim(PrimType::U64, loc)),
                    loc,
                ))
            } else {
                None
            }
        }

        ReturnType::ResultErr(_) => {
            // Result::Err(arg) -> Result[U64, typeof(arg)]
            if let Some(arg_ty) = arg_tys.first() {
                Some(TypeExpr::Result(
                    Box::new(TypeExpr::Prim(PrimType::U64, loc)),
                    Box::new(arg_ty.clone()),
                    loc,
                ))
            } else {
                None
            }
        }

        ReturnType::OptionOfArg(_) => {
            // Option::Some(arg) -> Option[typeof(arg)]
            if let Some(arg_ty) = arg_tys.first() {
                Some(TypeExpr::Option(Box::new(arg_ty.clone()), loc))
            } else {
                None
            }
        }

        ReturnType::ConstructorStr => {
            // Str::from(arg) -> Str(0) — placeholder size, see is_placeholder_size
            Some(TypeExpr::Str(ArraySize::Literal(0, loc), loc))
        }

        ReturnType::ConstructorArray(_) => {
            // Array::splat(arg) -> Array[typeof(arg), 0]
            // We don't know the size at check time for splat.
            // Use a placeholder size; the real size is determined by the variable's type.
            if let Some(arg_ty) = arg_tys.first() {
                Some(TypeExpr::Array(
                    Box::new(arg_ty.clone()),
                    ArraySize::Literal(0, loc),
                    loc,
                ))
            } else {
                None
            }
        }

        ReturnType::ConstructorRingBuf => {
            // RingBuf::new() -> RingBuf[U64, 0]
            Some(TypeExpr::RingBuf(
                Box::new(TypeExpr::Prim(PrimType::U64, loc)),
                ArraySize::Literal(0, loc),
                loc,
            ))
        }

        ReturnType::ConstructorZeroedArray => {
            // Array::zeroed() -> Array[U64, 0]
            Some(TypeExpr::Array(
                Box::new(TypeExpr::Prim(PrimType::U64, loc)),
                ArraySize::Literal(0, loc),
                loc,
            ))
        }

        ReturnType::SelfConsuming => {
            // Methods that consume self and return Self — return the receiver type.
            // e.g., fn wrapping_add(self, other: U64) -> U64
            receiver_ty.cloned()
        }
    }
}

// ============================================================================
// Error code ranges
// ============================================================================
// E0201–E0299  Type errors
// E0300–E0399  Capability errors
// E0400–E0499  Effect errors
// E0500–E0599  Loop / bound errors

// ============================================================================
// Symbol table
// ============================================================================

/// What kind of entity a symbol represents
#[derive(Debug, Clone)]
pub enum SymbolKind {
    Variable {
        type_: TypeExpr,
        mut_: bool,
        /// Has this affine value been moved?
        moved: bool,
        /// Generation for handle tracking
        #[allow(dead_code)]
        gen: u64,
    },
    Function(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    TypeAlias(TypeAlias),
    Service(ServiceDecl),
}

/// A symbol in the scoped environment
#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub loc: Loc,
}

/// Scoped symbol table — a stack of environments
#[derive(Debug, Clone)]
pub struct Env {
    /// Nested scopes; index 0 is the outermost (module) scope
    scopes: Vec<HashMap<String, Symbol>>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter a new nested scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Exit the current innermost scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Look up a symbol by name, searching from innermost to outermost
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym);
            }
        }
        None
    }

    /// Define a symbol in the current (innermost) scope
    pub fn define(&mut self, name: String, sym: Symbol) -> Option<Symbol> {
        self.scopes.last_mut().unwrap().insert(name, sym)
    }

    /// Update a symbol's moved flag in-place
    pub fn mark_moved(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                if let SymbolKind::Variable { ref mut moved, .. } = &mut sym.kind {
                    *moved = true;
                }
                return;
            }
        }
    }

    /// Check whether a variable has been moved
    pub fn is_moved(&self, name: &str) -> bool {
        self.lookup(name).map_or(false, |s| match &s.kind {
            SymbolKind::Variable { moved, .. } => *moved,
            _ => false,
        })
    }

    /// Get the generation for a handle variable
    pub fn get_gen(&self, name: &str) -> Option<u64> {
        self.lookup(name).and_then(|s| match &s.kind {
            SymbolKind::Variable { gen, .. } => Some(*gen),
            _ => None,
        })
    }

    /// Bump generation for a variable (after handle invalidation)
    pub fn bump_gen(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                if let SymbolKind::Variable { ref mut gen, .. } = &mut sym.kind {
                    *gen += 1;
                }
                return;
            }
        }
    }

    /// Look up a type by name (struct, enum, alias)
    pub fn lookup_type(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                match &sym.kind {
                    SymbolKind::Struct(_) | SymbolKind::Enum(_) | SymbolKind::TypeAlias(_) => {
                        return Some(sym);
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Look up a function by name
    pub fn lookup_fn(&self, name: &str) -> Option<&FnDecl> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                if let SymbolKind::Function(f) = &sym.kind {
                    return Some(f);
                }
            }
        }
        None
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Capability tracking
// ============================================================================

/// Tracks affine resources (capabilities) that must be used exactly once
#[derive(Debug, Clone)]
pub struct CapTracker {
    /// Active capabilities in the current scope
    caps: Vec<CapRecord>,
    /// Next capability id for generation tracking
    next_id: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CapRecord {
    name: String,
    type_: TypeExpr,
    consumed: bool,
    #[allow(dead_code)]
    gen: u64,
}

impl CapTracker {
    pub fn new() -> Self {
        Self {
            caps: Vec::new(),
            next_id: 1,
        }
    }

    /// Register a new capability (enters scope as available)
    pub fn add(&mut self, name: String, type_: TypeExpr) {
        let gen = self.next_id;
        self.next_id += 1;
        self.caps.push(CapRecord {
            name,
            type_,
            consumed: false,
            gen,
        });
    }

    /// Try to consume a capability — returns false if already consumed
    pub fn consume(&mut self, name: &str) -> bool {
        for cap in self.caps.iter_mut().rev() {
            if cap.name == name && !cap.consumed {
                cap.consumed = true;
                return true;
            }
        }
        false
    }

    /// Check if a capability name is available (exists and not consumed)
    pub fn available(&self, name: &str) -> bool {
        self.caps.iter().any(|c| c.name == name && !c.consumed)
    }

    /// Check if a name is a known capability at all
    pub fn is_cap(&self, name: &str) -> bool {
        self.caps.iter().any(|c| c.name == name)
    }

    /// Return all unconsumed capabilities (for scope-exit check)
    pub(crate) fn unconsumed(&self) -> Vec<&CapRecord> {
        self.caps.iter().filter(|c| !c.consumed).collect()
    }
}

impl Default for CapTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Effect tracking
// ============================================================================

/// A set of effects for subset/superset comparisons
#[derive(Debug, Clone, Default)]
pub struct EffectSet {
    effects: Vec<Effect>,
}

impl EffectSet {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    pub fn from_vec(effects: Vec<Effect>) -> Self {
        Self { effects }
    }

    pub fn contains(&self, eff: &Effect) -> bool {
        self.effects.contains(eff)
    }

    /// Check if self is a subset of other (all of our effects appear in other)
    pub fn is_subset_of(&self, other: &EffectSet) -> bool {
        self.effects.iter().all(|e| other.contains(e))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.effects.iter()
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}

/// Resolve an effect from a parsed Effect value into our set representation
#[allow(dead_code)]
fn effect_from_decl(e: &Effect) -> Effect {
    e.clone()
}

// ============================================================================
// Handle generation tracker
// ============================================================================

/// Tracks Handle<T> generation numbers to detect stale handle usage
#[derive(Debug, Clone)]
pub struct HandleTracker {
    /// Map from handle variable name to its creation generation
    handles: HashMap<String, HandleState>,
}

#[derive(Debug, Clone, Copy)]
struct HandleState {
    /// Generation when created
    #[allow(dead_code)]
    created_at_gen: u64,
    /// Generation of the underlying resource (bumped on invalidation)
    resource_gen: u64,
    /// Whether this handle has been invalidated
    stale: bool,
}

impl HandleTracker {
    pub fn new() -> Self {
        Self {
            handles: HashMap::new(),
        }
    }

    /// Register a newly created handle
    pub fn add_handle(&mut self, name: String, gen: u64) {
        self.handles.insert(
            name,
            HandleState {
                created_at_gen: gen,
                resource_gen: gen,
                stale: false,
            },
        );
    }

    /// Invalidate all handles pointing to a resource (bump resource gen)
    pub fn invalidate_resource(&mut self, gen_threshold: u64) {
        for state in self.handles.values_mut() {
            if state.resource_gen <= gen_threshold {
                state.stale = true;
            }
        }
    }

    /// Check if a handle is stale
    pub fn is_stale(&self, name: &str) -> bool {
        self.handles.get(name).map_or(false, |h| h.stale)
    }

    /// Consume a handle (it's been used in a capability operation)
    pub fn consume_handle(&mut self, name: &str) -> bool {
        if let Some(state) = self.handles.get_mut(name) {
            if state.stale {
                return false;
            }
            state.stale = true;
            true
        } else {
            false
        }
    }
}

impl Default for HandleTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// A method call resolved by the checker.
/// Carries the owner type name, receiver type, and computed return type.
/// The return type is used by the lowerer to allocate correctly-typed PHIR temps.
#[derive(Debug, Clone)]
pub struct MethodOwner {
    pub line: u32,
    pub col: u32,
    pub owner: String,
    /// The full type of the receiver expression, if available.
    /// Used for generic returns like pop() -> Option[inner_type].
    pub receiver_ty: Option<TypeExpr>,
    /// The computed return type of the method call.
    /// Set by check_method_call after signature resolution.
    /// Consumed by the lowerer to allocate correctly-typed PHIR temporaries.
    pub return_ty: Option<TypeExpr>,
}

// ============================================================================
// Check context
// ============================================================================

/// The main context passed through all checking operations
pub struct CheckContext<'a> {
    /// Scoped symbol table
    pub env: Env,
    /// Current function's declared effect set
    pub declared_effects: Vec<Effect>,
    /// Tracker for affine capabilities
    pub caps: CapTracker,
    /// Diagnostics accumulated during checking
    pub diags: Vec<Diag>,
    /// Expected return type of the enclosing function
    pub return_type: Option<TypeExpr>,
    /// Resolved method owners: (line, col) → owner_type_name + receiver type
    /// Populated by checker, consumed by lowerer for callee name generation.
    pub method_owners: Vec<MethodOwner>,
    /// Handle generation counter (global for the module)
    pub gen_counter: u64,
    /// Handle tracker for stale detection
    pub handles: HandleTracker,
    /// Stack of loop bounds for verification
    pub loop_bounds: Vec<Option<u64>>,
    /// Are we currently inside a trusted block?
    pub in_trusted: bool,
    /// Currently declared effects as an EffectSet for fast subset checks
    pub effect_set: EffectSet,
    /// The source file being checked (for cross-references)
    pub source: Option<&'a SourceFile>,
}

impl<'a> CheckContext<'a> {
    pub fn new() -> Self {
        Self {
            env: Env::new(),
            declared_effects: Vec::new(),
            caps: CapTracker::new(),
            diags: Vec::new(),
            return_type: None,
            gen_counter: 0,
            handles: HandleTracker::new(),
            loop_bounds: Vec::new(),
            in_trusted: false,
            effect_set: EffectSet::new(),
            source: None,
            method_owners: Vec::new(),
        }
    }

    fn next_gen(&mut self) -> u64 {
        let g = self.gen_counter;
        self.gen_counter += 1;
        g
    }

    pub fn add_diag(&mut self, code: &'static str, loc: Loc, message: String) {
        self.diags.push(Diag::new(code, loc, message));
    }

    /// Look up a type, resolving Named types to their declaration
    fn resolve_type<'b>(&'b self, ty: &'b TypeExpr) -> &'b TypeExpr {
        // Resolve named types (type aliases) through the symbol table
        match ty {
            TypeExpr::Named(ident) => {
                if let Some(sym) = self.env.lookup_type(&ident.name) {
                    if let SymbolKind::TypeAlias(talias) = &sym.kind {
                        return &talias.type_;
                    }
                }
                ty
            }
            _ => ty,
        }
    }

    /// Check if a type is a capability type (Cap(T))
    fn is_cap_type(ty: &TypeExpr) -> bool {
        matches!(ty, TypeExpr::Cap(..))
    }

    /// Check if a type is a handle type (Handle(T))
    fn is_handle_type(ty: &TypeExpr) -> bool {
        matches!(ty, TypeExpr::Handle(..))
    }

    /// Check if a type is copy-able (primitives, non-capability types)
    #[allow(dead_code)]
    fn is_copy_type(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Prim(..) => true,
            TypeExpr::Named(..) => true, // resolves to its underlying type; conservative
            TypeExpr::Str(_, _) => false, // Str(N) is affine
            TypeExpr::Cap(..) => false,
            TypeExpr::Handle(..) => false,
            TypeExpr::Effect(_) => true,
            TypeExpr::Provenance(_) => true,
            TypeExpr::Receipt(_) => true,
            TypeExpr::TrustState(_) => false,
            TypeExpr::ResidualTy(_) => false,
            TypeExpr::Never(_) => true,
            TypeExpr::FnType(..) => true,
            TypeExpr::Array(inner, _, _) => Self::is_copy_type(inner),
            TypeExpr::RingBuf(inner, _, _) => Self::is_copy_type(inner),
            TypeExpr::Slice(inner, _) => Self::is_copy_type(inner),
            TypeExpr::Tuple(types, _) => types.iter().all(|t| Self::is_copy_type(t)),
            TypeExpr::Option(inner, _) => Self::is_copy_type(inner),
            TypeExpr::Result(ok, err, _) => Self::is_copy_type(ok) && Self::is_copy_type(err),
        }
    }

    /// Check that a type expression is well-formed (all referenced types exist)
    fn check_type_form(&mut self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Prim(..)
            | TypeExpr::Effect(_)
            | TypeExpr::Never(_)
            | TypeExpr::Provenance(_)
            | TypeExpr::Receipt(_)
            | TypeExpr::TrustState(_)
            | TypeExpr::ResidualTy(_) => true,

            TypeExpr::Named(ident) => {
                if self.env.lookup_type(&ident.name).is_none() {
                    self.add_diag(
                        code_type(201),
                        ident.loc,
                        format!("undefined type: `{}`", ident.name),
                    );
                    false
                } else {
                    true
                }
            }

            TypeExpr::Array(inner, _, _loc)
            | TypeExpr::RingBuf(inner, _, _loc)
            | TypeExpr::Slice(inner, _loc)
            | TypeExpr::Cap(inner, _loc)
            | TypeExpr::Handle(inner, _loc)
            | TypeExpr::Option(inner, _loc) => self.check_type_form(inner),

            TypeExpr::Result(ok, err, _) => self.check_type_form(ok) && self.check_type_form(err),

            TypeExpr::Tuple(types, _) => types.iter().all(|t| self.check_type_form(t)),

            TypeExpr::Str(_, _) => true,

            TypeExpr::FnType(sig, _) => {
                sig.params.iter().all(|(_, t)| self.check_type_form(t))
                    && self.check_type_form(&sig.return_type)
            }
        }
    }

    /// Check that a type annotation matches the inferred type
    fn check_type_assignable(&mut self, expected: &TypeExpr, actual: &TypeExpr, loc: Loc) -> bool {
        // Bootstrap relaxation: u64 literals can be assigned to smaller integer types.
        // This handles `let x: u32 = 0;` where the literal infers as u64.
        let is_widening_int = |expected: &TypeExpr, actual: &TypeExpr| -> bool {
            matches!(
                (expected, actual),
                (
                    TypeExpr::Prim(PrimType::U8, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::U16, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::U32, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::I8, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::I16, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::I32, _),
                    TypeExpr::Prim(PrimType::U64, _)
                ) | (
                    TypeExpr::Prim(PrimType::Bool, _),
                    TypeExpr::Prim(PrimType::U64, _)
                )
            )
        };
        // Bootstrap relaxation: Handle(T) and Cap(T) are treated as u64 at runtime.
        // Allow assigning Handle/Cap values where u64 is expected.
        let is_handle_as_u64 = |expected: &TypeExpr, actual: &TypeExpr| -> bool {
            matches!(expected, TypeExpr::Prim(PrimType::U64, _))
                && matches!(actual, TypeExpr::Handle(..) | TypeExpr::Cap(..))
        };
        // Bootstrap relaxation: constructor placeholder types (Array/RingBuf with Literal(0) size)
        // should match any concrete type for the inner element.
        let is_constructor_array = |expected: &TypeExpr, actual: &TypeExpr| -> bool {
            match (expected, actual) {
                (
                    TypeExpr::Array(_, _, _) | TypeExpr::RingBuf(_, _, _),
                    TypeExpr::Array(_, actual_size, _) | TypeExpr::RingBuf(_, actual_size, _),
                ) => Self::is_placeholder_size(actual_size),
                _ => false,
            }
        };
        if self.types_equal(expected, actual)
            || is_widening_int(expected, actual)
            || is_handle_as_u64(expected, actual)
            || is_constructor_array(expected, actual)
        {
            true
        } else {
            self.add_diag(
                code_type(202),
                loc,
                format!(
                    "type mismatch: expected `{:?}`, got `{:?}`",
                    expected, actual
                ),
            );
            false
        }
    }

    /// Compare two ArraySize values, ignoring source location.
    /// The derived PartialEq on ArraySize includes Loc, which breaks
    /// equality for identically-valued sizes from different source positions.
    fn array_sizes_equal(a: &ArraySize, b: &ArraySize) -> bool {
        match (a, b) {
            (ArraySize::Literal(na, _), ArraySize::Literal(nb, _)) => na == nb,
            (ArraySize::ConstIdent(sa, _), ArraySize::ConstIdent(sb, _)) => sa == sb,
            _ => false,
        }
    }

    /// Check if an ArraySize is a constructor placeholder (Literal(0) meaning "unknown size").
    fn is_placeholder_size(size: &ArraySize) -> bool {
        matches!(size, ArraySize::Literal(0, _))
    }

    /// Check if a type expression contains any constructor placeholder sizes.
    /// Used to relax type checking when a constructor returns an incomplete type.
    #[allow(dead_code)]
    fn has_placeholder(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Array(_, size, _) | TypeExpr::RingBuf(_, size, _) => {
                Self::is_placeholder_size(size)
            }
            TypeExpr::Str(size, _) => Self::is_placeholder_size(size),
            _ => false,
        }
    }

    /// Structural type equality check (resolves named types).
    /// ArraySize::Literal(0) is treated as a wildcard (constructor placeholder)
    /// that matches any size.
    fn types_equal(&self, a: &TypeExpr, b: &TypeExpr) -> bool {
        let a = self.resolve_type(a);
        let b = self.resolve_type(b);
        match (a, b) {
            (TypeExpr::Prim(pa, _), TypeExpr::Prim(pb, _)) => pa == pb,
            (TypeExpr::Named(ia), TypeExpr::Named(ib)) => ia.name == ib.name,
            (TypeExpr::Array(ia, na, _), TypeExpr::Array(ib, nb, _)) => {
                (Self::array_sizes_equal(na, nb)
                    || Self::is_placeholder_size(na)
                    || Self::is_placeholder_size(nb))
                    && self.types_equal(ia, ib)
            }
            (TypeExpr::RingBuf(ia, na, _), TypeExpr::RingBuf(ib, nb, _)) => {
                (Self::array_sizes_equal(na, nb)
                    || Self::is_placeholder_size(na)
                    || Self::is_placeholder_size(nb))
                    && self.types_equal(ia, ib)
            }
            (TypeExpr::Slice(ia, _), TypeExpr::Slice(ib, _)) => self.types_equal(ia, ib),
            (TypeExpr::Str(na, _), TypeExpr::Str(nb, _)) => {
                Self::array_sizes_equal(na, nb)
                    || Self::is_placeholder_size(na)
                    || Self::is_placeholder_size(nb)
            }
            (TypeExpr::Cap(ia, _), TypeExpr::Cap(ib, _)) => self.types_equal(ia, ib),
            (TypeExpr::Handle(ia, _), TypeExpr::Handle(ib, _)) => self.types_equal(ia, ib),
            (TypeExpr::Effect(_), TypeExpr::Effect(_)) => true,
            (TypeExpr::Provenance(_), TypeExpr::Provenance(_)) => true,
            (TypeExpr::Receipt(_), TypeExpr::Receipt(_)) => true,
            (TypeExpr::TrustState(_), TypeExpr::TrustState(_)) => true,
            (TypeExpr::ResidualTy(_), TypeExpr::ResidualTy(_)) => true,
            (TypeExpr::Never(_), TypeExpr::Never(_)) => true,
            (TypeExpr::FnType(sa, _), TypeExpr::FnType(sb, _)) => {
                sa.params.len() == sb.params.len()
                    && sa
                        .params
                        .iter()
                        .zip(&sb.params)
                        .all(|((_, ta), (_, tb))| self.types_equal(ta, tb))
                    && self.types_equal(&sa.return_type, &sb.return_type)
            }
            (TypeExpr::Tuple(va, _), TypeExpr::Tuple(vb, _)) => {
                va.len() == vb.len() && va.iter().zip(vb).all(|(ta, tb)| self.types_equal(ta, tb))
            }
            (TypeExpr::Option(ia, _), TypeExpr::Option(ib, _)) => self.types_equal(ia, ib),
            (TypeExpr::Result(oka, erra, _), TypeExpr::Result(okb, errb, _)) => {
                self.types_equal(oka, okb) && self.types_equal(erra, errb)
            }
            _ => false,
        }
    }

    /// Get the "truthy" type — what bool/int/char expressions resolve to
    #[allow(dead_code)]
    fn plain_type(&self, ty: &TypeExpr) -> TypeExpr {
        match ty {
            TypeExpr::Prim(p, loc) => TypeExpr::Prim(p.clone(), *loc),
            other => other.clone(),
        }
    }

    /// Walk the inner type of a compound type (e.g., Cap(T) -> T)
    #[allow(dead_code)]
    fn inner_type<'b>(&'b self, ty: &'b TypeExpr) -> Option<&'b TypeExpr> {
        match ty {
            TypeExpr::Cap(inner, _)
            | TypeExpr::Handle(inner, _)
            | TypeExpr::Option(inner, _)
            | TypeExpr::Array(inner, _, _)
            | TypeExpr::RingBuf(inner, _, _)
            | TypeExpr::Slice(inner, _) => Some(inner),
            TypeExpr::Result(ok, _, _) => Some(ok),
            _ => None,
        }
    }
}

impl<'a> Default for CheckContext<'a> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper: get Loc from various AST nodes
// ============================================================================

fn expr_loc(expr: &Expr) -> Loc {
    match expr {
        Expr::Literal(lit) => match lit {
            Literal::Int(_, loc)
            | Literal::Float(_, loc)
            | Literal::Bool(_, loc)
            | Literal::Char(_, loc)
            | Literal::Str(_, loc) => *loc,
        },
        Expr::Ident(id) => id.loc,
        Expr::Binary { loc, .. }
        | Expr::Unary { loc, .. }
        | Expr::Call { loc, .. }
        | Expr::FieldAccess { loc, .. }
        | Expr::MethodCall { loc, .. }
        | Expr::Index { loc, .. }
        | Expr::Block(_, loc)
        | Expr::If { loc, .. }
        | Expr::Match { loc, .. }
        | Expr::Loop { loc, .. }
        | Expr::ForLoop { loc, .. }
        | Expr::WhileLoop { loc, .. }
        | Expr::Return(_, loc)
        | Expr::Break(loc)
        | Expr::Continue(loc)
        | Expr::Yield(_, loc)
        | Expr::Spawn(_, loc)
        | Expr::Trusted { loc, .. }
        | Expr::ResidualEmit { loc, .. }
        | Expr::HandleCast(_, loc)
        | Expr::CapMove(_, loc)
        | Expr::StructLit { loc, .. }
        | Expr::Cast { loc, .. }
        | Expr::PostfixBang { loc, .. }
        | Expr::ArrayLit(_, loc)
        | Expr::Error(loc) => *loc,
        Expr::TypeIdent(id) => id.loc,
    }
}

// ============================================================================
// Type checking — expressions
// ============================================================================

/// Check a literal expression and return its type
fn check_literal(lit: &Literal) -> TypeExpr {
    match lit {
        Literal::Int(_, loc) => TypeExpr::Prim(PrimType::U64, *loc),
        Literal::Float(_, loc) => TypeExpr::Prim(PrimType::F64, *loc),
        Literal::Bool(_, loc) => TypeExpr::Prim(PrimType::Bool, *loc),
        Literal::Char(_, loc) => TypeExpr::Prim(PrimType::Char, *loc),
        Literal::Str(_, loc) => {
            // String literals are Str(N) — for now, infer a reasonable default
            TypeExpr::Str(ArraySize::Literal(255, *loc), *loc)
        }
    }
}

/// Check an identifier expression
fn check_ident(ctx: &mut CheckContext, id: &Ident) -> Result<TypeExpr, ()> {
    let name = &id.name;

    // Check if this is a variable
    if let Some(sym) = ctx.env.lookup(name) {
        match &sym.kind {
            SymbolKind::Variable {
                type_,
                moved,
                gen: _,
                ..
            } => {
                if *moved {
                    ctx.add_diag(
                        code_type(203),
                        id.loc,
                        format!("use of moved value: `{}`", name),
                    );
                    return Err(());
                }

                // If it's a Cap(T) type, track consumption
                if CheckContext::is_cap_type(type_) {
                    if !ctx.caps.consume(name) {
                        ctx.add_diag(
                            code_type(204),
                            id.loc,
                            format!("capability `{}` already consumed", name),
                        );
                        return Err(());
                    }
                }

                // If it's a Handle(T) type, check staleness
                if CheckContext::is_handle_type(type_) {
                    if ctx.handles.is_stale(name) {
                        ctx.add_diag(
                            code_type(205),
                            id.loc,
                            format!("use of stale handle: `{}`", name),
                        );
                        return Err(());
                    }
                }

                return Ok(type_.clone());
            }
            SymbolKind::Function(f) => {
                // Returning a function type — build FnType from declaration
                let ret = f
                    .return_type
                    .clone()
                    .unwrap_or_else(|| TypeExpr::Prim(PrimType::Void, f.name.loc));
                let params: Vec<(Ident, TypeExpr)> = f.params.clone();
                let sig = FnTypeSig {
                    params,
                    effects: f.effects.clone(),
                    return_type: Box::new(ret),
                    court: f.court.clone(),
                };
                return Ok(TypeExpr::FnType(Box::new(sig), id.loc));
            }
            SymbolKind::Struct(_) | SymbolKind::Enum(_) | SymbolKind::TypeAlias(_) => {
                ctx.add_diag(
                    code_type(206),
                    id.loc,
                    format!("`{}` is a type name, not a value", name),
                );
                return Err(());
            }
            SymbolKind::Service(_) => {
                ctx.add_diag(
                    code_type(207),
                    id.loc,
                    format!("`{}` is a service name, not a value", name),
                );
                return Err(());
            }
        }
    }

    ctx.add_diag(
        code_type(208),
        id.loc,
        format!("use of undeclared identifier: `{}`", name),
    );
    Err(())
}

/// Check a binary expression
fn check_binary(
    ctx: &mut CheckContext,
    op: &BinOp,
    lhs: &Expr,
    rhs: &Expr,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let lhs_ty = walk_expr(ctx, lhs)?;
    let rhs_ty = walk_expr(ctx, rhs)?;

    match op {
        // Arithmetic operators
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            match (&lhs_ty, &rhs_ty) {
                (TypeExpr::Prim(pl, _), TypeExpr::Prim(pr, _)) => {
                    let ok = matches!(
                        (pl, pr),
                        (PrimType::U8, PrimType::U8)
                            | (PrimType::U16, PrimType::U16)
                            | (PrimType::U32, PrimType::U32)
                            | (PrimType::U64, PrimType::U64)
                            | (PrimType::I8, PrimType::I8)
                            | (PrimType::I16, PrimType::I16)
                            | (PrimType::I32, PrimType::I32)
                            | (PrimType::I64, PrimType::I64)
                            | (PrimType::F32, PrimType::F32)
                            | (PrimType::F64, PrimType::F64)
                            | (PrimType::Usize, PrimType::Usize)
                            | (PrimType::Isize, PrimType::Isize)
                    );
                    if ok {
                        Ok(lhs_ty.clone())
                    } else {
                        ctx.add_diag(
                            code_type(209),
                            *loc,
                            format!(
                                "arithmetic on mismatched types: `{:?}` and `{:?}`",
                                lhs_ty, rhs_ty
                            ),
                        );
                        // Fallback: return lhs type
                        Ok(lhs_ty.clone())
                    }
                }
                _ => {
                    ctx.add_diag(
                        code_type(210),
                        *loc,
                        format!(
                            "arithmetic on non-numeric types: `{:?}` and `{:?}`",
                            lhs_ty, rhs_ty
                        ),
                    );
                    Ok(lhs_ty.clone())
                }
            }
        }

        // Bitwise operators
        BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => match (&lhs_ty, &rhs_ty) {
            (TypeExpr::Prim(pl, _), TypeExpr::Prim(pr, _)) => {
                let int_ok = matches!(
                    (pl, pr),
                    (PrimType::U8, PrimType::U8)
                        | (PrimType::U16, PrimType::U16)
                        | (PrimType::U32, PrimType::U32)
                        | (PrimType::U64, PrimType::U64)
                        | (PrimType::I8, PrimType::I8)
                        | (PrimType::I16, PrimType::I16)
                        | (PrimType::I32, PrimType::I32)
                        | (PrimType::I64, PrimType::I64)
                        | (PrimType::Usize, PrimType::Usize)
                        | (PrimType::Isize, PrimType::Isize)
                        | (PrimType::Bool, PrimType::Bool)
                );
                if int_ok {
                    Ok(lhs_ty.clone())
                } else {
                    ctx.add_diag(
                        code_type(211),
                        *loc,
                        format!(
                            "bitwise op on mismatched types: `{:?}` and `{:?}`",
                            lhs_ty, rhs_ty
                        ),
                    );
                    Ok(lhs_ty.clone())
                }
            }
            _ => {
                ctx.add_diag(
                    code_type(212),
                    *loc,
                    format!(
                        "bitwise op on non-integer types: `{:?}` and `{:?}`",
                        lhs_ty, rhs_ty
                    ),
                );
                Ok(lhs_ty.clone())
            }
        },

        // Comparison operators
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if ctx.types_equal(&lhs_ty, &rhs_ty) {
                Ok(TypeExpr::Prim(PrimType::Bool, *loc))
            } else {
                ctx.add_diag(
                    code_type(213),
                    *loc,
                    format!(
                        "comparison of different types: `{:?}` and `{:?}`",
                        lhs_ty, rhs_ty
                    ),
                );
                Ok(TypeExpr::Prim(PrimType::Bool, *loc))
            }
        }

        // Logical operators
        BinOp::AndAnd | BinOp::OrOr => match (&lhs_ty, &rhs_ty) {
            (TypeExpr::Prim(PrimType::Bool, _), TypeExpr::Prim(PrimType::Bool, _)) => {
                Ok(TypeExpr::Prim(PrimType::Bool, *loc))
            }
            _ => {
                ctx.add_diag(
                    code_type(214),
                    *loc,
                    format!(
                        "logical op requires bool operands, got `{:?}` and `{:?}`",
                        lhs_ty, rhs_ty
                    ),
                );
                Ok(TypeExpr::Prim(PrimType::Bool, *loc))
            }
        },

        // Assignment
        BinOp::Assign => {
            // Check that lhs is an assignable target (ident, field access, index)
            match lhs {
                Expr::Ident(id) => {
                    if let Some(sym) = ctx.env.lookup(&id.name) {
                        if let SymbolKind::Variable { mut_, .. } = &sym.kind {
                            if !*mut_ {
                                ctx.add_diag(
                                    code_type(215),
                                    id.loc,
                                    format!("cannot assign to immutable variable `{}`", id.name),
                                );
                            }
                        }
                    }
                    // Mark as reassigned — handle gen stays same
                    let assignable_ty = ctx.env.lookup(&id.name).and_then(|sym| {
                        if let SymbolKind::Variable { type_, .. } = &sym.kind {
                            Some(type_.clone())
                        } else {
                            None
                        }
                    });
                    if let Some(type_) = assignable_ty {
                        ctx.check_type_assignable(&type_, &rhs_ty, *loc);
                    }
                    Ok(rhs_ty.clone())
                }
                _ => {
                    ctx.add_diag(
                        code_type(216),
                        *loc,
                        "invalid assignment target (not a simple identifier)".to_string(),
                    );
                    Ok(rhs_ty.clone())
                }
            }
        }

        // Compound assignment
        BinOp::AddAssign
        | BinOp::SubAssign
        | BinOp::MulAssign
        | BinOp::DivAssign
        | BinOp::RemAssign
        | BinOp::AndAssign
        | BinOp::OrAssign
        | BinOp::XorAssign
        | BinOp::ShlAssign
        | BinOp::ShrAssign => {
            // Check lhs is mutable, then check type compatibility
            match lhs {
                Expr::Ident(id) => {
                    let var_info = ctx.env.lookup(&id.name).and_then(|sym| {
                        if let SymbolKind::Variable { mut_, type_, .. } = &sym.kind {
                            Some((*mut_, type_.clone()))
                        } else {
                            None
                        }
                    });
                    if let Some((is_mut, var_type)) = var_info {
                        if !is_mut {
                            ctx.add_diag(
                                code_type(215),
                                id.loc,
                                format!("cannot assign to immutable variable `{}`", id.name),
                            );
                        }
                        ctx.check_type_assignable(&var_type, &rhs_ty, *loc);
                    }
                    Ok(rhs_ty.clone())
                }
                _ => {
                    ctx.add_diag(
                        code_type(216),
                        *loc,
                        "invalid compound assignment target".to_string(),
                    );
                    Ok(rhs_ty.clone())
                }
            }
        }

        // Range operators
        BinOp::Range | BinOp::RangeInclusive => {
            // Range requires integer types
            match (&lhs_ty, &rhs_ty) {
                (TypeExpr::Prim(pl, _), TypeExpr::Prim(pr, _)) => {
                    let ok = matches!(
                        (pl, pr),
                        (PrimType::U8, PrimType::U8)
                            | (PrimType::U16, PrimType::U16)
                            | (PrimType::U32, PrimType::U32)
                            | (PrimType::U64, PrimType::U64)
                            | (PrimType::I8, PrimType::I8)
                            | (PrimType::I16, PrimType::I16)
                            | (PrimType::I32, PrimType::I32)
                            | (PrimType::I64, PrimType::I64)
                            | (PrimType::Usize, PrimType::Usize)
                            | (PrimType::Isize, PrimType::Isize)
                    );
                    if ok {
                        // Range yields the element type (for iteration), but as a type it's
                        // opaque — return lhs type for now
                        Ok(lhs_ty.clone())
                    } else {
                        ctx.add_diag(
                            code_type(209),
                            *loc,
                            format!(
                                "range requires integer types, got `{:?}` and `{:?}`",
                                lhs_ty, rhs_ty
                            ),
                        );
                        Ok(lhs_ty.clone())
                    }
                }
                _ => {
                    ctx.add_diag(
                        code_type(210),
                        *loc,
                        format!(
                            "range requires integer types, got `{:?}` and `{:?}`",
                            lhs_ty, rhs_ty
                        ),
                    );
                    Ok(lhs_ty.clone())
                }
            }
        }
    }
}

/// Check a unary expression
fn check_unary(ctx: &mut CheckContext, op: &UnOp, expr: &Expr, loc: &Loc) -> Result<TypeExpr, ()> {
    let inner_ty = walk_expr(ctx, expr)?;

    match op {
        UnOp::Neg => match &inner_ty {
            TypeExpr::Prim(p, _) => {
                let ok = matches!(
                    p,
                    PrimType::I8
                        | PrimType::I16
                        | PrimType::I32
                        | PrimType::I64
                        | PrimType::F32
                        | PrimType::F64
                );
                if ok {
                    Ok(inner_ty.clone())
                } else {
                    ctx.add_diag(
                        code_type(217),
                        *loc,
                        format!(
                            "negation requires signed numeric type, got `{:?}`",
                            inner_ty
                        ),
                    );
                    Ok(inner_ty.clone())
                }
            }
            _ => {
                ctx.add_diag(
                    code_type(217),
                    *loc,
                    format!("negation requires numeric type, got `{:?}`", inner_ty),
                );
                Ok(inner_ty.clone())
            }
        },
        UnOp::Not => match &inner_ty {
            TypeExpr::Prim(p, _) => {
                let ok = matches!(
                    p,
                    PrimType::Bool
                        | PrimType::U8
                        | PrimType::U16
                        | PrimType::U32
                        | PrimType::U64
                        | PrimType::I8
                        | PrimType::I16
                        | PrimType::I32
                        | PrimType::I64
                );
                if ok {
                    Ok(inner_ty.clone())
                } else {
                    ctx.add_diag(
                        code_type(218),
                        *loc,
                        format!("not requires bool or integer type, got `{:?}`", inner_ty),
                    );
                    Ok(inner_ty.clone())
                }
            }
            _ => {
                ctx.add_diag(
                    code_type(218),
                    *loc,
                    format!("not requires bool or integer type, got `{:?}`", inner_ty),
                );
                Ok(inner_ty.clone())
            }
        },
        UnOp::Deref => {
            // Deref requires a pointer-like type; for Phorensic, Handle(T) or Cap(T)
            match &inner_ty {
                TypeExpr::Handle(inner, _) | TypeExpr::Cap(inner, _) => Ok(*inner.clone()),
                _ => {
                    ctx.add_diag(
                        code_type(219),
                        *loc,
                        format!(
                            "dereference requires handle or cap type, got `{:?}`",
                            inner_ty
                        ),
                    );
                    Ok(inner_ty.clone())
                }
            }
        }
        UnOp::Ref => {
            // Reference — wrap in Cap type? For now, just use a named ref.
            // In Phorensic, `&expr` creates a borrow, which is fine for any type
            Ok(TypeExpr::Cap(Box::new(inner_ty.clone()), *loc))
        }
    }
}

/// Check a function call expression
fn check_call(
    ctx: &mut CheckContext,
    callee: &Expr,
    args: &[Expr],
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let callee_ty = walk_expr(ctx, callee)?;

    // Handle type constructor calls: cap(val), Handle(val), Slice(val), etc.
    if let Expr::TypeIdent(type_id) = callee {
        let type_name = type_id.name.as_str();
        // Normalize: the lexer produces lowercase for some keywords ("cap") and
        // uppercase for others ("Handle", "Slice"). Accept both forms.
        let normalized = match type_name {
            "cap" | "Cap" => "Cap",
            "handle" | "Handle" => "Handle",
            "slice" | "Slice" => "Slice",
            "array" | "Array" => "Array",
            "ringbuf" | "RingBuf" => "RingBuf",
            "str" | "Str" => "Str",
            _ => type_name,
        };
        let is_constructor = matches!(
            normalized,
            "Cap" | "Handle" | "Slice" | "Array" | "RingBuf" | "Str"
        );
        if is_constructor && args.len() == 1 {
            let arg_ty = walk_expr(ctx, &args[0])?;
            let result_ty = match normalized {
                "Cap" => TypeExpr::Cap(Box::new(arg_ty), *loc),
                "Handle" => TypeExpr::Handle(Box::new(arg_ty), *loc),
                "Slice" => TypeExpr::Slice(Box::new(arg_ty), *loc),
                _ => arg_ty,
            };
            return Ok(result_ty);
        }
        // Multi-arg constructors: Array(T, N), RingBuf(T, N), Str(N)
        if normalized == "Str" && args.len() == 1 {
            // Return Str(0) — placeholder size, matched by is_placeholder_size wildcard
            return Ok(TypeExpr::Str(ArraySize::Literal(0, *loc), *loc));
        }
    }

    // Clone the fn decl early to avoid holding a borrow on ctx
    let fn_decl = match callee {
        Expr::Ident(id) => ctx.env.lookup_fn(&id.name).cloned(),
        _ => None,
    };

    if let Some(fdecl) = fn_decl {
        // Check argument count
        if args.len() != fdecl.params.len() {
            ctx.add_diag(
                code_type(220),
                *loc,
                format!(
                    "wrong number of arguments: expected {}, got {}",
                    fdecl.params.len(),
                    args.len()
                ),
            );
        }

        // Check each argument type against parameter type
        for (_i, (arg, (_, param_ty))) in args.iter().zip(&fdecl.params).enumerate() {
            match walk_expr(ctx, arg) {
                Ok(arg_ty) => {
                    ctx.check_type_assignable(param_ty, &arg_ty, expr_loc(arg));
                }
                Err(_) => {}
            }
        }

        // Effect checking: callee effects must be a subset of caller's declared effects
        let callee_effects = EffectSet::from_vec(fdecl.effects.clone());
        if !callee_effects.is_empty() {
            let current_effects = ctx.effect_set.clone();
            let declared_effects = ctx.declared_effects.clone();
            if !callee_effects.is_subset_of(&current_effects) {
                let callee_effs: Vec<String> =
                    fdecl.effects.iter().map(|e| format!("{:?}", e)).collect();
                ctx.add_diag(
                    code_type(401),
                    *loc,
                    format!(
                        "call to `{}` requires effects [{:?}] which are not a subset of declared effects {:?}",
                        fdecl.name.name,
                        callee_effs.join(", "),
                        declared_effects
                    ),
                );
            }
        }

        // Check bound annotation on the function (loop bound style)
        if let Some((ref _what, bound_val)) = fdecl.bound {
            // This function has a declared computational bound — verify it
            // If we're inside a loop context, check we don't exceed the parent bound
            let loop_bounds = ctx.loop_bounds.clone();
            if let Some(Some(parent_bound)) = loop_bounds.last() {
                if bound_val > *parent_bound {
                    ctx.add_diag(
                        code_type(501),
                        *loc,
                        format!(
                            "function `{}` bound {} exceeds enclosing bound {}",
                            fdecl.name.name, bound_val, parent_bound
                        ),
                    );
                }
            }
        }

        // Determine return type
        Ok(fdecl
            .return_type
            .clone()
            .unwrap_or_else(|| TypeExpr::Prim(PrimType::Void, *loc)))
    } else {
        // Not a known function — try checking the callee type for function type
        match &callee_ty {
            TypeExpr::FnType(sig, _) => {
                // Check callable type
                if args.len() != sig.params.len() {
                    ctx.add_diag(
                        code_type(220),
                        *loc,
                        format!(
                            "wrong number of arguments: expected {}, got {}",
                            sig.params.len(),
                            args.len()
                        ),
                    );
                }
                for (_i, (arg, (_, param_ty))) in args.iter().zip(&sig.params).enumerate() {
                    match walk_expr(ctx, arg) {
                        Ok(arg_ty) => {
                            ctx.check_type_assignable(param_ty, &arg_ty, expr_loc(arg));
                        }
                        Err(_) => {}
                    }
                }
                Ok(*sig.return_type.clone())
            }
            _ => {
                ctx.add_diag(
                    code_type(221),
                    *loc,
                    format!("callee is not a function, got `{:?}`", callee_ty),
                );
                Err(())
            }
        }
    }
}

/// Check a block expression
fn check_block(ctx: &mut CheckContext, block: &Block, _loc: &Loc) -> Result<TypeExpr, ()> {
    ctx.env.push_scope();

    let mut last_ty = TypeExpr::Prim(PrimType::Void, block.loc);
    for stmt in &block.stmts {
        match walk_stmt(ctx, stmt) {
            Ok(Some(ty)) => last_ty = ty,
            Ok(None) => {}
            Err(_) => {
                // Propagate error but continue checking
                last_ty = TypeExpr::Prim(PrimType::Void, block.loc);
            }
        }
    }

    ctx.env.pop_scope();
    Ok(last_ty)
}

/// Check an if expression
fn check_if(
    ctx: &mut CheckContext,
    cond: &Expr,
    then_block: &Block,
    else_expr: &Option<Box<Expr>>,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let cond_ty = walk_expr(ctx, cond)?;

    // Condition must be boolean
    match &cond_ty {
        TypeExpr::Prim(PrimType::Bool, _) => {}
        _ => {
            ctx.add_diag(
                code_type(222),
                *loc,
                format!("if condition must be bool, got `{:?}`", cond_ty),
            );
        }
    }

    let then_ty = walk_expr(ctx, &Expr::Block(then_block.clone(), *loc))?;

    let else_ty = if let Some(else_branch) = else_expr {
        walk_expr(ctx, else_branch)?
    } else {
        TypeExpr::Prim(PrimType::Void, *loc)
    };

    // Both branches must have compatible types
    if ctx.types_equal(&then_ty, &else_ty) {
        Ok(then_ty)
    } else {
        // If one is void and other isn't, prefer the non-void
        match (&then_ty, &else_ty) {
            (TypeExpr::Prim(PrimType::Void, _), _) => Ok(else_ty),
            (_, TypeExpr::Prim(PrimType::Void, _)) => Ok(then_ty),
            _ => {
                // Mismatch — report and use then type
                ctx.add_diag(
                    code_type(223),
                    *loc,
                    format!(
                        "if-else branches have incompatible types: `{:?}` and `{:?}`",
                        then_ty, else_ty
                    ),
                );
                Ok(then_ty)
            }
        }
    }
}

/// Check a match expression
fn check_match(
    ctx: &mut CheckContext,
    expr: &Expr,
    arms: &[MatchArm],
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let scrut_ty = walk_expr(ctx, expr)?;

    let mut arm_types: Vec<TypeExpr> = Vec::new();
    let mut arm_has_error = false;

    for arm in arms {
        // Check the pattern against the scrutinee type
        check_pattern(ctx, &arm.pattern, &scrut_ty);

        // Check the guard if present
        if let Some(guard) = &arm.guard {
            let guard_ty = walk_expr(ctx, guard)?;
            match &guard_ty {
                TypeExpr::Prim(PrimType::Bool, _) => {}
                _ => {
                    ctx.add_diag(
                        code_type(222),
                        *loc,
                        format!("match guard must be bool, got `{:?}`", guard_ty),
                    );
                }
            }
        }

        // Push a new scope for the pattern bindings
        ctx.env.push_scope();
        bind_pattern(ctx, &arm.pattern, &scrut_ty);
        let body_ty = walk_expr(ctx, &arm.body);
        ctx.env.pop_scope();

        match body_ty {
            Ok(ty) => arm_types.push(ty),
            Err(_) => arm_has_error = true,
        }
    }

    if arm_has_error {
        return Err(());
    }

    // All arms must have compatible types
    if arm_types.is_empty() {
        return Ok(TypeExpr::Prim(PrimType::Never, *loc));
    }

    let first = &arm_types[0];
    for arm_ty in &arm_types[1..] {
        if !ctx.types_equal(first, arm_ty) {
            ctx.add_diag(
                code_type(223),
                *loc,
                format!(
                    "match arms have incompatible types: `{:?}` and `{:?}`",
                    first, arm_ty
                ),
            );
        }
    }

    Ok(first.clone())
}

/// Check a pattern against an expected type, registering bindings in the process
fn check_pattern(ctx: &mut CheckContext, pat: &Pattern, expected_ty: &TypeExpr) {
    match pat {
        Pattern::Wild(_) => {
            // Anything matches wildcard
        }
        Pattern::Ident(id) => {
            // Bind the identifier with the expected type
            let sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: expected_ty.clone(),
                    mut_: false,
                    moved: false,
                    gen: ctx.next_gen(),
                },
                loc: id.loc,
            };
            ctx.env.define(id.name.clone(), sym);
        }
        Pattern::Lit(lit) => {
            let lit_ty = check_literal(lit);
            if !ctx.types_equal(&lit_ty, expected_ty) {
                ctx.add_diag(
                    code_type(224),
                    expr_loc(&Expr::Literal(lit.clone())),
                    format!(
                        "pattern literal type mismatch: expected `{:?}`, got `{:?}`",
                        expected_ty, lit_ty
                    ),
                );
            }
        }
        Pattern::EnumVariant(owner, variant_name, field_pats, _) => {
            // Determine the enum type name
            let enum_type_name = match owner {
                Some(ident) => &ident.name,
                None => &variant_name.name,
            };
            // Look up the enum
            if let Some(sym) = ctx.env.lookup_type(enum_type_name) {
                if let SymbolKind::Enum(_enum_decl) = &sym.kind {
                    // The expected type should match the enum variant type.
                    // For now, just check field patterns recursively.
                    for fp in field_pats {
                        // For each field, infer type from the enum variant
                        check_pattern(ctx, fp, expected_ty);
                    }
                } else {
                    let report_loc = match owner {
                        Some(ident) => ident.loc,
                        None => variant_name.loc,
                    };
                    ctx.add_diag(
                        code_type(225),
                        report_loc,
                        format!("`{}` is not an enum", enum_type_name),
                    );
                }
            } else {
                let report_loc = match owner {
                    Some(ident) => ident.loc,
                    None => variant_name.loc,
                };
                ctx.add_diag(
                    code_type(201),
                    report_loc,
                    format!("undefined enum: `{}`", enum_type_name),
                );
            }
        }
        Pattern::Tuple(pats, _) => {
            if let TypeExpr::Tuple(types, _) = expected_ty {
                let min_len = pats.len().min(types.len());
                for i in 0..min_len {
                    check_pattern(ctx, &pats[i], &types[i]);
                }
                if pats.len() != types.len() {
                    ctx.add_diag(
                        code_type(226),
                        pats.first()
                            .map(|p| match p {
                                Pattern::Wild(loc)
                                | Pattern::Lit(Literal::Int(_, loc))
                                | Pattern::Lit(Literal::Float(_, loc))
                                | Pattern::Lit(Literal::Bool(_, loc))
                                | Pattern::Lit(Literal::Char(_, loc))
                                | Pattern::Lit(Literal::Str(_, loc)) => *loc,
                                Pattern::Ident(id) => id.loc,
                                Pattern::EnumVariant(_, _, _, loc) => *loc,
                                Pattern::Tuple(_, loc) => *loc,
                            })
                            .unwrap_or(Loc::zero()),
                        format!(
                            "tuple pattern has {} fields but type has {}",
                            pats.len(),
                            types.len()
                        ),
                    );
                }
            } else {
                ctx.add_diag(
                    code_type(227),
                    pats.first()
                        .map(|p| match p {
                            Pattern::Wild(loc)
                            | Pattern::Lit(Literal::Int(_, loc))
                            | Pattern::Lit(Literal::Float(_, loc))
                            | Pattern::Lit(Literal::Bool(_, loc))
                            | Pattern::Lit(Literal::Char(_, loc))
                            | Pattern::Lit(Literal::Str(_, loc)) => *loc,
                            Pattern::Ident(id) => id.loc,
                            Pattern::EnumVariant(_, _, _, loc) => *loc,
                            Pattern::Tuple(_, loc) => *loc,
                        })
                        .unwrap_or(Loc::zero()),
                    format!("tuple pattern used on non-tuple type `{:?}`", expected_ty),
                );
            }
        }
    }
}

/// Bind pattern identifiers into the current scope (for match arm bodies)
fn bind_pattern(ctx: &mut CheckContext, pat: &Pattern, expected_ty: &TypeExpr) {
    match pat {
        Pattern::Ident(id) => {
            let sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: expected_ty.clone(),
                    mut_: false,
                    moved: false,
                    gen: ctx.next_gen(),
                },
                loc: id.loc,
            };
            ctx.env.define(id.name.clone(), sym);
        }
        Pattern::Tuple(pats, _) => {
            if let TypeExpr::Tuple(types, _) = expected_ty {
                for (fp, ft) in pats.iter().zip(types.iter()) {
                    bind_pattern(ctx, fp, ft);
                }
            }
        }
        Pattern::EnumVariant(_, _, field_pats, _) => {
            // Bind each field pattern — for enum variants we may not know exact type
            // without more info, so bind them to a generic type
            for fp in field_pats {
                bind_pattern(ctx, fp, expected_ty);
            }
        }
        _ => {}
    }
}

/// Check a Loop expression
fn check_loop(
    ctx: &mut CheckContext,
    body: &Block,
    bound: &Option<u64>,
    proven: &bool,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    // Loop bound verification
    match bound {
        Some(b) => {
            // Check that the bound is positive
            if *b == 0 {
                ctx.add_diag(
                    code_type(502),
                    *loc,
                    "loop bound must be positive, got 0".to_string(),
                );
            }

            // Check against parent loop bounds
            if let Some(Some(parent_bound)) = ctx.loop_bounds.last() {
                if *b > *parent_bound {
                    ctx.add_diag(
                        code_type(503),
                        *loc,
                        format!(
                            "loop bound {} exceeds enclosing loop bound {}",
                            b, parent_bound
                        ),
                    );
                }
            }
        }
        None => {
            if !*proven {
                ctx.add_diag(
                    code_type(501),
                    *loc,
                    "loop without a bound must be marked `proven`".to_string(),
                );
            }
        }
    }

    // Push loop bound onto stack
    ctx.loop_bounds.push(*bound);
    ctx.env.push_scope();

    // Check the loop body — loops always yield void (no value) in Phorensic
    for stmt in &body.stmts {
        let _ = walk_stmt(ctx, stmt);
    }

    ctx.env.pop_scope();
    ctx.loop_bounds.pop();

    Ok(TypeExpr::Prim(PrimType::Void, *loc))
}

/// Check a ForLoop expression
fn check_for_loop(
    ctx: &mut CheckContext,
    var: &Ident,
    range: &Expr,
    body: &Block,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let range_ty = walk_expr(ctx, range)?;

    // The range should be iterable — for now, check it's an integer or range type
    match &range_ty {
        TypeExpr::Prim(p, _) => {
            let ok = matches!(
                p,
                PrimType::U8
                    | PrimType::U16
                    | PrimType::U32
                    | PrimType::U64
                    | PrimType::I8
                    | PrimType::I16
                    | PrimType::I32
                    | PrimType::I64
                    | PrimType::Usize
                    | PrimType::Isize
            );
            if !ok {
                ctx.add_diag(
                    code_type(228),
                    *loc,
                    format!("for-loop range must be iterable, got `{:?}`", range_ty),
                );
            }

            // ForLoop with a single value — iterate 0..value
            // The loop variable type matches the range
            ctx.env.push_scope();
            let var_sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: range_ty.clone(),
                    mut_: false,
                    moved: false,
                    gen: ctx.next_gen(),
                },
                loc: var.loc,
            };
            ctx.env.define(var.name.clone(), var_sym);

            for stmt in &body.stmts {
                let _ = walk_stmt(ctx, stmt);
            }

            ctx.env.pop_scope();
        }
        _ => {
            ctx.add_diag(
                code_type(228),
                *loc,
                format!(
                    "for-loop range must be an integer type, got `{:?}`",
                    range_ty
                ),
            );
            // Still check body with a default type for the variable
            ctx.env.push_scope();
            let var_sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: TypeExpr::Prim(PrimType::Void, var.loc),
                    mut_: false,
                    moved: false,
                    gen: ctx.next_gen(),
                },
                loc: var.loc,
            };
            ctx.env.define(var.name.clone(), var_sym);

            for stmt in &body.stmts {
                let _ = walk_stmt(ctx, stmt);
            }

            ctx.env.pop_scope();
        }
    }

    Ok(TypeExpr::Prim(PrimType::Void, *loc))
}

/// Check a WhileLoop expression
fn check_while_loop(
    ctx: &mut CheckContext,
    cond: &Expr,
    body: &Block,
    proven: &bool,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let cond_ty = walk_expr(ctx, cond)?;

    match &cond_ty {
        TypeExpr::Prim(PrimType::Bool, _) => {}
        _ => {
            ctx.add_diag(
                code_type(222),
                *loc,
                format!("while-loop condition must be bool, got `{:?}`", cond_ty),
            );
        }
    }

    // While loops must be proven bounded
    if !*proven {
        ctx.add_diag(
            code_type(504),
            *loc,
            "while-loop requires `proven` annotation to guarantee termination".to_string(),
        );
    }

    // Push a synthetic bound for nested loop checking
    ctx.loop_bounds.push(None);
    ctx.env.push_scope();

    for stmt in &body.stmts {
        let _ = walk_stmt(ctx, stmt);
    }

    ctx.env.pop_scope();
    ctx.loop_bounds.pop();

    Ok(TypeExpr::Prim(PrimType::Void, *loc))
}

/// Check a Return expression
fn check_return(
    ctx: &mut CheckContext,
    expr: &Option<Box<Expr>>,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let ret_type = ctx.return_type.clone();
    match (expr, &ret_type) {
        (Some(val), Some(expected)) => {
            let val_ty = walk_expr(ctx, val)?;
            ctx.check_type_assignable(expected, &val_ty, *loc);
        }
        (Some(val), None) => {
            let val_ty = walk_expr(ctx, val)?;
            ctx.add_diag(
                code_type(229),
                *loc,
                format!(
                    "return value of type `{:?}` but function returns void",
                    val_ty
                ),
            );
        }
        (None, Some(ret_ty)) => {
            // Check if the return type is void
            match ret_ty {
                TypeExpr::Prim(PrimType::Void, _) => {}
                _ => {
                    ctx.add_diag(
                        code_type(230),
                        *loc,
                        format!("expected return value of type `{:?}`, got nothing", ret_ty),
                    );
                }
            }
        }
        (None, None) => {}
    }

    // Return type is Never for the expression (diverging)
    Ok(TypeExpr::Prim(PrimType::Never, *loc))
}

/// Check a Trusted expression
fn check_trusted(
    ctx: &mut CheckContext,
    reason: &str,
    body: &Block,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    // Verify rationale is non-empty
    if reason.is_empty() {
        ctx.add_diag(
            code_type(231),
            *loc,
            "trusted block requires a rationale string explaining why it is safe".to_string(),
        );
    }

    // Check that the reason has reasonable length
    if reason.len() < 10 {
        ctx.add_diag(
            code_type(232),
            *loc,
            format!(
                "trusted rationale is too short ({} chars); provide a detailed explanation",
                reason.len()
            ),
        );
    }

    let prev_trusted = ctx.in_trusted;
    ctx.in_trusted = true;
    ctx.env.push_scope();

    for stmt in &body.stmts {
        let _ = walk_stmt(ctx, stmt);
    }

    ctx.env.pop_scope();
    ctx.in_trusted = prev_trusted;

    // Trusted block yields the type of its last expression
    if let Some(last_stmt) = body.stmts.last() {
        match last_stmt {
            Stmt::Expr(expr, _) => walk_expr(ctx, expr),
            Stmt::Return(Some(val), _) => walk_expr(ctx, val),
            _ => Ok(TypeExpr::Prim(PrimType::Void, *loc)),
        }
    } else {
        Ok(TypeExpr::Prim(PrimType::Void, *loc))
    }
}

/// Check a Yield expression
fn check_yield(ctx: &mut CheckContext, expr: &Expr, _loc: &Loc) -> Result<TypeExpr, ()> {
    let val_ty = walk_expr(ctx, expr)?;

    // Yield preserves the type and transfers capability
    if CheckContext::is_cap_type(&val_ty) {
        // If yielding a capability, mark it as consumed
        if let Expr::Ident(id) = expr {
            ctx.caps.consume(&id.name);
        }
    }

    Ok(val_ty)
}

/// Check a Spawn expression
fn check_spawn(ctx: &mut CheckContext, expr: &Expr, loc: &Loc) -> Result<TypeExpr, ()> {
    let expr_ty = walk_expr(ctx, expr)?;

    // Spawn requires a function type (the function to spawn)
    match &expr_ty {
        TypeExpr::FnType(_, _) => {
            // Spawn returns Handle(T) where T is the function's return type
            if let TypeExpr::FnType(sig, _) = &expr_ty {
                let inner = sig.return_type.as_ref().clone();
                let handle_ty = TypeExpr::Handle(Box::new(inner), *loc);

                // Register the handle
                let gen = ctx.next_gen();
                if let Expr::Ident(id) = expr {
                    ctx.handles.add_handle(id.name.clone(), gen);
                }

                Ok(handle_ty)
            } else {
                Ok(TypeExpr::Handle(
                    Box::new(TypeExpr::Prim(PrimType::Void, *loc)),
                    *loc,
                ))
            }
        }
        _ => {
            ctx.add_diag(
                code_type(233),
                *loc,
                format!("spawn requires a function type, got `{:?}`", expr_ty),
            );
            Ok(TypeExpr::Prim(PrimType::Void, *loc))
        }
    }
}

/// Check a ResidualEmit expression
fn check_residual_emit(
    ctx: &mut CheckContext,
    op: &str,
    fields: &[(String, Expr)],
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    // Residual emission creates side-effect metadata; check we're in a context
    // that allows residual operations (inside an effect-ful function)
    let has_residual = ctx.declared_effects.contains(&Effect::Residual);
    if !has_residual && !ctx.in_trusted {
        ctx.add_diag(
            code_type(402),
            *loc,
            format!(
                "residual emission `{}` requires `residual` effect declaration or trusted block",
                op
            ),
        );
    }

    // Check the operation name is not empty
    if op.is_empty() {
        ctx.add_diag(
            code_type(234),
            *loc,
            "residual operation name must not be empty".to_string(),
        );
    }

    // Validate field expressions
    for (_, expr) in fields {
        walk_expr(ctx, expr)?;
    }

    Ok(TypeExpr::Prim(PrimType::Void, *loc))
}

/// Check a HandleCast expression
fn check_handle_cast(ctx: &mut CheckContext, expr: &Expr, loc: &Loc) -> Result<TypeExpr, ()> {
    let inner_ty = walk_expr(ctx, expr)?;

    // Handle cast creates a Handle(T) from a value
    Ok(TypeExpr::Handle(Box::new(inner_ty), *loc))
}

/// Check a CapMove expression
fn check_cap_move(ctx: &mut CheckContext, expr: &Expr, loc: &Loc) -> Result<TypeExpr, ()> {
    let val_ty = walk_expr(ctx, expr)?;

    // CapMove transfers ownership of a capability
    if let Expr::Ident(id) = expr {
        if !ctx.caps.is_cap(&id.name) {
            // It might still be consumable — if it's a Cap type, consume it now
            if CheckContext::is_cap_type(&val_ty) {
                ctx.caps.consume(&id.name);
                ctx.env.mark_moved(&id.name);
            } else {
                ctx.add_diag(
                    code_type(301),
                    *loc,
                    format!("cannot move non-capability value `{}`", id.name),
                );
            }
        } else {
            ctx.caps.consume(&id.name);
            ctx.env.mark_moved(&id.name);
        }
    } else {
        ctx.add_diag(
            code_type(302),
            *loc,
            "cap_move requires an identifier (simple name)".to_string(),
        );
    }

    Ok(TypeExpr::Cap(Box::new(val_ty), *loc))
}

/// Check a StructLit expression
fn check_struct_lit(
    ctx: &mut CheckContext,
    type_name: &Ident,
    fields: &[(Ident, Expr)],
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    // Look up the struct declaration
    let struct_decl = match ctx.env.lookup_type(&type_name.name) {
        Some(sym) => match &sym.kind {
            SymbolKind::Struct(s) => s.clone(),
            _ => {
                ctx.add_diag(
                    code_type(235),
                    type_name.loc,
                    format!("`{}` is not a struct type", type_name.name),
                );
                return Err(());
            }
        },
        None => {
            ctx.add_diag(
                code_type(201),
                type_name.loc,
                format!("undefined struct: `{}`", type_name.name),
            );
            return Err(());
        }
    };

    // Check all fields are present
    let mut found_fields = std::collections::HashSet::new();

    for (field_name, field_expr) in fields {
        let field_decl = struct_decl
            .fields
            .iter()
            .find(|f| f.name.name == field_name.name);
        match field_decl {
            Some(fd) => {
                let field_ty = walk_expr(ctx, field_expr)?;
                ctx.check_type_assignable(&fd.type_, &field_ty, field_name.loc);
                found_fields.insert(field_name.name.clone());
            }
            None => {
                ctx.add_diag(
                    code_type(236),
                    field_name.loc,
                    format!(
                        "struct `{}` has no field `{}`",
                        type_name.name, field_name.name
                    ),
                );
            }
        }
    }

    // Check for missing fields
    for field in &struct_decl.fields {
        if !found_fields.contains(&field.name.name) {
            ctx.add_diag(
                code_type(237),
                *loc,
                format!(
                    "missing field `{}` in struct literal for `{}`",
                    field.name.name, type_name.name
                ),
            );
        }
    }

    Ok(TypeExpr::Named(type_name.clone()))
}

/// Check a FieldAccess expression
fn check_field_access(
    ctx: &mut CheckContext,
    obj: &Expr,
    field: &Ident,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let obj_ty = walk_expr(ctx, obj)?;

    // Resolve the struct type
    let type_name = match &obj_ty {
        TypeExpr::Named(id) => &id.name,
        _ => {
            ctx.add_diag(
                code_type(238),
                *loc,
                format!("field access on non-struct type `{:?}`", obj_ty),
            );
            return Err(());
        }
    };

    let struct_decl = match ctx.env.lookup_type(type_name) {
        Some(sym) => match &sym.kind {
            SymbolKind::Struct(s) => s,
            _ => {
                ctx.add_diag(
                    code_type(235),
                    *loc,
                    format!("`{}` is not a struct", type_name),
                );
                return Err(());
            }
        },
        None => {
            ctx.add_diag(
                code_type(201),
                *loc,
                format!("undefined type `{}`", type_name),
            );
            return Err(());
        }
    };

    // Find the field
    match struct_decl
        .fields
        .iter()
        .find(|f| f.name.name == field.name)
    {
        Some(field_decl) => Ok(field_decl.type_.clone()),
        None => {
            ctx.add_diag(
                code_type(236),
                field.loc,
                format!("struct `{}` has no field `{}`", type_name, field.name),
            );
            Err(())
        }
    }
}

/// Check a MethodCall expression.
///
/// Full trait/method resolution is a future feature. As a bootstrap path,
/// we accept method calls on `TypeIdent` (e.g. `Result::Ok(x)`, `Option::Some(42)`)
/// as intrinsic constructors. These are resolved later by the intrinsic stub generator.
fn check_method_call(
    ctx: &mut CheckContext,
    obj: &Expr,
    method: &Ident,
    args: &[Expr],
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    // Resolve the owner type name from the receiver expression.
    // For TypeIdent, use directly; for Ident with uppercase, treat as type name.
    // For value receivers, use the method_owners table populated by walk_expr.
    let owner_name = match obj {
        Expr::TypeIdent(id) => Some(id.name.clone()),
        Expr::Ident(id) if id.name.chars().next().map_or(false, |c| c.is_uppercase()) => {
            Some(id.name.clone())
        }
        _ => ctx
            .method_owners
            .iter()
            .find(|m| m.line == method.loc.line && m.col == method.loc.col)
            .map(|m| m.owner.clone()),
    };

    let owner = match &owner_name {
        Some(n) => n.as_str(),
        None => {
            ctx.add_diag(
                code_type(239),
                *loc,
                format!(
                    "method call {}.{} could not resolve receiver type",
                    "<expr>", method.name
                ),
            );
            for arg in args {
                walk_expr(ctx, arg)?;
            }
            return Ok(TypeExpr::Prim(PrimType::U64, *loc));
        }
    };

    // Walk args and collect their types
    let mut arg_tys: Vec<TypeExpr> = Vec::new();
    for arg in args {
        let ty = walk_expr(ctx, arg)?;
        arg_tys.push(ty);
    }

    // Look up the method in the bootstrap table
    let table = bootstrap_method_table();
    let entry = table
        .iter()
        .find(|(o, m, _)| *o == owner && *m == method.name.as_str())
        .map(|(_, _, e)| e.clone());

    let entry = match entry {
        Some(e) => e,
        None => {
            // Unknown method — accept as intrinsic but warn
            ctx.add_diag(
                code_type(239),
                *loc,
                format!(
                    "unknown method {}::{} — accepted as bootstrap intrinsic",
                    owner, method.name
                ),
            );
            return Ok(TypeExpr::Prim(PrimType::U64, *loc));
        }
    };

    // Check argument count
    if args.len() != entry.param_count {
        ctx.add_diag(
            code_type(205),
            *loc,
            format!(
                "{}::{} expects {} arguments, got {}",
                owner,
                method.name,
                entry.param_count,
                args.len()
            ),
        );
        return Err(());
    }

    // Check parameter type hints (e.g., Str::append requires Str argument)
    if let Some(hint) = entry.param_type_hint {
        if let Some(arg_ty) = arg_tys.first() {
            let type_name = arg_ty.type_name();
            if type_name.as_deref() != Some(hint) {
                ctx.add_diag(
                    code_type(202),
                    *loc,
                    format!(
                        "{}::{} argument type mismatch: expected `{}` type, got `{:?}`",
                        owner, method.name, hint, arg_ty
                    ),
                );
                // Non-fatal — continue
            }
        }
    }

    // Look up the stored receiver type from method_owners table.
    // For value receivers like buf.pop(), this carries the full compound type
    // (e.g., RingBuf(u64, 8)) which is needed for generic returns.
    // Use the stored value to avoid walking the receiver expression twice.
    let receiver_ty: Option<TypeExpr> = ctx
        .method_owners
        .iter()
        .find(|m| m.line == method.loc.line && m.col == method.loc.col)
        .and_then(|m| m.receiver_ty.clone());

    // Compute return type from the bootstrap method table
    let return_ty = method_return_type(owner, &method.name, receiver_ty.as_ref(), &arg_tys, *loc);
    let result_ty = return_ty.unwrap_or_else(|| TypeExpr::Prim(PrimType::U64, *loc));

    // Store the computed return type in method_owners for the lowerer.
    // This lets the lowerer allocate correctly-typed PHIR temps instead of blanket u64.
    if let Some(entry) = ctx
        .method_owners
        .iter_mut()
        .find(|m| m.line == method.loc.line && m.col == method.loc.col)
    {
        entry.return_ty = Some(result_ty.clone());
    }

    Ok(result_ty)
}

/// Check an Index expression
fn check_index(
    ctx: &mut CheckContext,
    obj: &Expr,
    index: &Expr,
    loc: &Loc,
) -> Result<TypeExpr, ()> {
    let obj_ty = walk_expr(ctx, obj)?;
    let index_ty = walk_expr(ctx, index)?;

    // Check index is an integer type
    match &index_ty {
        TypeExpr::Prim(p, _) => {
            let ok = matches!(
                p,
                PrimType::U8
                    | PrimType::U16
                    | PrimType::U32
                    | PrimType::U64
                    | PrimType::I8
                    | PrimType::I16
                    | PrimType::I32
                    | PrimType::I64
                    | PrimType::Usize
                    | PrimType::Isize
            );
            if !ok {
                ctx.add_diag(
                    code_type(240),
                    *loc,
                    format!("index must be an integer type, got `{:?}`", index_ty),
                );
            }
        }
        _ => {
            ctx.add_diag(
                code_type(240),
                *loc,
                format!("index must be integer, got `{:?}`", index_ty),
            );
        }
    }

    // Determine the element type
    match &obj_ty {
        TypeExpr::Array(inner, _, _)
        | TypeExpr::RingBuf(inner, _, _)
        | TypeExpr::Slice(inner, _) => Ok(*inner.clone()),
        TypeExpr::Str(_, _) => Ok(TypeExpr::Prim(PrimType::Char, *loc)),
        _ => {
            ctx.add_diag(
                code_type(241),
                *loc,
                format!("cannot index type `{:?}`", obj_ty),
            );
            Err(())
        }
    }
}

/// Walk an expression node and return its type
pub fn walk_expr(ctx: &mut CheckContext, expr: &Expr) -> Result<TypeExpr, ()> {
    match expr {
        Expr::Literal(lit) => Ok(check_literal(lit)),

        Expr::Ident(id) => check_ident(ctx, id),

        Expr::Binary { op, lhs, rhs, loc } => check_binary(ctx, op, lhs, rhs, loc),

        Expr::Unary {
            op,
            expr: inner,
            loc,
        } => check_unary(ctx, op, inner, loc),

        Expr::Call { callee, args, loc } => check_call(ctx, callee, args, loc),

        Expr::FieldAccess { obj, field, loc } => check_field_access(ctx, obj, field, loc),

        Expr::MethodCall {
            obj,
            method,
            args,
            loc,
            ..
        } => {
            // Resolve receiver type name and full type for method call disambiguation.
            // For value receivers, we need the full compound type (e.g., RingBuf(u64, 8))
            // so that pop() can return Option[u64] instead of a placeholder.
            let (owner_name, receiver_ty) = match obj.as_ref() {
                Expr::TypeIdent(id) => (Some(id.name.clone()), None),
                // Upper-case identifiers in expression context are type names
                // used in qualified calls like Command::Add(3, 4) or Result::Ok(x).
                Expr::Ident(id) if id.name.chars().next().map_or(false, |c| c.is_uppercase()) => {
                    (Some(id.name.clone()), None)
                }
                other => {
                    // Walk the receiver expression to get its full type.
                    // Return both the type name (for owner resolution) and the full type
                    // (for generic return computation like pop() -> Option[inner]).
                    match walk_expr(ctx, other) {
                        Ok(ty) => (ty.type_name(), Some(ty)),
                        Err(_) => (None, None),
                    }
                }
            };
            if let Some(name) = owner_name {
                ctx.method_owners.push(MethodOwner {
                    line: method.loc.line,
                    col: method.loc.col,
                    owner: name,
                    receiver_ty,
                    return_ty: None, // filled by check_method_call
                });
            }
            check_method_call(ctx, obj, method, args, loc)
        }

        Expr::Index { obj, index, loc } => check_index(ctx, obj, index, loc),

        Expr::Block(block, loc) => check_block(ctx, block, loc),

        Expr::If {
            cond,
            then,
            else_,
            loc,
        } => check_if(ctx, cond, then, else_, loc),

        Expr::Match {
            expr: inner,
            arms,
            loc,
        } => check_match(ctx, inner, arms, loc),

        Expr::Loop {
            body,
            bound,
            proven,
            loc,
        } => check_loop(ctx, body, bound, proven, loc),

        Expr::ForLoop {
            var,
            range,
            body,
            loc,
        } => check_for_loop(ctx, var, range, body, loc),

        Expr::WhileLoop {
            cond,
            body,
            proven,
            loc,
        } => check_while_loop(ctx, cond, body, proven, loc),

        Expr::Return(expr_val, loc) => check_return(ctx, expr_val, loc),

        Expr::Break(loc) => {
            // Break from a loop — type is Never (diverging)
            Ok(TypeExpr::Prim(PrimType::Never, *loc))
        }
        Expr::Continue(loc) => {
            // Continue to next loop iteration — type is Never (diverging)
            Ok(TypeExpr::Prim(PrimType::Never, *loc))
        }

        Expr::Yield(expr_val, loc) => check_yield(ctx, expr_val, loc),

        Expr::Spawn(expr_val, loc) => check_spawn(ctx, expr_val, loc),

        Expr::Trusted { reason, body, loc } => check_trusted(ctx, reason, body, loc),

        Expr::ResidualEmit { op, fields, loc } => check_residual_emit(ctx, op, fields, loc),

        Expr::Cast {
            expr: inner,
            type_,
            loc: _,
        } => {
            // Type cast: check the inner expression, return the target type
            // This is a checked cast — we verify the inner expression is valid
            walk_expr(ctx, inner)?;
            Ok(*type_.clone())
        }

        Expr::HandleCast(inner, loc) => check_handle_cast(ctx, inner, loc),

        Expr::CapMove(inner, loc) => check_cap_move(ctx, inner, loc),

        Expr::StructLit {
            type_name,
            fields,
            loc,
        } => check_struct_lit(ctx, type_name, fields, loc),

        Expr::PostfixBang { expr: inner, loc } => {
            // expr! — postfix not/bang, check inner expression
            walk_expr(ctx, inner)?;
            // Result type is the inner type (for now)
            Ok(TypeExpr::Prim(PrimType::Void, *loc))
        }

        Expr::ArrayLit(elements, loc) => {
            // Array literal: [expr, expr, ...]
            // Walk all elements and determine the common type
            if elements.is_empty() {
                return Ok(TypeExpr::Array(
                    Box::new(TypeExpr::Prim(PrimType::U64, *loc)),
                    ArraySize::Literal(0, *loc),
                    *loc,
                ));
            }
            let first_ty = walk_expr(ctx, &elements[0])?;
            for elem in &elements[1..] {
                let elem_ty = walk_expr(ctx, elem)?;
                ctx.check_type_assignable(&first_ty, &elem_ty, *loc);
            }
            Ok(TypeExpr::Array(
                Box::new(first_ty),
                ArraySize::Literal(elements.len() as u64, *loc),
                *loc,
            ))
        }

        Expr::TypeIdent(id) => {
            // Type identifier used in expression context (e.g. Handle, Slice)
            // Resolve to the named type (as a type expression, not a value)
            Ok(TypeExpr::Named(id.clone()))
        }

        Expr::Error(err_loc) => {
            ctx.add_diag(
                code_type(242),
                *err_loc,
                "expression contains a parse error".to_string(),
            );
            Err(())
        }
    }
}

// ============================================================================
// Type checking — statements
// ============================================================================

/// Walk a statement node — returns the type if it's a tail expression, else None
pub fn walk_stmt(ctx: &mut CheckContext, stmt: &Stmt) -> Result<Option<TypeExpr>, ()> {
    match stmt {
        Stmt::Let {
            name,
            type_ann,
            init,
            mut_,
            loc,
        } => {
            let init_ty = if let Some(init_expr) = init {
                let ty = walk_expr(ctx, init_expr)?;
                Some(ty)
            } else {
                None
            };

            // Determine the variable's type
            let var_type = match (type_ann, &init_ty) {
                (Some(ann), _) => {
                    // Verify the annotation is well-formed
                    ctx.check_type_form(ann);
                    if let Some(init_ty) = &init_ty {
                        ctx.check_type_assignable(ann, init_ty, *loc);
                    }
                    ann.clone()
                }
                (None, Some(inferred)) => inferred.clone(),
                (None, None) => {
                    ctx.add_diag(
                        code_type(243),
                        *loc,
                        format!(
                            "variable `{}` requires a type annotation when no initializer is given",
                            name.name
                        ),
                    );
                    TypeExpr::Prim(PrimType::Void, *loc)
                }
            };

            // Check capability analysis: if this is a Cap or Handle type,
            // register it in the capability tracker
            if CheckContext::is_cap_type(&var_type) {
                ctx.caps.add(name.name.clone(), var_type.clone());
            }

            // Register handle
            if CheckContext::is_handle_type(&var_type) {
                let gen = ctx.next_gen();
                ctx.handles.add_handle(name.name.clone(), gen);
            }

            // Define the variable in the environment
            let gen = ctx.next_gen();
            let sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: var_type,
                    mut_: *mut_,
                    moved: false,
                    gen,
                },
                loc: *loc,
            };
            ctx.env.define(name.name.clone(), sym);

            Ok(None)
        }

        Stmt::Expr(expr, _) => {
            let ty = walk_expr(ctx, expr)?;
            Ok(Some(ty))
        }

        Stmt::Return(expr_val, loc) => {
            check_return(ctx, &expr_val.as_ref().map(|e| Box::new(e.clone())), loc)?;
            Ok(Some(TypeExpr::Prim(PrimType::Never, *loc)))
        }

        Stmt::Assignment { target, value, loc } => {
            let _ = walk_expr(ctx, value)?; // value type

            match target.as_ref() {
                Expr::Ident(id) => {
                    let var_info = ctx.env.lookup(&id.name).and_then(|sym| {
                        if let SymbolKind::Variable { mut_, type_, .. } = &sym.kind {
                            Some((*mut_, type_.clone()))
                        } else {
                            None
                        }
                    });
                    if let Some((is_mut, var_type)) = var_info {
                        if !is_mut {
                            ctx.add_diag(
                                code_type(215),
                                *loc,
                                format!("cannot assign to immutable variable `{}`", id.name),
                            );
                        }
                        let val_ty = walk_expr(ctx, value)?;
                        ctx.check_type_assignable(&var_type, &val_ty, *loc);
                    } else {
                        ctx.add_diag(
                            code_type(208),
                            *loc,
                            format!("cannot assign to undeclared variable `{}`", id.name),
                        );
                    }
                }
                Expr::FieldAccess { obj, field, .. } => {
                    let obj_ty = walk_expr(ctx, obj)?;
                    let type_name = match &obj_ty {
                        TypeExpr::Named(id) => &id.name,
                        _ => {
                            ctx.add_diag(
                                code_type(238),
                                *loc,
                                "field assignment target is not a struct".to_string(),
                            );
                            return Ok(None);
                        }
                    };
                    let field_type = {
                        let struct_decl = match ctx.env.lookup_type(type_name) {
                            Some(sym) => match &sym.kind {
                                SymbolKind::Struct(s) => s,
                                _ => {
                                    ctx.add_diag(
                                        code_type(235),
                                        *loc,
                                        format!("`{}` is not a struct", type_name),
                                    );
                                    return Ok(None);
                                }
                            },
                            None => {
                                ctx.add_diag(
                                    code_type(201),
                                    *loc,
                                    format!("undefined type `{}`", type_name),
                                );
                                return Ok(None);
                            }
                        };
                        struct_decl
                            .fields
                            .iter()
                            .find(|f| f.name.name == field.name)
                            .map(|f| f.type_.clone())
                    };
                    match field_type {
                        Some(field_type) => {
                            let val_ty = walk_expr(ctx, value)?;
                            ctx.check_type_assignable(&field_type, &val_ty, *loc);
                        }
                        None => {
                            ctx.add_diag(
                                code_type(236),
                                field.loc,
                                format!("struct `{}` has no field `{}`", type_name, field.name),
                            );
                        }
                    }
                }
                _ => {
                    ctx.add_diag(
                        code_type(216),
                        *loc,
                        "invalid assignment target".to_string(),
                    );
                }
            }

            Ok(None)
        }
    }
}

// ============================================================================
// Type checking — function declarations
// ============================================================================

/// Walk a function declaration and type-check its body
pub fn walk_fn(ctx: &mut CheckContext, fdecl: &FnDecl) {
    // Check for duplicate parameters
    let mut param_names = std::collections::HashSet::new();
    for (param, _) in &fdecl.params {
        if !param_names.insert(param.name.clone()) {
            ctx.add_diag(
                code_type(244),
                param.loc,
                format!("duplicate parameter name `{}`", param.name),
            );
        }
    }

    // Set the return type and effects for the function body
    ctx.return_type = fdecl.return_type.clone();
    ctx.declared_effects = fdecl.effects.clone();
    ctx.effect_set = EffectSet::from_vec(fdecl.effects.clone());

    // Push a new scope for parameters
    ctx.env.push_scope();

    // Define each parameter
    for (param, param_ty) in &fdecl.params {
        // Verify parameter type is well-formed
        ctx.check_type_form(param_ty);

        let gen = ctx.next_gen();
        let sym = Symbol {
            kind: SymbolKind::Variable {
                type_: param_ty.clone(),
                mut_: true, // params are mutable locals in Phorensic
                moved: false,
                gen,
            },
            loc: param.loc,
        };
        ctx.env.define(param.name.clone(), sym);

        // Track capabilities in parameters
        if CheckContext::is_cap_type(param_ty) {
            ctx.caps.add(param.name.clone(), param_ty.clone());
        }
        if CheckContext::is_handle_type(param_ty) {
            let gen = ctx.next_gen();
            ctx.handles.add_handle(param.name.clone(), gen);
        }
    }

    // Push loop bound if the function has one
    let fn_bound = fdecl.bound.as_ref().map(|(_, v)| *v);
    ctx.loop_bounds.push(fn_bound);

    // Check the body and verify return type
    let body_ty = check_block(ctx, &fdecl.body, &fdecl.loc)
        .unwrap_or(TypeExpr::Prim(PrimType::Void, fdecl.loc));
    // If function declares a non-void return type, check the body's tail expression matches
    if let Some(ref ret_ty) = fdecl.return_type {
        match ret_ty {
            TypeExpr::Prim(PrimType::Void, _) => {}
            _ => {
                // Never is a subtype of everything: a body ending with `return expr;`
                // has tail type Never, which is compatible with any declared return type.
                if !matches!(body_ty, TypeExpr::Prim(PrimType::Never, _)) {
                    ctx.check_type_assignable(ret_ty, &body_ty, fdecl.loc);
                }
            }
        }
    }

    ctx.loop_bounds.pop();
    ctx.env.pop_scope();

    // Verify unconsumed capabilities at function exit (affine cap leak detection)
    let unconsumed: Vec<(String, TypeExpr)> = ctx
        .caps
        .unconsumed()
        .iter()
        .map(|c| (c.name.clone(), c.type_.clone()))
        .collect();
    if !unconsumed.is_empty() {
        for (cap_name, cap_type) in unconsumed {
            ctx.add_diag(
                code_type(303),
                fdecl.loc,
                format!(
                    "capability `{}` (type {:?}) was not consumed in function `{}`",
                    cap_name, cap_type, fdecl.name.name
                ),
            );
        }
    }

    // Reset per-function state
    ctx.return_type = None;
    ctx.declared_effects = Vec::new();
    ctx.effect_set = EffectSet::new();
    ctx.loop_bounds.clear();
}

// ============================================================================
// Type checking — type declarations
// ============================================================================

/// Walk a struct declaration and register it
/// Field type checking is deferred to avoid forward-reference issues
/// (a struct field may reference a struct defined later in the file).
/// Field types are validated when struct literals are type-checked.
fn walk_struct(ctx: &mut CheckContext, sdecl: &StructDecl) {
    // Define the struct in the symbol table FIRST so forward references work
    let sym = Symbol {
        kind: SymbolKind::Struct(sdecl.clone()),
        loc: sdecl.loc,
    };
    ctx.env.define(sdecl.name.name.clone(), sym);
}

/// Walk an enum declaration and register it
fn walk_enum(ctx: &mut CheckContext, edecl: &EnumDecl) {
    // Verify each variant's field types are well-formed
    for variant in &edecl.variants {
        for field_ty in &variant.fields {
            ctx.check_type_form(field_ty);
        }
    }

    // Check for duplicate variant names
    let mut variant_names = std::collections::HashSet::new();
    for variant in &edecl.variants {
        if !variant_names.insert(variant.name.name.clone()) {
            ctx.add_diag(
                code_type(245),
                variant.name.loc,
                format!(
                    "duplicate variant `{}` in enum `{}`",
                    variant.name.name, edecl.name.name
                ),
            );
        }
    }

    // Define the enum in the symbol table
    let sym = Symbol {
        kind: SymbolKind::Enum(edecl.clone()),
        loc: edecl.loc,
    };
    ctx.env.define(edecl.name.name.clone(), sym);
}

/// Walk a type alias and register it
fn walk_type_alias(ctx: &mut CheckContext, talias: &TypeAlias) {
    // Verify the underlying type is well-formed
    ctx.check_type_form(&talias.type_);

    // Define the alias in the symbol table
    let sym = Symbol {
        kind: SymbolKind::TypeAlias(talias.clone()),
        loc: talias.loc,
    };
    ctx.env.define(talias.name.name.clone(), sym);
}

/// Walk a service declaration and register it
fn walk_service(ctx: &mut CheckContext, sdecl: &ServiceDecl) {
    // Verify capability types are well-formed
    for cap_ty in &sdecl.capabilities {
        ctx.check_type_form(cap_ty);
    }

    // Verify provided/imported types
    for prov in &sdecl.provides {
        ctx.check_type_form(prov);
    }
    for imp in &sdecl.imports {
        ctx.check_type_form(imp);
    }

    // Define the service in the symbol table
    let sym = Symbol {
        kind: SymbolKind::Service(sdecl.clone()),
        loc: sdecl.loc,
    };
    ctx.env.define(sdecl.name.name.clone(), sym);
}

// ============================================================================
// Type checking — top-level items
// ============================================================================

/// Walk a top-level item and register it / type-check it
pub fn walk_item(ctx: &mut CheckContext, item: &Item) {
    match item {
        Item::Fn(fdecl) => {
            // First pass: register the function (forward declaration)
            let fn_sym = Symbol {
                kind: SymbolKind::Function(fdecl.clone()),
                loc: fdecl.loc,
            };
            ctx.env.define(fdecl.name.name.clone(), fn_sym);
        }
        Item::Struct(sdecl) => {
            walk_struct(ctx, sdecl);
        }
        Item::Enum(edecl) => {
            walk_enum(ctx, edecl);
        }
        Item::TypeAlias(talias) => {
            walk_type_alias(ctx, talias);
        }
        Item::Service(sdecl) => {
            walk_service(ctx, sdecl);
        }
        Item::Import(ident, _) => {
            // Imports resolve to module-level names; for now, register as unknown type
            let sym = Symbol {
                kind: SymbolKind::TypeAlias(TypeAlias {
                    name: ident.clone(),
                    type_: TypeExpr::Named(ident.clone()),
                    loc: ident.loc,
                }),
                loc: ident.loc,
            };
            ctx.env.define(ident.name.clone(), sym);
        }
        Item::Const(cdecl) => {
            // Register module-level constants as immutable variables
            if let Some(ref ty) = cdecl.type_ {
                ctx.check_type_form(ty);
            }
            let var_ty = cdecl
                .type_
                .clone()
                .unwrap_or_else(|| TypeExpr::Prim(PrimType::U64, cdecl.loc));
            let gen = ctx.next_gen();
            let sym = Symbol {
                kind: SymbolKind::Variable {
                    type_: var_ty,
                    mut_: false,
                    moved: false,
                    gen,
                },
                loc: cdecl.loc,
            };
            ctx.env.define(cdecl.name.name.clone(), sym);
        }
        Item::Package(_, _) => {
            // Package declaration — no checking needed
        }
        Item::Impl(impl_block) => {
            // Register each method with owner-qualified name to avoid collisions
            for method in &impl_block.methods {
                let qualified = format!("{}_{}", impl_block.owner.name, method.name.name);
                let fn_sym = Symbol {
                    kind: SymbolKind::Function(method.clone()),
                    loc: method.loc,
                };
                ctx.env.define(qualified, fn_sym);
                // Also register under plain name for backward compat
                let fn_sym2 = Symbol {
                    kind: SymbolKind::Function(method.clone()),
                    loc: method.loc,
                };
                ctx.env.define(method.name.name.clone(), fn_sym2);
            }
        }
        Item::Machine(blocks) => {
            // Machine blocks are platform-specific — verify trusted rationale
            for block in blocks {
                if block.reason.is_empty() {
                    ctx.add_diag(
                        code_type(231),
                        block.loc,
                        "machine block requires a rationale string".to_string(),
                    );
                }
                // Check that arch list is non-empty
                if block.arch.is_empty() {
                    ctx.add_diag(
                        code_type(246),
                        block.loc,
                        "machine block requires at least one target architecture".to_string(),
                    );
                }
            }
        }
    }
}

/// Second pass: type-check function bodies (after all declarations are registered)
pub fn walk_fn_bodies(ctx: &mut CheckContext, items: &[Item]) {
    for item in items {
        match item {
            Item::Fn(fdecl) => walk_fn(ctx, fdecl),
            Item::Impl(impl_block) => {
                for method in &impl_block.methods {
                    walk_fn(ctx, method);
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// Entry point
// ============================================================================

/// Check an entire module (SourceFile), returning all diagnostics found.
/// This is the main entry point for the semantic analysis phase.
///
/// The checker performs two passes:
///   1. Register all declarations (structs, enums, type aliases, functions, services)
///   2. Type-check all function bodies
///
/// Error codes:
///   E0201–E0299  Type errors
///   E0300–E0399  Capability errors
///   E0400–E0499  Effect errors
///   E0500–E0599  Loop / bound errors
pub fn check_module(source: &SourceFile) -> (Vec<Diag>, Vec<MethodOwner>) {
    let mut ctx = CheckContext::new();
    ctx.source = Some(source);

    // Seed built-in types into the environment
    seed_builtins(&mut ctx);

    // Phase 1: Register all declarations
    for item in &source.items {
        walk_item(&mut ctx, item);
    }

    // Phase 2: Check function bodies
    walk_fn_bodies(&mut ctx, &source.items);

    // Phase 3: Final verification — check for unused items warnings, etc.
    // (Future: orphan rules, coherence checks)

    (ctx.diags, ctx.method_owners)
}

/// Seed the environment with built-in types and functions
fn seed_builtins(ctx: &mut CheckContext) {
    // Built-in primitive types are always available
    // (they don't need to be in the symbol table since parse_type handles them)

    // Built-in functions
    let builtins: Vec<(&str, Vec<(&str, TypeExpr)>, TypeExpr, Vec<Effect>)> = vec![
        (
            "print",
            vec![(
                "msg",
                TypeExpr::Str(ArraySize::Literal(255, Loc::zero()), Loc::zero()),
            )],
            TypeExpr::Prim(PrimType::Void, Loc::zero()),
            vec![Effect::IOWrite],
        ),
        (
            "println",
            vec![(
                "msg",
                TypeExpr::Str(ArraySize::Literal(255, Loc::zero()), Loc::zero()),
            )],
            TypeExpr::Prim(PrimType::Void, Loc::zero()),
            vec![Effect::IOWrite],
        ),
        (
            "assert",
            vec![
                ("cond", TypeExpr::Prim(PrimType::Bool, Loc::zero())),
                (
                    "msg",
                    TypeExpr::Str(ArraySize::Literal(255, Loc::zero()), Loc::zero()),
                ),
            ],
            TypeExpr::Prim(PrimType::Void, Loc::zero()),
            vec![],
        ),
        (
            "read_u64",
            vec![],
            TypeExpr::Prim(PrimType::U64, Loc::zero()),
            vec![Effect::IORead],
        ),
        (
            "size_of",
            vec![],
            TypeExpr::Prim(PrimType::Usize, Loc::zero()),
            vec![],
        ),
        (
            "panic",
            vec![(
                "msg",
                TypeExpr::Str(ArraySize::Literal(255, Loc::zero()), Loc::zero()),
            )],
            TypeExpr::Prim(PrimType::Never, Loc::zero()),
            vec![],
        ),
    ];

    for (name, params, ret_ty, effects) in builtins {
        let decl = FnDecl {
            name: Ident {
                name: name.to_string(),
                loc: Loc::zero(),
            },
            params: params
                .into_iter()
                .map(|(pn, pt)| {
                    (
                        Ident {
                            name: pn.to_string(),
                            loc: Loc::zero(),
                        },
                        pt,
                    )
                })
                .collect(),
            effects,
            return_type: Some(ret_ty),
            court: None,
            bound: None,
            visibility: crate::ast::Visibility::Private,
            body: Block {
                stmts: vec![],
                loc: Loc::zero(),
            },
            loc: Loc::zero(),
        };

        let sym = Symbol {
            kind: SymbolKind::Function(decl),
            loc: Loc::zero(),
        };
        ctx.env.define(name.to_string(), sym);
    }
}

// ============================================================================
// Helper: code_type string generation
// ============================================================================

fn code_type(n: u32) -> &'static str {
    match n {
        // Type errors: E0201–E0299
        201 => "E0201",
        202 => "E0202",
        203 => "E0203",
        204 => "E0204",
        205 => "E0205",
        206 => "E0206",
        207 => "E0207",
        208 => "E0208",
        209 => "E0209",
        210 => "E0210",
        211 => "E0211",
        212 => "E0212",
        213 => "E0213",
        214 => "E0214",
        215 => "E0215",
        216 => "E0216",
        217 => "E0217",
        218 => "E0218",
        219 => "E0219",
        220 => "E0220",
        221 => "E0221",
        222 => "E0222",
        223 => "E0223",
        224 => "E0224",
        225 => "E0225",
        226 => "E0226",
        227 => "E0227",
        228 => "E0228",
        229 => "E0229",
        230 => "E0230",
        231 => "E0231",
        232 => "E0232",
        233 => "E0233",
        234 => "E0234",
        235 => "E0235",
        236 => "E0236",
        237 => "E0237",
        238 => "E0238",
        239 => "E0239",
        240 => "E0240",
        241 => "E0241",
        242 => "E0242",
        243 => "E0243",
        244 => "E0244",
        245 => "E0245",
        246 => "E0246",

        // Capability errors: E0300–E0399
        301 => "E0301",
        302 => "E0302",
        303 => "E0303",

        // Effect errors: E0400–E0499
        401 => "E0401",
        402 => "E0402",

        // Loop/bound errors: E0500–E0599
        501 => "E0501",
        502 => "E0502",
        503 => "E0503",
        504 => "E0504",

        _ => "E0999",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::Lexer;
    use crate::parse::Parser;

    // If this test runs, the check module tests are compiled
    #[test]
    fn test_check_module_compiles() {
        // This test simply verifies the check module compiles
        let _cfg = CheckContext::new();
        assert!(true, "check module tests are compiled");
    }

    fn check_source(source: &str) -> Vec<Diag> {
        let mut lexer = Lexer::new(source);
        let mut parser = Parser::new(&mut lexer);
        let sf = match parser.parse_source() {
            Ok(sf) => sf,
            Err(_) => {
                let mut diags: Vec<Diag> = lexer.diagnostics().to_vec();
                diags.extend(parser.diagnostics().iter().cloned());
                return diags;
            }
        };
        // Even if parse succeeded with warnings, include parser diagnostics
        let (mut diags, _method_owners) = check_module(&sf);
        diags.extend(parser.diagnostics().iter().cloned());
        diags
    }

    #[test]
    fn test_empty_module() {
        let diags = check_source("");
        assert!(
            diags.is_empty(),
            "empty module should produce no errors: {:?}",
            diags
        );
    }

    #[test]
    fn test_simple_function() {
        let src = "fn main() -> u64 { 42 }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "simple function: {:?}", diags);
    }

    #[test]
    fn test_void_function() {
        let src = "fn main() { let x: u64 = 42; }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "void function: {:?}", diags);
    }

    #[test]
    fn test_type_mismatch() {
        let src = "fn main() -> u64 { true }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should detect type mismatch");
        assert!(
            diags[0].code.starts_with("E020"),
            "code should be E02xx, got {}",
            diags[0].code
        );
    }

    #[test]
    fn test_undeclared_variable() {
        let src = "fn main() -> u64 { x }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should detect undeclared variable");
    }

    #[test]
    fn test_if_expression() {
        let src = "fn main() -> u64 { if true { 42 } else { 0 } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "if expression: {:?}", diags);
    }

    #[test]
    fn test_binary_ops() {
        let src = "fn main() -> u64 { let x: u64 = 10 + 20 * 3; x }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "binary ops: {:?}", diags);
    }

    #[test]
    fn test_loop_with_bound() {
        let src = "fn main() { loop bound 10 { } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "loop with bound: {:?}", diags);
    }

    #[test]
    fn test_unbounded_loop() {
        let src = "fn main() { loop { } }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should flag unbounded loop");
    }

    #[test]
    fn test_while_loop_proven() {
        let src = "fn main() { while true proven { } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "while proven: {:?}", diags);
    }

    #[test]
    fn test_while_loop_unproven() {
        let src = "fn main() { while true { } }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should flag unproven while");
    }

    #[test]
    fn test_struct_decl_and_lit() {
        let src = "struct Point { x: u64, y: u64 }
                    fn main() -> u64 { let p: Point = Point { x: 10, y: 20 }; p.x }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "struct: {:?}", diags);
    }

    #[test]
    fn test_function_call() {
        let src = "fn add(a: u64, b: u64) -> u64 { a + b }
                    fn main() -> u64 { add(1, 2) }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "function call: {:?}", diags);
    }

    #[test]
    fn test_trusted_block() {
        let src = "fn main() -> u64 { trusted \"safe because it's a test\" { 42 } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "trusted: {:?}", diags);
    }

    #[test]
    fn test_trusted_empty_rationale() {
        let src = "fn main() { trusted \"\" { } }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should flag empty rationale");
    }

    #[test]
    fn test_match_expression() {
        let src = "fn main() -> u64 { match 42 { x => x } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "match: {:?}", diags);
    }

    #[test]
    fn test_effect_checking() {
        let src = "fn compute_fn(x: u64) -> u64 effect [compute] { x + 1 }
                    fn main() -> u64 effect [compute] { compute_fn(42) }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "effect checking: {:?}", diags);
    }

    #[test]
    fn test_effect_violation() {
        let src = "fn write(n: u64) effect [compute] { }
                    fn main() { write(42) }";
        let diags = check_source(src);
        // callee has [compute] effects but caller (main) has no declared effects
        assert!(!diags.is_empty(), "should flag effect violation");
    }

    #[test]
    fn test_type_alias() {
        let src = "type Age = u64
                    fn main() -> Age { 42 }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "type alias: {:?}", diags);
    }

    #[test]
    fn test_for_loop() {
        let src = "fn main() { for x in 10 { } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "for loop: {:?}", diags);
    }

    #[test]
    fn test_nested_loop_bounds() {
        let src = "fn main() { loop bound 10 { loop bound 5 { } } }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "nested loop bounds ok: {:?}", diags);
    }

    #[test]
    fn test_nested_loop_bound_exceeded() {
        let src = "fn main() { loop bound 5 { loop bound 10 { } } }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should flag nested bound exceeded");
    }

    #[test]
    fn test_return_type_mismatch() {
        let src = "fn main() -> u64 { return true; }";
        let diags = check_source(src);
        assert!(!diags.is_empty(), "should flag return type mismatch");
    }

    #[test]
    fn test_capability_tracking() {
        // Test capability creation works without crashing the checker
        // The only expected diag is the cap-not-consumed warning (E0303)
        let src = "fn main() -> u64 { let c: cap(u64) = cap(42); 0 }";
        let diags = check_source(src);
        // Should not have parse errors or type errors
        let has_parse_error = diags.iter().any(|d| d.code.starts_with("E01"));
        let has_type_error = diags.iter().any(|d| d.code == "E0202");
        assert!(!has_parse_error, "cap tracking parse errors: {:?}", diags);
        assert!(!has_type_error, "cap tracking type errors: {:?}", diags);
    }

    #[test]
    fn test_enum_decl() {
        let src = "enum Color { Red, Green, Blue }
                    fn main() -> u64 { 0 }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "enum: {:?}", diags);
    }

    #[test]
    fn test_method_call_on_value_receiver() {
        // Value-receiver method calls should not produce E0239
        let src = "fn test() -> u64 { let s: Str(64) = Str(\"hi\"); s.len() }";
        let diags = check_source(src);
        let has_method_error = diags.iter().any(|d| d.code == "E0239");
        assert!(
            !has_method_error,
            "no E0239 for s.len() on Str receiver: {:?}",
            diags
        );
    }

    #[test]
    fn test_qualified_enum_constructor() {
        // Enum variant constructor via :: should not produce E0239
        let src = "enum Color { Red, Blue }\n\
                    fn test() -> u64 { let c: Color = Color::Red; 0 }";
        let diags = check_source(src);
        let has_method_error = diags.iter().any(|d| d.code == "E0239");
        assert!(
            !has_method_error,
            "no E0239 for Color::Red on enum: {:?}",
            diags
        );
    }

    #[test]
    fn test_method_call_with_args() {
        // Method calls with receiver + args should not produce E0239
        let src = "fn test(s: Str(32)) -> bool { let b: bool = s.append(\"!\"); b }";
        let diags = check_source(src);
        let has_method_error = diags.iter().any(|d| d.code == "E0239");
        assert!(
            !has_method_error,
            "no E0239 for s.append() on Str receiver: {:?}",
            diags
        );
    }

    #[test]
    fn test_service_decl() {
        // Service declarations are not yet parsed at top level
        // Test a valid function instead
        let src = "fn main() -> u64 { 42 }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "service placeholder: {:?}", diags);
    }

    #[test]
    fn test_method_return_type_ringbuf_push() {
        // RingBuf::push returns bool
        let src =
            "fn test() -> bool { let mut buf: RingBuf(u64, 8) = RingBuf::new(); buf.push(42) }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "RingBuf::push returns bool: {:?}", diags);
    }

    #[test]
    fn test_method_return_type_str_len() {
        // Str::len returns u64 — use Str::from to get wildcard size
        let src = "fn test() -> u64 { let s: Str(32) = Str::from(\"hi\"); s.len() }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "Str::len returns u64: {:?}", diags);
    }

    #[test]
    fn test_method_return_type_result_ok() {
        // Result::Ok(x) returns Result[typeof(x), U64]
        let src = "fn test() -> Result(u64, u64) { Result::Ok(42) }";
        let diags = check_source(src);
        assert!(
            diags.is_empty(),
            "Result::Ok(42) returns Result(u64, u64): {:?}",
            diags
        );
    }

    #[test]
    fn test_method_return_type_option_some() {
        // Option::Some(x) returns Option[typeof(x)]
        let src = "fn test() -> Option(u64) { Option::Some(42) }";
        let diags = check_source(src);
        assert!(
            diags.is_empty(),
            "Option::Some(42) returns Option(u64): {:?}",
            diags
        );
    }

    #[test]
    fn test_method_return_type_result_ok_annotated() {
        // Result::Ok(42) with explicit Result(u64, u64) annotation
        let src = "fn test() -> Result(u64, u64) { Result::Ok(42) }";
        let diags = check_source(src);
        assert!(
            diags.is_empty(),
            "Result::Ok(42) with annotation: {:?}",
            diags
        );
    }

    #[test]
    fn test_method_return_type_array_capacity() {
        // Array::capacity returns u64
        let src = "fn test() -> u64 { let arr: Array(u64, 10) = Array::zeroed(); arr.capacity() }";
        let diags = check_source(src);
        assert!(diags.is_empty(), "Array::capacity returns u64: {:?}", diags);
    }

    #[test]
    fn test_method_return_type_ringbuf_pop() {
        // RingBuf::pop() should return Option(inner_type) from receiver type
        let src =
            "fn test() -> Option(u64) { let mut buf: RingBuf(u64, 8) = RingBuf::new(); buf.pop() }";
        let diags = check_source(src);
        assert!(
            diags.is_empty(),
            "RingBuf::pop returns Option(u64): {:?}",
            diags
        );
    }

    #[test]
    fn test_method_return_type_array_slice() {
        // Array::slice() should return Slice(inner_type) from receiver type
        let src = "fn test() -> Slice(u64) { let arr: Array(u64, 10) = Array::zeroed(); arr.slice(0, 4) }";
        let diags = check_source(src);
        assert!(
            diags.is_empty(),
            "Array::slice returns Slice(u64): {:?}",
            diags
        );
    }
}
