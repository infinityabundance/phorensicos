# Port Specification (`PortSpec`)

**Status: Phase 1 complete** (`phost/src/porting/portspec.rs`,`registry.rs`, `abi.rs`). This
document is normative for the `PortSpec` type; the *Status* section records
exactly what is and is not shipped.

`PortSpec` is the canonical, typed specification of a foreign API surface to be
reconstructed. It replaces the bootstrap `PortTarget` (a bag of strings) with a
structure that *separates the contract from the implementation observed* and can
be content-addressed so that a synthesizer, a mutation profile, a precondition
validator and a qualification policy can all bind to one identity.

## 1. What a spec distinguishes

| Field | Distinguishes |
|---|---|
| `target_id` | the qualified id `dialect:symbol:locale:contract:version` |
| `contract: ContractSource` | **which specification** the contract is drawn from (`IsoC`, `Posix`, `PhorensicComposition`, `ImplementationDefined { family }`) — not which library implemented it |
| `observable: ObservableSpec` | the semantic shape observed (`ExactBytes`, `UnsignedInteger{bits}`, `SignedInteger{bits}`, `Sign`, `IndexOrAbsent`, `Length`) |
| `projection: ObservationProjectionSpec` | how a raw observation is normalised to the observable (`Identity`, `LowByte`, `RawSign`, `PointerToIndex{base_arg}`) |
| `preconditions: &[PreconditionSpec]` | machine-checkable validity constraints on a case |
| `abi: AbiSpec` | the entry symbol and argument packing |
| `case_space: CaseSpaceSpec` | how the corpus is generated (`ExhaustiveFinite` / `BoundedDeterministic`) and by which generator |
| `qualification: QualificationPolicy` | how strongly the result is qualified (`HostObserved` today) |
| `candidate: CandidateConstraints` | what the candidate may not do (`leaf_only`, `no_relocations`, `max_source_bytes`) |
| `candidate_source` | the clean-room source bound into the seal |

A spec is entirely `&'static` data, so `PortSpec: Copy` and it stays inside the
same `no_std + alloc` closure as the rest of the runtime.

## 2. Content identity

The identity is **not** a hash of pretty JSON. JSON is a human projection and can
never be authoritative.

```text
PortSpecId = SHA-256( "PHOR/PORTSPEC/v1\0" || canonical_bytes(spec) )
```

`canonical_bytes` is a deterministic, length-prefixed typed encoding: counts and
lengths are fixed-width little-endian, strings are length-prefixed, lists are
ordered and counted. No delimiter can be smuggled through a string, and list
order is significant (so two specs differing only in the order of a
precondition list hash differently).

Bumping `PORTSPEC_DOMAIN` or the field order is a **versioned** act: it changes
every `PortSpecId` and therefore every seal that binds one.

### The pinned identities

| Target | `PortSpecId` |
|---|---|
| `libc:toupper:c-locale:u8:v1` | `a375d704d504131f164e79a3e7f476a27022604c7a9589aff14b5025df6d93e7` |
| `libc:memcmp:c-locale:sign:v1` | `fde427474312a5329f30d16fdfdd3ba38f0c690c01e4b606be98c7da420eb54f` |
| `libc:memchr:c-locale:index:v1` | `c41d3de23d13e775427c1237e6cc1d47489b638a92435e7b63de0938befe212d` |
| `libc:strlen:c-locale:u64:v1` | `c5f0882dec71150f33a1d8e161eed10f65eafc5e6df4dc81feed893c1aa0e61c` |
| `libc:strrchr:c-locale:index:v1` | `a5c0fcd53f17d975fcd9910c2c4f2c0a85aa4eb1f90154cbbc0ce60577f623ee` |
| `posix:strspn:c-locale:u64:v1` | `bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23` |

These are pinned by `test_spec_ids_are_pinned_golden`; any change to a semantic
field or to the encoding fails the test.

## 3. Precondition validation

`validate_case(spec, args) -> Result<ValidatedCase, PreconditionViolation>`

Only a `ValidatedCase` may reach foreign execution. A differential court
contaminated by undefined or out-of-contract behavior is not useful evidence: the
foreign oracle must never be invoked with a case that violates its safety or
semantic preconditions. The existing declarative preconditions are:

| Precondition | Meaning |
|---|---|
| `LengthWithinBuffer { len_arg, buffer_arg }` | the length argument does not exceed the buffer |
| `NulWithinBound { buffer_arg, bound_arg }` | the first NUL lies within the bound |
| `NulFree { arg }` | the argument contains no NUL |
| `IntegerRange { arg, min, max }` | the argument lies in a range |

Validation reports the *first* violated precondition, in declared order, so the
outcome is deterministic. A missing argument is a reported violation, never a
panic.

The declared preconditions are checked against the real corpora by
`test_every_leaf_corpus_is_in_contract`: a target whose own cases fail its own
preconditions would be unusable, so this test fails closed if a precondition is
mis-declared.

## 4. Status

**Implemented (Phase 1 complete):** the typed `PortSpec`, the domain-separated
canonical encoding and `PortSpecId`, all six leaf specs, the precondition
validator, the pinned golden identities, and `PortSpec::of_target`.

The generic machinery is now registry-driven (`phost/src/porting/registry.rs`),
not branch-driven:

* `target::cases_for` and `target::resolve_target` look the target up in the
extension registry instead of matching on an id;
* `candidate::run_candidate` dispatches to the registered candidate adapter;
* the execution court's `call_target` dispatches to the registered ABI adapter
(`abi.rs`) instead of a chain of `if target.id == …`;
* the foreign oracle remains `dialect_cage.rs`, a separate `OracleAdapter`
extension boundary.

Adding a leaf target is therefore a new `PortSpec` plus three narrowly scoped
extension points (**CaseGenerator**, candidate adapter, ABI adapter) and one row in
the registry — and **no** change to the generic engine. A static audit,
`registry::tests::test_generic_machinery_has_no_target_branches`, reads the source
of every generic `porting/` module and fails if a `.id ==` target branch
reappears, so another central `match target.id` cannot creep back in.

**Later phases:** `MutationProfile` (Phase 3), `OracleWitness` binding and the
broader `QualificationPolicy` variants (Phases 6–7), and `CompositionIR`
(Phase 2, which replaces the remaining `CompositionKind` dispatch).
