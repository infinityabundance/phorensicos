# Composition IR (`CompositionIR`)

**Status: Phase 2 core implemented** (`phost/src/porting/composition_ir.rs`).
The migration of the six bespoke runners onto the IR is the remainder of Phase 2.

Phase 1 proved sealed ports compose — but with six *bespoke Rust runners*, one per
chain shape. A new composition meant a new runner, and the oracle implementation
and the native implementation could drift because they were different code.
Phase 2 makes composition **data**: a small, typed, versioned, acyclic program
over already-sealed ports, evaluated by **one** interpreter.

## 1. The graph

```rust
struct CompositionIR {
    id: String,               // phor:compose:…
    locale_contract: String,
    inputs: Vec<Type>,
    outputs: Vec<Output>,     // Output { ty, value: ValueId }
    nodes: Vec<Node>,
}
```

Values are SSA-like: a `ValueId` is the index of the node that produced it, and
every operand must reference an **earlier** node. That single rule makes the graph
acyclic by construction and evaluation a single forward pass — there is no
separate cycle check to get wrong.

| Node | Meaning |
|---|---|
| `Input { index }` | the declared input |
| `ConstantScalar { value }` | a literal scalar |
| `Call { port, args }` | call a sealed port, return its observable |
| `MapBytes { port, input }` | apply a unary byte→byte port across a buffer |
| `Slice { input, origin, length }` | `input[origin .. origin+length]` |
| `Compare { lhs, op, rhs }` | a scalar comparison → `Bool` |
| `Select { condition, then, else }` | a conditional value |
| `FoldBytes { port, init, input }` | fold a buffer with a binary port |

`Select` is how a **data-dependent non-execution is represented explicitly**: a
stage that does not run is a value choice, never confused with a stage that ran
and succeeded.

## 2. One interpreter, two backends

```rust
trait PortBackend {
    fn call(&mut self, port: &str, args: &[Value]) -> Result<Value, BackendError>;
}

fn eval(ir: &CompositionIR, backend: &mut dyn PortBackend, inputs: &[Value])
    -> Result<Vec<Value>, EvalError>;
```

The *same* IR is evaluated on the foreign side (`ForeignBackend`, routing to the
dialect cage) and the sealed side (`SealedBackend`, routing through
`NativeDispatcher`). Because there is one evaluator, the oracle implementation and
the native implementation cannot drift: a composition means exactly one thing.

## 3. Content identity

```text
CompositionIrId = SHA-256( "PHOR/COMPOSITION-IR/v1\0" || canonical_bytes )
```

`canonical_bytes` is a length-prefixed typed encoding in which **node order is
significant** (it is the data-flow order). As Phase 2 completes, a seal binds not
one hash but four distinct identities — this is deliberate, because two different
graphs can be behaviorally identical:

| Identity | Meaning |
|---|---|
| `behavior_hash` | normalized observed behavior over the declared corpus |
| `composition_ir_hash` | canonical exact graph identity |
| `dependency_binding_hash` | ordered identity of every dependency and its seal |
| `composition_artifact_hash` | `H(schema, PortSpecId, composition_ir_hash, dependency_binding, behavior_hash)` |

## 4. Validation and bounds

`validate()` enforces: bounded node/input/output counts (`MAX_NODES` 256,
`MAX_INPUTS` 16, `MAX_OUTPUTS` 8), no forward references, correct operand types,
in-range input indices, non-empty port names, and that each declared output's type
matches its value. Every failure is a named `IrError`; the interpreter never
guesses.

## 5. Status

**Implemented (Phase 2 core):** the typed IR, canonical encoding and
`CompositionIrId`, validation (bounded, typed, acyclic), the generic `eval` over a
`PortBackend`, and an equivalence test that expresses the first composition
(`toupper ∘ memchr`) as IR and reproduces the **foreign oracle over the whole
560-case corpus** through a backend backed by the clean-room mirrors.

**Remainder of Phase 2:** migrate the six bespoke runners
(`composition.rs`, `composition_strlen_memchr.rs`, `composition_pair.rs`,
`composition_toupper_each.rs`, `composition_nested.rs`,
`composition_suffix.rs`) onto the IR, implement `ForeignBackend`/`SealedBackend`
over the dialect cage and `NativeDispatcher`, remove the `CompositionKind`
dispatch, and version the artifact identities (the four-hash model above) without
faking byte equality with the pre-IR verdicts. Every committed composition verdict
must be preserved or the migration must be explicitly versioned.
