// porting/composition_registry.rs — the one composition table (Phase 2)
//
// Before this module, a composition *was* bespoke Rust control flow: seven
// hand-written runners, selected by a chain of `if id == …` branches in
// `composition_runner`. Phase 2 makes a composition **typed data**. This module is
// the single registration table that binds each composition id to:
//
//   * its committed metadata (`CompositionTarget`: id, stages, schemas), so the
//     store and the verifier keep the same qualified identity they always had;
//   * its **`CompositionIR`** — the chain as an acyclic data-flow graph, evaluated
//     by exactly one interpreter (`composition_ir::eval`);
//   * its **`CaseGenerator`** — a deterministic corpus (the one place a
//     composition is still target-specific, and legitimately so: a corpus is data
//     about a contract, not engine logic).
//
// Adding a composition is therefore new *data* plus, at most, a new corpus — and
// no change to the interpreter, the court, the dispatcher, or the store. The
// static audit in `registry.rs` keeps target-id branching out of the generic
// machinery; this module is a declared extension boundary.
//
// The six-leaf vocabulary the seven chains use is deliberately small:
// `Input`, `ConstantScalar`, `Call`, `MapBytes`, `PackUsize`, `Slice`, `Compare`,
// `Binary`, `Select`. Every node is load-bearing for a real chain.

use alloc::string::String;
use alloc::vec::Vec;

use crate::porting::candidate::{decode_index, decode_usize};
use crate::porting::composition::CompositionTarget;
use crate::porting::composition_ir::{BinOp, Value};
use crate::porting::composition_ir::{CompareOp, CompositionIR, Node, Output, Type, ValueId};
use crate::porting::dialect_cage;
use crate::porting::oracle_trace::OracleTrace;
use crate::porting::portspec::{self, ObservableSpec};
use crate::porting::target::{self, TestCase};
use crate::porting::PortError;

// The qualified port ids the chains call. These are the *same* ids the store
// publishes and the dispatcher resolves; a composition is built only from sealed
// ports, leaf or composition.
const T: &str = target::LIBC_TOUPPER.id;
const M: &str = target::LIBC_MEMCHR.id;
const S: &str = target::LIBC_STRLEN.id;
const EACH: &str = "phor:compose:toupper_each:c-locale:u8s:v1";
const TMEM: &str = "phor:compose:toupper_memchr:c-locale:index:v1";

/// How a port's observable is decoded back into a typed IR value. This is the
/// projection boundary: a pointer-shaped result is an index; a byte is still a
/// byte. It applies identically to the foreign backend and the sealed backend, so
/// the two cannot disagree about what a port returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortValue {
    /// Exact output bytes (a byte map's folded buffer).
    Bytes,
    /// An index or an absent sentinel (`memchr`, `strrchr`).
    Index,
    /// A non-negative length (`strlen`, `strspn`).
    Length,
    /// The C ordering sign (`memcmp`).
    Sign,
}

/// Decode a port's raw output bytes into the typed value its observable denotes.
pub fn decode_port_output(kind: PortValue, output: &[u8]) -> Value {
    match kind {
        PortValue::Bytes => Value::Bytes(output.to_vec()),
        PortValue::Index => Value::Scalar(decode_index(output) as i64),
        PortValue::Length => Value::Scalar(decode_usize(output) as i64),
        PortValue::Sign => Value::Scalar(decode_index(output) as i64),
    }
}

/// The observable kind of a port, leaf or composition.
///
/// A leaf's kind comes from its `PortSpec` (the Phase 1 contract); a composition's
/// kind is declared here because a composition is itself a sealed port with its own
/// contract. An unknown port is `None` and the callers fail closed.
pub fn port_value(port_id: &str) -> Option<PortValue> {
    if port_id == EACH {
        return Some(PortValue::Bytes);
    }
    if ALL.iter().any(|c| c.target.id == port_id) {
        // A search composition's output is an index or an absent sentinel.
        return Some(PortValue::Index);
    }
    let spec = portspec::by_target_id(port_id)?;
    Some(match spec.observable {
        ObservableSpec::ExactBytes | ObservableSpec::UnsignedInteger { .. } => PortValue::Bytes,
        ObservableSpec::SignedInteger { .. } | ObservableSpec::Sign => PortValue::Sign,
        ObservableSpec::IndexOrAbsent => PortValue::Index,
        ObservableSpec::Length => PortValue::Length,
    })
}

/// One composition: its committed metadata, its IR, and its corpus.
pub struct CompositionDef {
    pub target: CompositionTarget,
    pub ir: fn() -> CompositionIR,
    pub cases: fn() -> Vec<TestCase>,
    /// The deterministic foreign observation of this chain's corpus. This is the
    /// oracle adapter extension point, shared with the Phase 1 courts.
    pub observe:
        fn(&[TestCase], &crate::porting::PortingAuthority) -> Result<Vec<OracleTrace>, PortError>,
}

/// Every composition the store publishes, in dependency order (an inner
/// composition precedes the chain that consumes it).
pub static ALL: [CompositionDef; 7] = [
    CompositionDef {
        target: crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR,
        ir: ir_toupper_memchr,
        cases: crate::porting::composition::composition_corpus,
        observe: dialect_cage::observe_composition,
    },
    CompositionDef {
        target: crate::porting::composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR,
        ir: ir_toupper_strlen_memchr,
        cases: crate::porting::composition_strlen_memchr::composition_corpus,
        observe: dialect_cage::observe_composition_strlen_memchr,
    },
    CompositionDef {
        target: crate::porting::composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR,
        ir: ir_toupper_strlen_memchr_pair,
        cases: crate::porting::composition_pair::composition_corpus,
        observe: dialect_cage::observe_composition_pair,
    },
    CompositionDef {
        target: crate::porting::composition_toupper_each::COMPOSITION_TOUPPER_EACH,
        ir: ir_toupper_each,
        cases: crate::porting::composition_toupper_each::composition_corpus,
        observe: dialect_cage::observe_composition_toupper_each,
    },
    CompositionDef {
        target: crate::porting::composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR,
        ir: ir_toupper_each_strlen_memchr,
        cases: crate::porting::composition_nested::nested_corpus,
        observe: dialect_cage::observe_composition_nested,
    },
    CompositionDef {
        target: crate::porting::composition_suffix::COMPOSITION_TOUPPER_MEMCHR_SUFFIX,
        ir: ir_toupper_memchr_suffix,
        cases: crate::porting::composition_suffix::composition_corpus,
        observe: dialect_cage::observe_composition_suffix,
    },
    CompositionDef {
        target: crate::porting::composition_slice_search::COMPOSITION_TOUPPER_EACH_SLICE_SEARCH,
        ir: ir_toupper_each_slice_search,
        cases: crate::porting::composition_slice_search::composition_corpus,
        observe: dialect_cage::observe_composition_suffix,
    },
];

/// Resolve a composition by qualified id.
pub fn by_id(id: &str) -> Option<&'static CompositionDef> {
    ALL.iter().find(|c| c.target.id == id)
}

/// Resolve a composition by short name (`toupper_memchr`, …) or qualified id.
pub fn by_name(name: &str) -> Option<&'static CompositionDef> {
    ALL.iter()
        .find(|c| c.target.id == name || short_name(c.target.id) == name)
}

/// The short name of a composition id (`phor:compose:toupper_each:c-locale:u8s:v1`
/// → `toupper_each`).
pub fn short_name(id: &str) -> &str {
    id.strip_prefix("phor:compose:")
        .and_then(|r| r.split(':').next())
        .unwrap_or(id)
}

// ---------------------------------------------------------------------------
// The seven chains, as data
// ---------------------------------------------------------------------------

/// `toupper ∘ memchr`: fold the whole haystack and the needle, then search.
fn ir_toupper_memchr() -> CompositionIR {
    CompositionIR {
        id: String::from(crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR.id),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![Output {
            ty: Type::Scalar,
            value: ValueId(6),
        }],
        nodes: alloc::vec![
            Node::Input { index: 0 }, // 0 haystack
            Node::Input { index: 1 }, // 1 needle (one byte)
            Node::Input { index: 2 }, // 2 n
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(0)
            }, // 3 folded hay
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(1)
            }, // 4 folded needle
            Node::PackUsize { value: ValueId(2) }, // 5 n as bytes
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(3), ValueId(4), ValueId(5)]
            }, // 6 search
        ],
    }
}

/// `toupper ∘ strlen ∘ memchr`: fold the haystack, derive the bound with `strlen`,
/// fold the needle, then search within the **derived** bound.
fn ir_toupper_strlen_memchr() -> CompositionIR {
    CompositionIR {
        id: String::from(
            crate::porting::composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR.id,
        ),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![Output {
            ty: Type::Scalar,
            value: ValueId(8),
        }],
        nodes: alloc::vec![
            Node::Input { index: 0 }, // 0 haystack
            Node::Input { index: 1 }, // 1 needle
            Node::Input { index: 2 }, // 2 n
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(0)
            }, // 3 folded hay
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(1)
            }, // 4 folded needle
            Node::PackUsize { value: ValueId(2) }, // 5 n as bytes
            Node::Call {
                port: String::from(S),
                args: alloc::vec![ValueId(3), ValueId(5)]
            }, // 6 derived bound
            Node::PackUsize { value: ValueId(6) }, // 7 bound as bytes
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(3), ValueId(4), ValueId(7)]
            }, // 8 search
        ],
    }
}

/// `toupper ∘ strlen ∘ memchr ∘ toupper ∘ memchr`: one derived bound consumed by
/// **two** searches, the second non-adjacent to the stage that produced it.
fn ir_toupper_strlen_memchr_pair() -> CompositionIR {
    CompositionIR {
        id: String::from(
            crate::porting::composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id,
        ),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![
            Output {
                ty: Type::Scalar,
                value: ValueId(10),
            },
            Output {
                ty: Type::Scalar,
                value: ValueId(11),
            },
        ],
        nodes: alloc::vec![
            Node::Input { index: 0 }, // 0 haystack
            Node::Input { index: 1 }, // 1 needle A
            Node::Input { index: 2 }, // 2 needle B
            Node::Input { index: 3 }, // 3 n
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(0)
            }, // 4 folded hay
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(1)
            }, // 5 folded A
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(2)
            }, // 6 folded B
            Node::PackUsize { value: ValueId(3) }, // 7 n as bytes
            Node::Call {
                port: String::from(S),
                args: alloc::vec![ValueId(4), ValueId(7)]
            }, // 8 shared derived bound
            Node::PackUsize { value: ValueId(8) }, // 9 bound as bytes
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(4), ValueId(5), ValueId(9)]
            }, // 10 search A
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(4), ValueId(6), ValueId(9)]
            }, // 11 search B (same bound)
        ],
    }
}

/// `toupper_each`: fold the first `n` bytes, returning a buffer.
fn ir_toupper_each() -> CompositionIR {
    CompositionIR {
        id: String::from(crate::porting::composition_toupper_each::COMPOSITION_TOUPPER_EACH.id),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Scalar],
        outputs: alloc::vec![Output {
            ty: Type::Bytes,
            value: ValueId(4),
        }],
        nodes: alloc::vec![
            Node::Input { index: 0 },          // 0 bytes
            Node::Input { index: 1 },          // 1 n
            Node::ConstantScalar { value: 0 }, // 2 origin
            Node::Slice {
                input: ValueId(0),
                origin: ValueId(2),
                length: Some(ValueId(1))
            }, // 3 first n bytes
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(3)
            }, // 4 folded
        ],
    }
}

/// `toupper_each ∘ strlen ∘ memchr`: the fold stage is itself the sealed
/// **composition** `toupper_each`, resolved from the store and dispatched
/// recursively.
fn ir_toupper_each_strlen_memchr() -> CompositionIR {
    CompositionIR {
        id: String::from(
            crate::porting::composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id,
        ),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![Output {
            ty: Type::Scalar,
            value: ValueId(10),
        }],
        nodes: alloc::vec![
            Node::Input { index: 0 },              // 0 haystack
            Node::Input { index: 1 },              // 1 needle
            Node::Input { index: 2 },              // 2 n
            Node::PackUsize { value: ValueId(2) }, // 3 n as bytes
            Node::Call {
                port: String::from(EACH),
                args: alloc::vec![ValueId(0), ValueId(3)]
            }, // 4 fold hay (composition)
            Node::Call {
                port: String::from(S),
                args: alloc::vec![ValueId(4), ValueId(3)]
            }, // 5 derived bound
            Node::PackUsize { value: ValueId(5) }, // 6 bound as bytes
            Node::ConstantScalar { value: 1 },     // 7 one
            Node::PackUsize { value: ValueId(7) }, // 8 one as bytes
            Node::Call {
                port: String::from(EACH),
                args: alloc::vec![ValueId(1), ValueId(8)]
            }, // 9 fold needle (composition)
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(4), ValueId(9), ValueId(6)]
            }, // 10 search
        ],
    }
}

/// `toupper ∘ memchr ∘ slice ∘ memchr`: a derived value selects a **buffer**. The
/// folded haystack is sliced at the origin the first `memchr` derived, and the
/// second `memchr` searches that slice. When the origin is absent the second search
/// is **not dispatched** — a data-dependent non-execution, expressed as a `Select`.
fn ir_toupper_memchr_suffix() -> CompositionIR {
    CompositionIR {
        id: String::from(crate::porting::composition_suffix::COMPOSITION_TOUPPER_MEMCHR_SUFFIX.id),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![
            Output {
                ty: Type::Scalar,
                value: ValueId(19),
            },
            // The B fold is observed unconditionally: its status is evidence even
            // when the origin is absent and the second search is skipped.
            Output {
                ty: Type::Bool,
                value: ValueId(20),
            },
        ],
        nodes: alloc::vec![
            Node::Input { index: 0 }, // 0 haystack
            Node::Input { index: 1 }, // 1 needle A
            Node::Input { index: 2 }, // 2 needle B
            Node::Input { index: 3 }, // 3 n (the window)
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(0)
            }, // 4 folded hay
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(1)
            }, // 5 folded A
            Node::MapBytes {
                port: String::from(T),
                input: ValueId(2)
            }, // 6 folded B
            Node::PackUsize { value: ValueId(3) }, // 7 window as bytes
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(4), ValueId(5), ValueId(7)]
            }, // 8 origin
            Node::ConstantScalar { value: 0 }, // 9 zero
            Node::Compare {
                lhs: ValueId(8),
                op: CompareOp::Ge,
                rhs: ValueId(9)
            }, // 10 origin >= 0
            Node::Binary {
                op: BinOp::Sub,
                lhs: ValueId(3),
                rhs: ValueId(8)
            }, // 11 suffix_len = window - origin
            Node::Slice {
                input: ValueId(4),
                origin: ValueId(8),
                length: Some(ValueId(11))
            }, // 12 the slice
            Node::PackUsize { value: ValueId(11) }, // 13 suffix_len as bytes
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(12), ValueId(6), ValueId(13)]
            }, // 14 search suffix
            Node::Compare {
                lhs: ValueId(14),
                op: CompareOp::Ge,
                rhs: ValueId(9)
            }, // 15 j >= 0
            Node::Binary {
                op: BinOp::Add,
                lhs: ValueId(8),
                rhs: ValueId(14)
            }, // 16 origin + j
            Node::ConstantScalar { value: -1 }, // 17 absent
            Node::Select {
                condition: ValueId(15),
                then_value: ValueId(16),
                else_value: ValueId(17)
            }, // 18 j < 0 ? -1 : origin+j
            Node::Select {
                condition: ValueId(10),
                then_value: ValueId(18),
                else_value: ValueId(17)
            }, // 19 origin < 0 ? -1 : 18
            Node::Observe { value: ValueId(6) }, // 20 observe the B fold unconditionally
        ],
    }
}

/// `toupper_each ∘ memchr ∘ slice ∘ toupper_memchr`: a sealed **composition
/// consumes a buffer another composition selected**. The nested `toupper_each`
/// produces the folded view, a leaf derives the origin, and the nested
/// `toupper_memchr` consumes the slice — folding it itself, because the slice is
/// taken from the *unfolded* haystack.
fn ir_toupper_each_slice_search() -> CompositionIR {
    CompositionIR {
        id: String::from(
            crate::porting::composition_slice_search::COMPOSITION_TOUPPER_EACH_SLICE_SEARCH.id,
        ),
        locale_contract: String::from("C"),
        inputs: alloc::vec![Type::Bytes, Type::Bytes, Type::Bytes, Type::Scalar],
        outputs: alloc::vec![Output {
            ty: Type::Scalar,
            value: ValueId(20),
        }],
        nodes: alloc::vec![
            Node::Input { index: 0 },              // 0 haystack
            Node::Input { index: 1 },              // 1 needle A
            Node::Input { index: 2 },              // 2 needle B
            Node::Input { index: 3 },              // 3 n (the window)
            Node::PackUsize { value: ValueId(3) }, // 4 window as bytes
            Node::Call {
                port: String::from(EACH),
                args: alloc::vec![ValueId(0), ValueId(4)]
            }, // 5 fold hay (composition)
            Node::ConstantScalar { value: 1 },     // 6 one
            Node::PackUsize { value: ValueId(6) }, // 7 one as bytes
            Node::Call {
                port: String::from(EACH),
                args: alloc::vec![ValueId(1), ValueId(7)]
            }, // 8 fold needle A (composition)
            Node::Call {
                port: String::from(M),
                args: alloc::vec![ValueId(5), ValueId(8), ValueId(4)]
            }, // 9 origin
            Node::ConstantScalar { value: 0 },     // 10 zero
            Node::Compare {
                lhs: ValueId(9),
                op: CompareOp::Ge,
                rhs: ValueId(10)
            }, // 11 origin >= 0
            Node::Binary {
                op: BinOp::Sub,
                lhs: ValueId(3),
                rhs: ValueId(9)
            }, // 12 suffix_len = window - origin
            // 13: the slice is taken from the ORIGINAL haystack, not the folded view.
            Node::Slice {
                input: ValueId(0),
                origin: ValueId(9),
                length: Some(ValueId(12))
            }, // 13 slice
            Node::PackUsize { value: ValueId(12) }, // 14 suffix_len as bytes
            // 15: a sealed composition consumes the derived buffer, folding it itself.
            Node::Call {
                port: String::from(TMEM),
                args: alloc::vec![ValueId(13), ValueId(2), ValueId(14)]
            }, // 15 search
            Node::Compare {
                lhs: ValueId(15),
                op: CompareOp::Ge,
                rhs: ValueId(10)
            }, // 16 j >= 0
            Node::Binary {
                op: BinOp::Add,
                lhs: ValueId(9),
                rhs: ValueId(15)
            }, // 17 origin + j
            Node::ConstantScalar { value: -1 }, // 18 absent
            Node::Select {
                condition: ValueId(16),
                then_value: ValueId(17),
                else_value: ValueId(18)
            }, // 19 j < 0 ? -1 : origin+j
            Node::Select {
                condition: ValueId(11),
                then_value: ValueId(19),
                else_value: ValueId(18)
            }, // 20 origin < 0 ? -1 : 19
        ],
    }
}
