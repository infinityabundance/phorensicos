// Regression tests for the lowering/codegen defects that blocked sealed-object
// execution. These assert on the pipeline output (PHIR) rather than on emitted
// bytes, so they pin the semantic fixes without depending on a disassembler.

use phorc::check::check_module;
use phorc::lex::Lexer;
use phorc::lower::lower;
use phorc::parse::Parser;

fn lower_src(src: &str) -> phorc::ir::PhirModule {
    let mut lexer = Lexer::new(src);
    let ast = {
        let mut parser = Parser::new(&mut lexer);
        parser.parse_source().expect("parse")
    };
    assert!(
        lexer.diagnostics().is_empty(),
        "lexer diagnostics: {:?}",
        lexer.diagnostics()
    );
    let (diags, owners) = check_module(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    lower(&ast, "test.phor", &owners)
}

/// `<<` must lower to a shift, not a comparison. Before the lexer/parser fix,
/// `a << b` silently became the `Lt` comparison, so the memcmp candidate's mask
/// computation compared instead of shifting.
#[test]
fn test_shift_lowers_to_shift_not_comparison() {
    let module =
        lower_src("fn f(x: u64) -> u64 effect [compute] { let y: u64 = x << 3; return y; }");
    let f = &module.functions[0];
    let has_shl = f.blocks[0].ops.iter().any(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                op: phorc::ir::BinOpKind::Shl,
                ..
            }
        )
    });
    assert!(has_shl, "expected a Shl op, got {:?}", f.blocks[0].ops);
}

/// Module-level `const` identifiers must resolve to their literal value. Before
/// the fix they lowered to a zero-valued `Global` load, so `b >= FIRST_LOWER`
/// compared against 0.
#[test]
fn test_named_constants_resolve_to_literals() {
    let module = lower_src(
        "const FIRST: u64 = 97;\n\
         const DELTA: u64 = 32;\n\
         fn f(b: u64) -> u64 effect [compute] { return b - DELTA; }",
    );
    let f = &module.functions[0];
    let uses_literal = f.blocks[0].ops.iter().any(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                rhs: phorc::ir::Value::Const(phorc::ir::Constant::Int(32, _)),
                ..
            }
        )
    });
    assert!(
        uses_literal,
        "const DELTA should have lowered to the literal 32, got {:?}",
        f.blocks[0].ops
    );
    // No op may still reference the const as a global load.
    let global_leak = f.blocks[0]
        .ops
        .iter()
        .any(|op| format!("{:?}", op).contains("Global(\"DELTA\""));
    assert!(
        !global_leak,
        "const leaked as a Global: {:?}",
        f.blocks[0].ops
    );
}

/// Boolean conjunction must lower to `And`, not to `Add` (the old catch-all).
#[test]
fn test_logical_and_lowers_to_bitand() {
    let module = lower_src(
        "fn f(a: u64, b: u64) -> bool effect [compute] { \
           let ok: bool = (a < 10) && (b < 10); return ok; }",
    );
    let f = &module.functions[0];
    let ops = &f.blocks[0].ops;
    let ands = ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                phorc::ir::Op::BinOp {
                    op: phorc::ir::BinOpKind::And,
                    ..
                }
            )
        })
        .count();
    assert!(ands >= 1, "expected an And op, got {:?}", ops);
    // The conjunction result must not have been added to anything.
    let added_bool = ops.iter().any(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                op: phorc::ir::BinOpKind::Add,
                ..
            }
        )
    });
    assert!(!added_bool, "`&&` must not lower to Add: {:?}", ops);
}

/// Integer `const` expressions must fold. `const NEG: u64 = 0 - 1;` (a
/// branchless -1) previously stayed unresolved and materialized as 0.
#[test]
fn test_const_arithmetic_folds() {
    let module = lower_src(
        "const NEG: u64 = 0 - 1;\n\
         fn f(x: u64) -> u64 effect [compute] { return x - NEG; }",
    );
    let f = &module.functions[0];
    let ops = &f.blocks[0].ops;
    let folded = ops.iter().any(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                rhs: phorc::ir::Value::Const(phorc::ir::Constant::Int(u64::MAX, _)),
                ..
            }
        )
    });
    assert!(folded, "`0 - 1` should fold to u64::MAX, got {:?}", ops);
    let leaked = ops
        .iter()
        .any(|op| format!("{:?}", op).contains("Global(\"NEG\""));
    assert!(!leaked, "const leaked as a Global: {:?}", ops);
}

/// Regression: `x = expr` reaches the lowerer as `Binary { op: Assign }`, and
/// the `lower_binop` catch-all used to turn `Assign` into `Add` — so every
/// reassignment silently added instead of storing (which is why the memchr
/// candidate returned mirrored indices).
#[test]
fn test_assignment_stores_instead_of_adding() {
    let module =
        lower_src("fn f() -> u64 effect [compute] { let mut x: u64 = 0; x = 7; return x; }");
    let f = &module.functions[0];
    let ops = &f.blocks[0].ops;

    let adds = ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                phorc::ir::Op::BinOp {
                    op: phorc::ir::BinOpKind::Add,
                    ..
                }
            )
        })
        .count();
    assert_eq!(adds, 0, "assignment must not lower to Add: {:?}", ops);

    let stored_seven = ops.iter().any(|op| {
        matches!(
            op,
            phorc::ir::Op::Store {
                val: phorc::ir::Value::Const(phorc::ir::Constant::Int(7, _)),
                ..
            }
        )
    });
    assert!(
        stored_seven,
        "`x = 7` should emit a Store of the literal 7, got {:?}",
        ops
    );
}
#[test]
fn test_shift_binds_looser_than_addition() {
    let module =
        lower_src("fn f(a: u64, b: u64, c: u64) -> u64 effect [compute] { return a + b << c; }");
    let f = &module.functions[0];
    let ops = &f.blocks[0].ops;
    let add_idx = ops.iter().position(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                op: phorc::ir::BinOpKind::Add,
                ..
            }
        )
    });
    let shl_idx = ops.iter().position(|op| {
        matches!(
            op,
            phorc::ir::Op::BinOp {
                op: phorc::ir::BinOpKind::Shl,
                ..
            }
        )
    });
    assert!(add_idx.is_some() && shl_idx.is_some(), "ops: {:?}", ops);
    assert!(
        add_idx.unwrap() < shl_idx.unwrap(),
        "addition must be evaluated before the shift: {:?}",
        ops
    );
}
