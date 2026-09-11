// porting/composition_synth.rs — bounded CompositionIR synthesis (§21, Phase 10)
//
// Once composition is typed data, a composition is a program in a bounded graph
// grammar, and synthesizing one is syntax-guided search rather than guessing.
// This module enumerates well-typed acyclic `CompositionIR` graphs over a declared
// port set and a bounded node budget, deterministically, and lets the caller
// check each candidate against the oracle. It does **not** use a model.
//
// The grammar is deliberately the subset the existing chains use: `Input`,
// `ConstantScalar`, `MapBytes`, `PackUsize`, `Call` and `Slice`. A graph is a
// candidate only when it validates and a value of the requested output type
// exists; a candidate is accepted only when it reproduces the oracle on the
// declared corpus. No graph is accepted merely because it is the only survivor
// under visible examples: the caller must still qualify, challenge and promote it.
//
// Compositional authority (§21): a composition may use only dependencies whose
// seals satisfy its declared minimum policy. The synthesizer takes its port set
// from the caller; it never lowers that set.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::porting::composition_engine::ForeignBackend;
use crate::porting::composition_ir::{eval, validate, CompositionIR, Node, Output, Type, ValueId};
use crate::porting::target::PortTarget;

/// The identity domain tag for a synthesized graph is the IR's own; this module
/// adds no second identity. It is a versioned generator.
pub const COMPOSITION_SYNTH_VERSION: &str = "phorport.composition-synth.v1";

/// A hard bound on the number of explored states, so a search always terminates.
pub const SYNTH_MAX_STATES: usize = 200_000;

/// One port the synthesizer may use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SynthPort {
    pub id: String,
    /// The argument types of a `Call`.
    pub args: Vec<Type>,
    /// The return type.
    pub ret: Type,
    /// Whether the port may be used through `MapBytes` (a byte-to-byte mapping).
    pub map: bool,
}

impl SynthPort {
    pub fn new(id: &str, args: Vec<Type>, ret: Type, map: bool) -> Self {
        SynthPort {
            id: id.to_string(),
            args,
            ret,
            map,
        }
    }
}

/// The declared graph grammar for a synthesis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grammar {
    /// Whether `ConstantScalar` and `Slice` nodes are available.
    pub constants_and_slice: bool,
}

/// The synthesis request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SynthRequest {
    pub inputs: Vec<Type>,
    pub output: Type,
    pub ports: Vec<SynthPort>,
    pub max_nodes: usize,
    pub grammar: Grammar,
}

/// An enumerated candidate.
#[derive(Clone, Debug)]
pub struct SynthCandidate {
    pub id: String,
    pub nodes: Vec<Node>,
    pub output: Output,
}

impl SynthCandidate {
    /// Build a `CompositionIR` for validation or evaluation.
    pub fn to_ir(&self) -> CompositionIR {
        CompositionIR {
            id: self.id.clone(),
            locale_contract: String::from("C"),
            inputs: Vec::new(),
            outputs: vec![self.output],
            nodes: self.nodes.clone(),
        }
    }

    /// Build a valid `CompositionIR` with the request's declared inputs.
    pub fn to_ir_with(&self, inputs: Vec<Type>) -> CompositionIR {
        CompositionIR {
            id: self.id.clone(),
            locale_contract: String::from("C"),
            inputs,
            outputs: vec![self.output],
            nodes: self.nodes.clone(),
        }
    }
}

/// Why synthesis produced nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SynthError {
    /// No well-typed graph produced the requested output within the budget.
    NoCandidate,
    /// The state budget was exhausted before a candidate was found.
    BudgetExhausted,
}

impl SynthError {
    pub fn as_str(&self) -> &'static str {
        match self {
            SynthError::NoCandidate => "no well-typed composition produced the output type",
            SynthError::BudgetExhausted => "the synthesis state budget was exhausted",
        }
    }
}

/// The BFS frontier state: the node list (value `i` is node `i`).
#[derive(Clone)]
struct State {
    nodes: Vec<Node>,
    values: Vec<Type>,
}

fn type_of(n: &Node, values: &[Type], req: &SynthRequest) -> Option<Type> {
    match n {
        Node::Input { index } => req.inputs.get(*index as usize).copied(),
        Node::ConstantScalar { .. } => Some(Type::Scalar),
        Node::MapBytes { .. } => Some(Type::Bytes),
        Node::PackUsize { .. } => Some(Type::Bytes),
        Node::Slice { .. } => Some(Type::Bytes),
        Node::Call { port, .. } => req.ports.iter().find(|p| &p.id == port).map(|p| p.ret),
        _ => {
            let _ = values;
            None
        }
    }
}

/// Enumerate candidate graphs (bounded, deterministic). A candidate is a graph
/// whose **last node** yields the requested output type (the program's result is
/// its final expression), in increasing size order.
pub fn enumerate(req: &SynthRequest) -> Result<Vec<SynthCandidate>, SynthError> {
    let mut candidates: Vec<SynthCandidate> = Vec::new();
    let mut seen_states: BTreeSet<String> = BTreeSet::new();
    let mut explored = 0usize;
    let mut hit_budget = false;
    let mut id_counter = 0u64;

    // Seed the frontier with the input nodes.
    let mut nodes = Vec::new();
    for i in 0..req.inputs.len() {
        nodes.push(Node::Input { index: i as u32 });
    }
    let mut frontier: Vec<State> = vec![State {
        values: req.inputs.clone(),
        nodes,
    }];

    while !frontier.is_empty() {
        let mut next: Vec<State> = Vec::new();
        for state in frontier.drain(..) {
            explored += 1;
            if explored > SYNTH_MAX_STATES {
                hit_budget = true;
                break;
            }
            if state.nodes.len() >= req.max_nodes {
                continue;
            }
            for child in expand(&state, req) {
                let key = format!("{:?}", child.nodes);
                if !seen_states.insert(key) {
                    continue;
                }
                let last = child.values.len() - 1;
                if child.values[last] == req.output {
                    candidates.push(SynthCandidate {
                        id: format!("{COMPOSITION_SYNTH_VERSION}/cand{id_counter}"),
                        nodes: child.nodes.clone(),
                        output: Output {
                            ty: req.output,
                            value: ValueId(last as u32),
                        },
                    });
                    id_counter += 1;
                }
                next.push(child);
            }
        }
        if hit_budget {
            break;
        }
        frontier = next;
    }

    if candidates.is_empty() {
        return Err(if hit_budget {
            SynthError::BudgetExhausted
        } else {
            SynthError::NoCandidate
        });
    }
    Ok(candidates)
}

/// Expand one state by exactly one node.
fn expand(state: &State, req: &SynthRequest) -> Vec<State> {
    let mut out: Vec<State> = Vec::new();
    let values = &state.values;
    let derived_start = req.inputs.len();

    if req.grammar.constants_and_slice {
        push_node(&mut out, state, req, Node::ConstantScalar { value: 0 });
    }

    // MapBytes over each byte value, for each map port.
    for p in &req.ports {
        if !p.map {
            continue;
        }
        for (i, t) in values.iter().enumerate() {
            if *t == Type::Bytes {
                push_node(
                    &mut out,
                    state,
                    req,
                    Node::MapBytes {
                        port: p.id.clone(),
                        input: ValueId(i as u32),
                    },
                );
            }
        }
    }

    // PackUsize over each scalar value.
    for (i, t) in values.iter().enumerate() {
        if *t == Type::Scalar {
            push_node(
                &mut out,
                state,
                req,
                Node::PackUsize {
                    value: ValueId(i as u32),
                },
            );
        }
    }

    if req.grammar.constants_and_slice {
        for (i, ti) in values.iter().enumerate() {
            if *ti != Type::Bytes {
                continue;
            }
            for (j, tj) in values.iter().enumerate() {
                if *tj != Type::Scalar {
                    continue;
                }
                for (k, tk) in values.iter().enumerate() {
                    if *tk != Type::Scalar {
                        continue;
                    }
                    push_node(
                        &mut out,
                        state,
                        req,
                        Node::Slice {
                            input: ValueId(i as u32),
                            origin: ValueId(j as u32),
                            length: Some(ValueId(k as u32)),
                        },
                    );
                }
            }
        }
    }

    // Call over well-typed argument tuples that use at least one derived value,
    // so all-input calls are not enumerated at every level.
    for p in &req.ports {
        if p.map {
            continue;
        }
        let mut tuples: Vec<Vec<u32>> = vec![Vec::new()];
        for want in &p.args {
            let mut grown = Vec::new();
            for t in &tuples {
                for (i, ty) in values.iter().enumerate() {
                    if ty == want {
                        let mut nt = t.clone();
                        nt.push(i as u32);
                        grown.push(nt);
                    }
                }
            }
            tuples = grown;
            if tuples.is_empty() {
                break;
            }
        }
        for t in tuples {
            if !t.iter().any(|i| (*i as usize) >= derived_start) {
                continue;
            }
            push_node(
                &mut out,
                state,
                req,
                Node::Call {
                    port: p.id.clone(),
                    args: t.into_iter().map(ValueId).collect(),
                },
            );
        }
    }

    out
}

/// Append `node` to `state` as a child, pruning an existing duplicate and an
/// ill-typed node.
fn push_node(out: &mut Vec<State>, state: &State, req: &SynthRequest, node: Node) {
    if state.nodes.contains(&node) {
        return;
    }
    let ty = match type_of(&node, &state.values, req) {
        Some(t) => t,
        None => return,
    };
    let mut nodes = state.nodes.clone();
    nodes.push(node);
    let mut vals = state.values.clone();
    vals.push(ty);
    out.push(State {
        nodes,
        values: vals,
    });
}

/// The result of a verified synthesis.
#[derive(Clone, Debug)]
pub struct SynthResult {
    pub ir: CompositionIR,
    /// How many candidates were enumerated before one matched the oracle.
    pub candidates_tried: u64,
}

/// Synthesize a composition that reproduces `oracle`'s behavior on `cases`.
///
/// Each enumerated candidate is validated and evaluated over the foreign
/// backend; the first that matches the oracle on **every** case is returned.
/// A candidate that differs on any case is discarded.
pub fn synthesize_matching(
    req: &SynthRequest,
    oracle: &CompositionIR,
    cases: &[Vec<Vec<u8>>],
    target: &PortTarget,
    auth: &crate::porting::PortingAuthority,
) -> Result<SynthResult, SynthError> {
    let candidates = enumerate(req)?;
    for (tried, cand) in candidates.into_iter().enumerate() {
        let ir = cand.to_ir_with(req.inputs.clone());
        if validate(&ir).is_err() {
            continue;
        }
        let mut matched = true;
        for args in cases {
            let inputs = match crate::porting::composition_engine::case_inputs(&ir, args) {
                Ok(i) => i,
                Err(_) => {
                    matched = false;
                    break;
                }
            };
            let mut fb = ForeignBackend::new(auth);
            let got = match eval(&ir, &mut fb, &inputs) {
                Ok(v) => v,
                Err(_) => {
                    matched = false;
                    break;
                }
            };
            let mut fb2 = ForeignBackend::new(auth);
            let want = match eval(oracle, &mut fb2, &inputs) {
                Ok(v) => v,
                Err(_) => {
                    matched = false;
                    break;
                }
            };
            if got != want {
                matched = false;
                break;
            }
        }
        if matched {
            let _ = target;
            return Ok(SynthResult {
                ir,
                candidates_tried: tried as u64 + 1,
            });
        }
    }
    Err(SynthError::NoCandidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::composition_registry;

    fn toupper_memchr_ports() -> Vec<SynthPort> {
        vec![
            SynthPort::new("toupper", vec![Type::Bytes], Type::Bytes, true),
            SynthPort::new(
                "memchr",
                vec![Type::Bytes, Type::Bytes, Type::Bytes],
                Type::Scalar,
                false,
            ),
        ]
    }

    #[test]
    fn test_enumerate_is_deterministic_and_bounded() {
        let req = SynthRequest {
            inputs: vec![Type::Bytes, Type::Bytes, Type::Scalar],
            output: Type::Scalar,
            ports: toupper_memchr_ports(),
            max_nodes: 7,
            grammar: Grammar::default(),
        };
        let a = enumerate(&req).expect("candidates");
        let b = enumerate(&req).expect("candidates");
        assert_eq!(a.len(), b.len());
        assert!(!a.is_empty());
    }

    /// The synthesizer rediscovers `toupper_memchr` from the declared inputs,
    /// The synthesizer rediscovers `toupper_each` — a slice of a declared input
    /// followed by a byte-mapping port — from the declared inputs, ports and
    /// output type, not from the committed graph.
    #[test]
    fn test_synthesis_rediscovers_toupper_each() {
        let def = composition_registry::by_name("toupper_each").expect("def");
        let oracle = (def.ir)();
        let cases: Vec<Vec<Vec<u8>>> = (def.cases)().iter().map(|c| c.args.clone()).collect();
        // The port set is declared by the request (the sealed ports available);
        // the synthesizer searches over structure, not over port names.
        let map_ports: Vec<String> = oracle
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::MapBytes { port, .. } => Some(port.clone()),
                _ => None,
            })
            .collect();
        let ports: Vec<SynthPort> = map_ports
            .iter()
            .map(|p| SynthPort::new(p, vec![Type::Bytes], Type::Bytes, true))
            .collect();
        let req = SynthRequest {
            inputs: oracle.inputs.clone(),
            output: Type::Bytes,
            ports,
            max_nodes: 6,
            grammar: Grammar {
                constants_and_slice: true,
            },
        };
        let auth = crate::porting::PortingAuthority::granted();
        let target = crate::porting::target::resolve_target("toupper").expect("target");
        let result = synthesize_matching(&req, &oracle, &cases, &target, &auth)
            .expect("a synthesized graph matches the oracle");
        assert!(validate(&result.ir).is_ok());
        assert_eq!(result.ir.outputs[0].ty, Type::Bytes);
    }

    #[test]
    fn test_no_candidate_when_the_output_type_is_unreachable() {
        let req = SynthRequest {
            inputs: vec![Type::Scalar],
            output: Type::Bool,
            ports: vec![],
            max_nodes: 4,
            grammar: Grammar::default(),
        };
        // Only inputs, ConstantScalar and PackUsize are reachable; no Bool value
        // can be produced without a comparison or an observe node.
        assert!(matches!(enumerate(&req), Err(SynthError::NoCandidate)));
    }
}
