# Composition IR (`CompositionIR`)

**Status: Phase 2 complete.** Every composition the store publishes is now a typed
`CompositionIR` (data), evaluated by **one** interpreter; the runtime dispatcher
resolves a composition port through the IR, and no bespoke composition runner is on
any live path.

Phase 1 proved sealed ports compose — but with seven *bespoke Rust runners*, one per
chain shape. A new composition meant a new runner, and the oracle implementation
(`dialect_cage`) and the native implementation (`composition*.rs`) could drift
because they were different code. Phase 2 makes composition **data**: a small,
typed, versioned, acyclic program over already-sealed ports.

The composition table is `phost/src/porting/composition_registry.rs`; the IR core is
`phost/src/porting/composition_ir.rs`; the generic court and the two backends are
`phost/src/porting/composition_engine.rs`.

## 1. The graph

```rust
struct CompositionIR {
    id: String,               // phor:compose:…
    locale_contract: String,
    inputs: Vec<Type>,        // Bytes | Scalar | Bool
    outputs: Vec<Output>,     // Output { ty, value: ValueId }
    nodes: Vec<Node>,
}
```

Values are SSA-like: a `ValueId` is the index of the node that produced it, and
every operand must reference an **earlier** node. That single rule makes the graph
acyclic by construction: there is no separate cycle check to get wrong.

| Node | Meaning |
|---|---|
| `Input { index }` | the declared input |
| `ConstantScalar { value }` | a literal scalar |
| `Call { port, args }` | call a sealed port (leaf **or composition**), return its observable |
| `MapBytes { port, input }` | apply a unary byte→byte port across every byte of a buffer |
| `PackUsize { value }` | encode a scalar as an 8-byte little-endian buffer (the ABI packing boundary) |
| `Observe { value }` | force a value's evaluation for its **effect** (an observed stage), yield a non-observable `Bool` |
| `Slice { input, origin, length }` | `input[origin .. origin+length]` |
| `Compare { lhs, op, rhs }` | a scalar comparison → `Bool` |
| `Binary { op, lhs, rhs }` | saturating `Add`/`Sub` on scalars |
| `Select { condition, then, else }` | a conditional value; only the taken branch is evaluated |

`Select` is how a **data-dependent non-execution is represented explicitly**:
evaluation is lazy, so a stage on the untaken branch is *never dispatched*. "A stage
did not run" can therefore never be confused with "a stage succeeded" — the court
reports it as `not_reached_cases` for that stage.

`Observe` is the dual: a stage whose *status* is evidence even when its result is
not consumed on this path (the suffix chain folds the second needle unconditionally,
so the fold's own status is observed even when the origin is absent and the second
search is skipped).

## 2. One interpreter, two backends

```rust
trait PortBackend {
    fn enter(&mut self, site: SiteId);                                   // reached
    fn call(&mut self, site: SiteId, port: &str, args: &[Value])
        -> Result<Value, BackendError>;                                  // dispatched
}

fn eval(ir: &CompositionIR, backend: &mut dyn PortBackend, inputs: &[Value])
    -> Result<Vec<Value>, EvalError>;
```

The *same* IR is evaluated on the foreign side (`ForeignBackend`, routing each leaf
call to the dialect cage and recursing into a nested composition's IR) and the
sealed side (`SealedBackend`, routing every call — leaf or composition — through
`NativeDispatcher`). Because there is one evaluator, the oracle chain and the sealed
chain cannot drift: a composition means exactly one thing. The court additionally
re-derives the oracle through the IR and fails closed if it disagrees with the
committed foreign traces (a harness/cage drift, never a candidate bug).

## 3. Content identity

```text
CompositionIrId = SHA-256( "PHOR/COMPOSITION-IR/v1\0" || canonical_bytes )
```

`canonical_bytes` is a length-prefixed typed encoding in which **node order is
significant** (it is the data-flow order). A composed artifact binds **four**
distinct identities, because two different graphs can be behaviorally identical:

| Identity | Meaning |
|---|---|
| `behavior_hash` | normalized observed behavior over the declared corpus (per-case stage statuses + observable) |
| `composition_ir_hash` | canonical exact graph identity |
| `dependency_binding_hash` | ordered identity of every dependency and the seal it published |
| `composition_artifact_hash` | `SHA-256("PHOR/COMPOSITION-ARTIFACT/v2\0" ‖ id ‖ ir_hash ‖ dependency_binding ‖ behavior_hash)` |

These are **v2** identities: the historical v1 `chain_hash`es are unchanged and
remain the seals the store publishes (`phost/evidence/store/index.json`). The v2
evidence lives in its own directory (`phost/evidence/composition_ir/<name>/`) and
uses the `…composition_verdict.v2` schema, so a v2 artifact can never be mistaken
for a v1 one. The difference is stated plainly: v1 hashed a bespoke per-chain tuple
with bespoke field names; v2 hashes the graph, its dependency binding, and the
normalized behavior separately, which is the correct ontology for a seal of a seal.

## 4. Validation and bounds

`validate()` enforces: bounded node/input/output counts (`MAX_NODES` 256,
`MAX_INPUTS` 16, `MAX_OUTPUTS` 8), no forward references, correct operand types,
in-range input indices, non-empty port names, and that each declared output's type
matches its value. Every failure is a named `IrError`; the interpreter never guesses.

## 5. The runtime path

`NativeDispatcher::dispatch_port` resolves a sealed **composition** port to its IR
and evaluates it (`composition_engine::eval_composition_port`). A nested composition
recurses through the same dispatcher, so every leaf-seal check, dispatch count and
per-port fan-in is preserved exactly. The sealed native service's committed session
verdict (`d219be2c…`, 13 calls / 68 dispatches / 6 objects) is byte-identical under
the IR runtime — the migration changed the mechanism, not the behavior.

## 6. Status and evidence

**Implemented (Phase 2 complete):**

* the typed IR, canonical encoding and `CompositionIrId`, validation, the lazy
  generic `eval`, and the site-aware `PortBackend` carrying fallback/broken/sealed
  outcomes and per-stage accounting;
* `ForeignBackend` and `SealedBackend`, and the generic court `run_ir_court`;
* the seven chains expressed as IR in `composition_registry.rs` (a new composition
  is new data plus, at most, a new corpus — no engine change);
* the `composition_runner(id)` branch removed; the runtime resolves compositions as
  data;
* **Phase 2 acceptance evidence** (`test_ir_court_agrees_with_the_legacy_court_for_every_composition`):
  for every composition the generic IR court and the legacy bespoke court agree on
  the external answers, the fallback/broken accounting, and (where the dispatch
  definition coincides) the dispatch count;
* committed v2 evidence for all seven compositions (`phost/evidence/composition_ir/`)
  verified by `verify_composition_ir_court.sh`, which checks the four identities,
  the per-stage accounting, and each stage's seal against the committed store — a
  seal of a seal.
