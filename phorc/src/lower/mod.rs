// AST to PHIR lowering
// Translates checked AST into PHIR intermediate representation
// Every expression returns a Value. No values are discarded.

use crate::ast::{
    ArraySize, BinOp, Block, Expr, FnDecl, Item, Literal, Pattern, PrimType, SourceFile, Stmt,
    StructDecl, TypeExpr,
};
use crate::check::MethodOwner;
use crate::ir::*;

// ============================================================================
// Struct layout computation — field offsets with alignment
// ============================================================================

/// Layout information for a single struct field
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct FieldLayout {
    name: String,
    offset: u64,
    size: u64,
}

/// Layout information for an entire struct type
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StructLayout {
    name: String,
    fields: Vec<FieldLayout>,
    total_size: u64,
}

/// Compute the size of a type expression (simplified: only primitives and arrays)
fn type_byte_size(ty: &TypeExpr) -> u64 {
    match ty {
        TypeExpr::Prim(p, _) => match p {
            PrimType::U8 | PrimType::I8 => 1,
            PrimType::U16 | PrimType::I16 => 2,
            PrimType::U32 | PrimType::I32 | PrimType::F32 => 4,
            PrimType::U64 | PrimType::I64 | PrimType::F64 | PrimType::Usize | PrimType::Isize => 8,
            PrimType::Bool | PrimType::Char => 1,
            PrimType::Void | PrimType::Never => 0,
        },
        TypeExpr::Array(inner, size, _) => {
            match size {
                ArraySize::Literal(n, _) => type_byte_size(inner) * n,
                ArraySize::ConstIdent(_, _) => type_byte_size(inner) * 8, // default to pointer-sized
            }
        }
        TypeExpr::Named(_) => 8, // pointer-sized default for named types
        _ => 8,
    }
}

/// Convert a checker TypeExpr to a PHIR type for temporary allocation.
/// Method calls now carry their return type from the checker, so the lowerer
/// can allocate correctly-typed temps instead of blanket u64.
fn type_expr_to_phir(ty: &TypeExpr) -> PhirType {
    match ty {
        TypeExpr::Prim(p, _) => PhirType::Prim(p.clone()),
        TypeExpr::Array(inner, size, _) => {
            let inner_ty = type_expr_to_phir(inner);
            let sz = match size {
                ArraySize::Literal(n, _) => *n,
                ArraySize::ConstIdent(_, _) => 0,
            };
            PhirType::Array(Box::new(inner_ty), sz)
        }
        TypeExpr::RingBuf(inner, size, _) => {
            let inner_ty = type_expr_to_phir(inner);
            let sz = match size {
                ArraySize::Literal(n, _) => *n,
                ArraySize::ConstIdent(_, _) => 0,
            };
            PhirType::Array(Box::new(inner_ty), sz)
        }
        TypeExpr::Str(_, _) => PhirType::Prim(PrimType::U64),
        TypeExpr::Slice(inner, _) => type_expr_to_phir(inner),
        TypeExpr::Option(inner, _) => type_expr_to_phir(inner),
        TypeExpr::Result(ok, _, _) => type_expr_to_phir(ok),
        TypeExpr::Cap(inner, _) => PhirType::Cap(Box::new(type_expr_to_phir(inner))),
        TypeExpr::Handle(inner, _) => PhirType::Handle(Box::new(type_expr_to_phir(inner))),
        TypeExpr::Named(_) => PhirType::Ptr(Box::new(PhirType::Prim(PrimType::U8))),
        TypeExpr::Tuple(types, _) => {
            if types.is_empty() {
                PhirType::Prim(PrimType::Void)
            } else {
                // Flatten: use the first element's type as a placeholder
                type_expr_to_phir(&types[0])
            }
        }
        _ => PhirType::Prim(PrimType::U64),
    }
}

/// Compute the alignment of a type expression
fn type_align(ty: &TypeExpr) -> u64 {
    match ty {
        TypeExpr::Prim(p, _) => match p {
            PrimType::U8 | PrimType::I8 | PrimType::Bool | PrimType::Char => 1,
            PrimType::U16 | PrimType::I16 => 2,
            PrimType::U32 | PrimType::I32 | PrimType::F32 => 4,
            PrimType::U64 | PrimType::I64 | PrimType::F64 | PrimType::Usize | PrimType::Isize => 8,
            PrimType::Void | PrimType::Never => 1,
        },
        TypeExpr::Array(inner, _, _) => type_align(inner),
        TypeExpr::Named(_) => 8,
        _ => 8,
    }
}

/// Compute the layout of a struct from its AST declaration
fn compute_struct_layout(decl: &StructDecl) -> StructLayout {
    let mut offset = 0u64;
    let mut fields = Vec::new();
    let mut max_align = 1u64;

    for field in &decl.fields {
        let size = type_byte_size(&field.type_);
        let align = type_align(&field.type_);
        // Align offset to field alignment
        offset = (offset + align - 1) / align * align;
        fields.push(FieldLayout {
            name: field.name.name.clone(),
            offset,
            size,
        });
        offset += size;
        max_align = max_align.max(align);
    }

    // Final struct alignment: round up to max alignment
    let total_size = (offset + max_align - 1) / max_align * max_align;
    StructLayout {
        name: decl.name.name.clone(),
        fields,
        total_size,
    }
}

/// Scan source items for struct declarations and compute layouts
fn build_struct_registry(source: &SourceFile) -> std::collections::HashMap<String, StructLayout> {
    let mut registry = std::collections::HashMap::new();
    for item in &source.items {
        if let Item::Struct(decl) = item {
            let layout = compute_struct_layout(decl);
            registry.insert(decl.name.name.clone(), layout);
        }
    }
    registry
}

// ============================================================================
// Lowering context — tracks temporaries, locals, and struct bindings
// ============================================================================

/// Describes a struct variable binding: which local holds which field
#[derive(Debug, Clone)]
struct StructBinding {
    /// (field_name, local_idx) for each field
    field_locals: std::collections::HashMap<String, usize>,
    /// The first local index (used as the struct's base address)
    base_local: usize,
}

struct LowerCtx<'a> {
    block: &'a mut BasicBlock,
    next_temp: usize,
    next_local: usize,
    /// Maps parameter name to (index, type)
    params: std::collections::HashMap<String, (usize, PhirType)>,
    /// Maps local variable name to (local_index, type)
    locals: std::collections::HashMap<String, (usize, PhirType)>,
    /// Maps struct variable name to its field→local mapping
    struct_vars: std::collections::HashMap<String, StructBinding>,
    /// Struct layout registry (type_name → layout)
    struct_layouts: std::collections::HashMap<String, StructLayout>,
    /// The source items, for looking up struct declarations
    #[allow(dead_code)]
    source_items: &'a [Item],
    /// Resolved method owners from checker (line, col, owner_name, receiver_type)
    method_owners: &'a [MethodOwner],
}

impl<'a> LowerCtx<'a> {
    fn new(
        block: &'a mut BasicBlock,
        struct_layouts: std::collections::HashMap<String, StructLayout>,
        source_items: &'a [Item],
        method_owners: &'a [MethodOwner],
    ) -> Self {
        Self {
            block,
            next_temp: 0,
            next_local: 0,
            params: std::collections::HashMap::new(),
            locals: std::collections::HashMap::new(),
            struct_vars: std::collections::HashMap::new(),
            struct_layouts,
            source_items,
            method_owners,
        }
    }

    /// Look up a struct declaration by name from the source items
    #[allow(dead_code)]
    fn find_struct(&self, name: &str) -> Option<&'a StructDecl> {
        for item in self.source_items {
            if let Item::Struct(decl) = item {
                if decl.name.name == name {
                    return Some(decl);
                }
            }
        }
        None
    }

    /// Get struct layout by type name
    fn struct_layout(&self, name: &str) -> Option<&StructLayout> {
        self.struct_layouts.get(name)
    }

    /// Register a struct variable binding: maps field names to allocated locals
    fn register_struct_var(&mut self, var_name: &str, layout: &StructLayout) -> StructBinding {
        let base_local = self.next_local;
        let mut field_locals = std::collections::HashMap::new();

        // Allocate one local per 8 bytes of struct data (or per field if smaller)
        for fl in &layout.fields {
            let slots_needed = if fl.size == 0 {
                1
            } else {
                ((fl.size + 7) / 8) as usize
            };
            // Use the first slot's index for this field
            let first_slot = self.next_local;
            for _ in 0..slots_needed {
                self.alloc_local(PhirType::Prim(PrimType::U64));
            }
            field_locals.insert(fl.name.clone(), first_slot);
        }

        let binding = StructBinding {
            field_locals,
            base_local,
        };
        self.struct_vars
            .insert(var_name.to_string(), binding.clone());
        binding
    }

    /// Resolve an identifier: returns Arg if it's a parameter, None otherwise
    fn resolve_ident(&self, name: &str) -> Option<Value> {
        self.params
            .get(name)
            .map(|(i, ty)| Value::Arg(*i, ty.clone()))
    }

    /// Allocate a fresh temporary value
    fn fresh_temp(&mut self, ty: PhirType) -> Value {
        let id = self.next_temp;
        self.next_temp += 1;
        Value::Temp(id, ty)
    }

    /// Allocate a new local variable slot
    fn alloc_local(&mut self, _ty: PhirType) -> usize {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    /// Emit an operation into the current block
    fn emit(&mut self, op: Op) {
        self.block.ops.push(op);
    }
}

// ============================================================================
// Public entry point
// ============================================================================

/// Lower a checked source file to PHIR
/// `method_owners` maps (line, col) → owner_type_name from the checker.
pub fn lower(source: &SourceFile, source_name: &str, method_owners: &[MethodOwner]) -> PhirModule {
    let mut module = PhirModule {
        functions: Vec::new(),
        globals: Vec::new(),
        source_name: source_name.to_string(),
    };

    // Pre-scan struct declarations and compute layouts
    let struct_layouts = build_struct_registry(source);

    for item in &source.items {
        match item {
            Item::Fn(fdecl) => {
                let func =
                    lower_function(fdecl, struct_layouts.clone(), &source.items, method_owners);
                module.functions.push(func);
            }
            Item::Impl(impl_block) => {
                // Lower each method with owner-qualified name
                for method in &impl_block.methods {
                    let mut func = lower_function(
                        method,
                        struct_layouts.clone(),
                        &source.items,
                        method_owners,
                    );
                    // Prefix the function name with the owner type for mangling
                    func.name = format!("{}_{}", impl_block.owner.name, func.name);
                    module.functions.push(func);
                }
            }
            Item::Struct(_) | Item::Enum(_) | Item::TypeAlias(_) => {
                // Types are resolved at compile time; no runtime code needed
            }
            Item::Package(_, _) => {}
            _ => {}
        }
    }

    module
}

// ============================================================================
// Function lowering
// ============================================================================

fn lower_function(
    fdecl: &FnDecl,
    struct_layouts: std::collections::HashMap<String, StructLayout>,
    source_items: &[Item],
    method_owners: &[MethodOwner],
) -> PhirFunction {
    let return_type = match &fdecl.return_type {
        Some(ty) => lower_type(ty),
        None => PhirType::Void,
    };

    let mut params = Vec::new();
    for (ident, ty) in &fdecl.params {
        params.push((ident.name.clone(), lower_type(ty)));
    }

    let mut entry_block = BasicBlock {
        id: 0,
        ops: Vec::new(),
        predecessors: Vec::new(),
    };

    // Lower body statements with a fresh lowering context
    // Seed the context with parameter names for resolution
    let local_count: usize;
    {
        let mut ctx = LowerCtx::new(
            &mut entry_block,
            struct_layouts,
            source_items,
            method_owners,
        );
        for (i, (ident, ty)) in fdecl.params.iter().enumerate() {
            ctx.params.insert(ident.name.clone(), (i, lower_type(ty)));
        }
        lower_block(&fdecl.body, &mut ctx);
        local_count = ctx.next_local;
    }

    // Add implicit return if last op isn't a return
    if !entry_block
        .ops
        .last()
        .map_or(false, |op| matches!(op, Op::Return(_)))
    {
        entry_block.ops.push(Op::Return(None));
    }

    PhirFunction {
        name: fdecl.name.name.clone(),
        params,
        return_type,
        effects: fdecl.effects.clone(),
        blocks: vec![entry_block],
        entry_block: 0,
        local_count,
    }
}

// ============================================================================
// Block lowering
// ============================================================================

fn lower_block(block: &Block, ctx: &mut LowerCtx) {
    for stmt in &block.stmts {
        lower_stmt(stmt, ctx);
    }
}

// ============================================================================
// Statement lowering
// ============================================================================

fn lower_stmt(stmt: &Stmt, ctx: &mut LowerCtx) {
    match stmt {
        Stmt::Let { name, init, .. } => {
            // Check if the initializer is a struct literal
            if let Some(Expr::StructLit {
                type_name, fields, ..
            }) = &init
            {
                if let Some(layout) = ctx.struct_layout(&type_name.name).cloned() {
                    // Register struct variable with field→local mapping
                    let binding = ctx.register_struct_var(&name.name, &layout);
                    // Also register the first local as the variable's "base" for Ident resolution
                    let base_ty = PhirType::Prim(PrimType::U64);
                    ctx.locals
                        .insert(name.name.clone(), (binding.base_local, base_ty.clone()));

                    // Store each field to its allocated local
                    for (field_ident, field_expr) in fields {
                        let field_val = lower_expr(field_expr, ctx);
                        if let Some(&field_local) = binding.field_locals.get(&field_ident.name) {
                            ctx.emit(Op::Store {
                                addr: Value::Local(field_local, PhirType::Prim(PrimType::U64)),
                                val: field_val,
                            });
                        }
                    }
                    return;
                }
            }

            let local_ty = PhirType::Prim(PrimType::U64); // default type; checker would refine
            let local_idx = ctx.alloc_local(local_ty.clone());
            ctx.locals
                .insert(name.name.clone(), (local_idx, local_ty.clone()));
            if let Some(init_expr) = init {
                let val = lower_expr(init_expr, ctx);
                ctx.emit(Op::Store {
                    addr: Value::Local(local_idx, local_ty),
                    val,
                });
            }
        }
        Stmt::Expr(expr, _) => {
            // Expression statement — lower for side effects, discard value
            let _val = lower_expr(expr, ctx);
        }
        Stmt::Return(ret_expr, _) => {
            if let Some(expr) = ret_expr {
                let val = lower_expr(expr, ctx);
                ctx.emit(Op::Return(Some(val)));
            } else {
                ctx.emit(Op::Return(None));
            }
        }
        Stmt::Assignment { target, value, .. } => {
            let val = lower_expr(value, ctx);
            // Resolve assignment target
            match target.as_ref() {
                // Handle struct field assignment: p.x = expr
                Expr::FieldAccess { obj, field, .. } => {
                    if let Expr::Ident(id) = obj.as_ref() {
                        if let Some(binding) = ctx.struct_vars.get(&id.name) {
                            if let Some(&field_local) = binding.field_locals.get(&field.name) {
                                ctx.emit(Op::Store {
                                    addr: Value::Local(field_local, PhirType::Prim(PrimType::U64)),
                                    val,
                                });
                                return;
                            }
                        }
                    }
                    // Fallback: store to target local
                    ctx.emit(Op::Store {
                        addr: Value::Global("_".to_string(), PhirType::Prim(PrimType::U64)),
                        val,
                    });
                }
                Expr::Ident(id) => {
                    if let Some((local_idx, ty)) = ctx.locals.get(&id.name).cloned() {
                        ctx.emit(Op::Store {
                            addr: Value::Local(local_idx, ty),
                            val,
                        });
                    } else {
                        // Fallback: global store
                        ctx.emit(Op::Store {
                            addr: Value::Global(id.name.clone(), PhirType::Prim(PrimType::U64)),
                            val,
                        });
                    }
                }
                _ => {
                    ctx.emit(Op::Store {
                        addr: Value::Global("_".to_string(), PhirType::Prim(PrimType::U64)),
                        val,
                    });
                }
            }
        }
    }
}

// ============================================================================
// Expression lowering — returns Value
// ============================================================================

fn lower_expr(expr: &Expr, ctx: &mut LowerCtx) -> Value {
    match expr {
        Expr::Literal(lit) => lower_literal(lit),

        Expr::Ident(id) => {
            // Resolve against function parameters first
            if let Some(arg_val) = ctx.resolve_ident(&id.name) {
                return arg_val;
            }
            // Resolve against struct variable bindings (return base address)
            if let Some(binding) = ctx.struct_vars.get(&id.name) {
                // Return the struct's base local as the value (its address on stack)
                return Value::Local(binding.base_local, PhirType::Prim(PrimType::U64));
            }
            // Resolve against local variables
            if let Some((local_idx, ty)) = ctx.locals.get(&id.name).cloned() {
                let out = ctx.fresh_temp(ty.clone());
                ctx.emit(Op::Load {
                    addr: Value::Local(local_idx, ty),
                    ty: PhirType::Prim(PrimType::U64),
                    out: out.clone(),
                });
                return out;
            }
            // Otherwise, treat as global variable — load from its storage location
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            ctx.emit(Op::Load {
                addr: Value::Global(id.name.clone(), PhirType::Prim(PrimType::U64)),
                ty: PhirType::Prim(PrimType::U64),
                out: out.clone(),
            });
            out
        }

        Expr::Binary { op, lhs, rhs, .. } => {
            let lhs_val = lower_expr(lhs, ctx);
            let rhs_val = lower_expr(rhs, ctx);
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            ctx.emit(Op::BinOp {
                op: lower_binop(*op),
                lhs: lhs_val,
                rhs: rhs_val,
                ty: PhirType::Prim(PrimType::U64),
                out: out.clone(),
            });
            out
        }

        Expr::Unary {
            op: _, expr: inner, ..
        } => {
            let inner_val = lower_expr(inner, ctx);
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            ctx.emit(Op::Cast {
                val: inner_val,
                to: PhirType::Prim(PrimType::U64),
                out: out.clone(),
            });
            out
        }

        Expr::Call { callee, args, .. } => {
            let mut arg_vals = Vec::new();
            for arg in args {
                let v = lower_expr(arg, ctx);
                arg_vals.push(v);
            }
            let callee_name = match callee.as_ref() {
                Expr::Ident(id) => id.name.clone(),
                Expr::TypeIdent(id) => id.name.clone(),
                _ => "unknown_call".to_string(),
            };
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            ctx.emit(Op::Call {
                callee: callee_name,
                args: arg_vals,
                effects: vec![],
                out: out.clone(),
            });
            out
        }

        Expr::Block(block, _) => {
            // Lower block body — return last expression's value if any
            let mut last_val = Value::Const(Constant::Int(0, 64));
            for stmt in &block.stmts {
                match stmt {
                    Stmt::Expr(e, _) => {
                        last_val = lower_expr(e, ctx);
                    }
                    Stmt::Return(r, _) => {
                        if let Some(e) = r {
                            last_val = lower_expr(e, ctx);
                        }
                        ctx.emit(Op::Return(Some(last_val.clone())));
                        return last_val;
                    }
                    other => {
                        lower_stmt(other, ctx);
                        last_val = Value::Const(Constant::Int(0, 64));
                    }
                }
            }
            last_val
        }

        Expr::If {
            cond, then, else_, ..
        } => {
            let _cond_val = lower_expr(cond, ctx);
            let then_val = lower_block_expr(then, ctx);
            let _else_val = if let Some(else_expr) = else_ {
                lower_expr(else_expr, ctx)
            } else {
                Value::Const(Constant::Int(0, 64))
            };
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            ctx.emit(Op::Cast {
                val: then_val,
                to: PhirType::Prim(PrimType::U64),
                out: out.clone(),
            });
            out
        }

        Expr::Match {
            expr: inner, arms, ..
        } => {
            let match_val = lower_expr(inner, ctx);
            let mut arm_vals = Vec::new();
            for arm in arms {
                // Emit pattern matching code for this arm
                match &arm.pattern {
                    Pattern::Wild(_) => {
                        // Wildcard always matches — no comparison needed
                    }
                    Pattern::Lit(lit) => {
                        let pat_val = lower_literal(lit);
                        let _cmp = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
                        ctx.emit(Op::BinOp {
                            op: BinOpKind::Eq,
                            lhs: match_val.clone(),
                            rhs: pat_val,
                            ty: PhirType::Prim(PrimType::U64),
                            out: _cmp,
                        });
                    }
                    Pattern::EnumVariant(_, _variant, _subpatterns, _) => {
                        // Enum variant matching — compare with variant discriminant
                        // Placeholder: emit Nop to avoid panic, real matching to be added
                        ctx.emit(Op::Nop);
                    }
                    Pattern::Ident(_) => {
                        // Ident pattern matches anything and binds the value
                        // Binding not yet fully implemented — just a Nop placeholder
                        ctx.emit(Op::Nop);
                    }
                    Pattern::Tuple(_, _) => {
                        // Tuple pattern — placeholder to avoid panic
                        ctx.emit(Op::Nop);
                    }
                }

                // Emit guard check if present
                if let Some(guard) = &arm.guard {
                    let _guard_val = lower_expr(guard, ctx);
                }

                let v = lower_expr(&arm.body, ctx);
                arm_vals.push(v);
            }
            let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
            if let Some(first) = arm_vals.first() {
                ctx.emit(Op::Cast {
                    val: first.clone(),
                    to: PhirType::Prim(PrimType::U64),
                    out: out.clone(),
                });
            }
            out
        }

        Expr::Loop { body, .. } => {
            lower_block(body, ctx);
            Value::Const(Constant::Int(0, 64))
        }

        Expr::ForLoop {
            var: _,
            range,
            body,
            ..
        } => {
            let _range_val = lower_expr(range, ctx);
            lower_block(body, ctx);
            Value::Const(Constant::Int(0, 64))
        }

        Expr::WhileLoop { cond, body, .. } => {
            let _cond_val = lower_expr(cond, ctx);
            lower_block(body, ctx);
            Value::Const(Constant::Int(0, 64))
        }

        Expr::Return(ret_expr, _) => {
            let val = if let Some(expr) = ret_expr {
                lower_expr(expr, ctx)
            } else {
                Value::Const(Constant::Int(0, 64))
            };
            ctx.emit(Op::Return(Some(val.clone())));
            val
        }

        Expr::Break(_) | Expr::Continue(_) => {
            // Placeholder: prevents dead code emission after break/continue.
            // Emits Return(None) as a single-block terminator.
            // Real loop-target lowering (multi-block PHIR with branch ops)
            // is not yet implemented — this is a bootstrap placeholder.
            ctx.emit(Op::Return(None));
            Value::Const(Constant::Int(0, 64))
        }

        Expr::Yield(e, _) => {
            let v = lower_expr(e, ctx);
            v
        }

        Expr::Spawn(e, _) => {
            let v = lower_expr(e, ctx);
            v
        }

        Expr::Trusted {
            reason: _, body, ..
        } => {
            lower_block(body, ctx);
            Value::Const(Constant::Int(0, 64))
        }

        Expr::ResidualEmit { op: _, fields, .. } => {
            let mut field_vals = Vec::new();
            for (name, fexpr) in fields {
                let v = lower_expr(fexpr, ctx);
                field_vals.push((name.clone(), v));
            }
            ctx.emit(Op::ResidualEmit {
                operation: "emit".to_string(),
                fields: field_vals,
            });
            Value::Const(Constant::Int(0, 64))
        }

        Expr::HandleCast(inner, _) => lower_expr(inner, ctx),

        Expr::Cast { expr: inner, .. } => lower_expr(inner, ctx),

        Expr::CapMove(inner, _) => lower_expr(inner, ctx),

        Expr::StructLit {
            type_name, fields, ..
        } => {
            // Standalone struct literal (not via let binding)
            // Look up layout, clone it to avoid borrow issues
            let layout_opt = ctx.struct_layout(&type_name.name).cloned();
            if let Some(layout) = layout_opt {
                let base_local = ctx.next_local;
                // Allocate one local per field
                for fl in &layout.fields {
                    let slots_needed = if fl.size == 0 {
                        1
                    } else {
                        ((fl.size + 7) / 8) as usize
                    };
                    for _ in 0..slots_needed {
                        ctx.alloc_local(PhirType::Prim(PrimType::U64));
                    }
                }

                // Store each field value to its allocated local
                for (field_ident, field_expr) in fields {
                    // Find the local index for this field by counting preceding fields
                    let mut local_idx = base_local;
                    for existing_fl in &layout.fields {
                        if existing_fl.name == field_ident.name {
                            break;
                        }
                        let slots = if existing_fl.size == 0 {
                            1
                        } else {
                            ((existing_fl.size + 7) / 8) as usize
                        };
                        local_idx += slots;
                    }
                    let field_val = lower_expr(field_expr, ctx);
                    ctx.emit(Op::Store {
                        addr: Value::Local(local_idx, PhirType::Prim(PrimType::U64)),
                        val: field_val,
                    });
                }

                // Return the base local as the struct value (address)
                Value::Local(base_local, PhirType::Prim(PrimType::U64))
            } else {
                let mut last_val = Value::Const(Constant::Int(0, 64));
                for (_, field_expr) in fields {
                    last_val = lower_expr(field_expr, ctx);
                }
                last_val
            }
        }

        Expr::FieldAccess { obj, field, .. } => {
            // Check if the object is a struct variable
            if let Expr::Ident(id) = obj.as_ref() {
                if let Some(binding) = ctx.struct_vars.get(&id.name) {
                    if let Some(&field_local) = binding.field_locals.get(&field.name) {
                        let out = ctx.fresh_temp(PhirType::Prim(PrimType::U64));
                        ctx.emit(Op::Load {
                            addr: Value::Local(field_local, PhirType::Prim(PrimType::U64)),
                            ty: PhirType::Prim(PrimType::U64),
                            out: out.clone(),
                        });
                        return out;
                    }
                }
                // Check if it's a regular local holding a struct (lowered inline)
                if let Some((_local_idx, _ty)) = ctx.locals.get(&id.name).cloned() {
                    // Try to find the struct layout for this variable
                    // For now, fall through to the simple forwarding
                }
            }
            // Fallback: forward the object expression (existing behavior)
            lower_expr(obj, ctx)
        }

        Expr::MethodCall {
            obj,
            method,
            args,
            resolved_owner,
            ..
        } => {
            let obj_val = lower_expr(obj, ctx);
            let mut arg_vals = vec![obj_val];
            for arg in args {
                arg_vals.push(lower_expr(arg, ctx));
            }
            // Use resolved_owner from checker if available (most accurate),
            // then check the method_owners table from checker,
            // then fall back to guessing from the object expression.
            // Also use the checker's return_ty to allocate a correctly-typed PHIR temp.
            let method_entry = ctx
                .method_owners
                .iter()
                .find(|m| m.line == method.loc.line && m.col == method.loc.col);

            let callee_name = if let Some(owner) = resolved_owner {
                format!("{}_{}", owner, method.name)
            } else if let Some(m) = method_entry {
                format!("{}_{}", m.owner, method.name)
            } else {
                match obj.as_ref() {
                    Expr::TypeIdent(id) => format!("{}_{}", id.name, method.name),
                    Expr::Ident(id) => {
                        if id.name.chars().next().map_or(false, |c| c.is_uppercase()) {
                            format!("{}_{}", id.name, method.name)
                        } else {
                            format!("method_{}", method.name)
                        }
                    }
                    _ => format!("method_{}", method.name),
                }
            };

            // Use the checker's computed return type to allocate the output temp,
            // falling back to u64 if no type info is available.
            let out_ty = method_entry
                .and_then(|m| m.return_ty.as_ref())
                .map(|ty| type_expr_to_phir(ty))
                .unwrap_or(PhirType::Prim(PrimType::U64));
            let out = ctx.fresh_temp(out_ty);
            ctx.emit(Op::Call {
                callee: callee_name,
                args: arg_vals,
                effects: vec![],
                out: out.clone(),
            });
            out
        }

        Expr::Index { obj, index, .. } => {
            let _obj_val = lower_expr(obj, ctx);
            lower_expr(index, ctx)
        }

        Expr::PostfixBang { expr: inner, .. } => lower_expr(inner, ctx),

        Expr::ArrayLit(elements, _) => {
            // Array literal: [a, b, c, ...]
            // Allocate one local slot per element, store each element,
            // return the base local as the array address.
            if elements.is_empty() {
                Value::Const(Constant::Int(0, 64))
            } else {
                let base_local = ctx.next_local;
                // Allocate one slot per element (each 8 bytes)
                for _ in elements {
                    ctx.alloc_local(PhirType::Prim(PrimType::U64));
                }
                // Store each element to its slot
                for (i, elem) in elements.iter().enumerate() {
                    let val = lower_expr(elem, ctx);
                    let slot = base_local + i;
                    ctx.emit(Op::Store {
                        addr: Value::Local(slot, PhirType::Prim(PrimType::U64)),
                        val,
                    });
                }
                // Return base address
                Value::Local(base_local, PhirType::Prim(PrimType::U64))
            }
        }

        Expr::TypeIdent(_) => Value::Const(Constant::Int(0, 64)),

        Expr::Error(_) => Value::Const(Constant::Int(0, 64)),
    }
}

/// Lower a block as an expression, returning the last value
fn lower_block_expr(block: &Block, ctx: &mut LowerCtx) -> Value {
    let mut last_val = Value::Const(Constant::Int(0, 64));
    for stmt in &block.stmts {
        match stmt {
            Stmt::Expr(e, _) => {
                last_val = lower_expr(e, ctx);
                // If the expression is a Break, Continue, or Return, stop processing
                // to avoid emitting dead code after a terminator.
                if matches!(e, Expr::Break(_) | Expr::Continue(_) | Expr::Return(_, _)) {
                    break;
                }
            }
            Stmt::Return(ret_expr, _) => {
                if let Some(expr) = ret_expr {
                    last_val = lower_expr(expr, ctx);
                    ctx.emit(Op::Return(Some(last_val.clone())));
                } else {
                    ctx.emit(Op::Return(None));
                }
                // Stop processing after return — rest is dead code
                break;
            }
            other => {
                lower_stmt(other, ctx);
                last_val = Value::Const(Constant::Int(0, 64));
            }
        }
    }
    last_val
}

/// Lower a literal to a PHIR constant
fn lower_literal(lit: &Literal) -> Value {
    match lit {
        Literal::Int(v, _) => Value::Const(Constant::Int(*v, 64)),
        Literal::Float(v, _) => Value::Const(Constant::Float(v.to_bits(), 64)),
        Literal::Bool(v, _) => Value::Const(Constant::Bool(*v)),
        Literal::Char(c, _) => Value::Const(Constant::Int(*c as u64, 32)),
        Literal::Str(s, _) => Value::Const(Constant::String(s.as_bytes().to_vec())),
    }
}

/// Lower AST binary operator to PHIR
fn lower_binop(op: BinOp) -> BinOpKind {
    match op {
        BinOp::Add => BinOpKind::Add,
        BinOp::Sub => BinOpKind::Sub,
        BinOp::Mul => BinOpKind::Mul,
        BinOp::Div => BinOpKind::Div,
        BinOp::Rem => BinOpKind::Rem,
        BinOp::And => BinOpKind::And,
        BinOp::Or => BinOpKind::Or,
        BinOp::Xor => BinOpKind::Xor,
        BinOp::Shl => BinOpKind::Shl,
        BinOp::Shr => BinOpKind::Shr,
        BinOp::Eq => BinOpKind::Eq,
        BinOp::Ne => BinOpKind::Ne,
        BinOp::Lt => BinOpKind::Lt,
        BinOp::Le => BinOpKind::Le,
        BinOp::Gt => BinOpKind::Gt,
        BinOp::Ge => BinOpKind::Ge,
        _ => BinOpKind::Add,
    }
}

/// Lower type expression to PHIR type
fn lower_type(ty: &TypeExpr) -> PhirType {
    match ty {
        TypeExpr::Prim(p, _) => PhirType::Prim(p.clone()),
        TypeExpr::Named(_) => PhirType::Ptr(Box::new(PhirType::Prim(PrimType::U8))),
        TypeExpr::Array(inner, size, _) => {
            let n = match size {
                ArraySize::Literal(n, _) => *n,
                ArraySize::ConstIdent(_, _) => 8, // default to pointer-sized for const-ident arrays
            };
            PhirType::Array(Box::new(lower_type(inner)), n)
        }
        _ => PhirType::Prim(PrimType::U64),
    }
}
