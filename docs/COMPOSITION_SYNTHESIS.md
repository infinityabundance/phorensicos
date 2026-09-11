# Bounded CompositionIR synthesis (Phase 10)

**Status: Phase 10 core implemented** (`phost/src/porting/composition_synth.rs`).

Once composition is typed data, a composition is a program in a bounded graph
grammar, and synthesizing one is syntax-guided search rather than guessing.

## 1. The request

A synthesis is a declared request, never an open-ended prompt:

```text
inputs:        the typed inputs of the desired composition
output:        the type of the desired observable
ports:         the sealed ports available, with their argument and return types
grammar:       which node forms are available
max_nodes:     a hard node budget
```

`SynthPort` names a port and whether it may be used through `MapBytes` (a
byte-to-byte mapping) or only through `Call`.

## 2. The search

`enumerate` explores the graph grammar breadth-first, one node at a time, and
emits a candidate whenever the **last node** yields the requested output type —
the program's result is its final expression. The search is deterministic and
bounded:

* a node that already exists verbatim is not re-added (no redundant duplicates);
* a `Call` must use at least one derived value, so all-input calls are not
  re-enumerated at every level;
* `ConstantScalar`/`Slice` are available only when the request's grammar enables
  them;
* the state budget (`SYNTH_MAX_STATES`) always terminates the search.

## 3. Verification, not blind acceptance

`synthesize_matching` validates every candidate and evaluates it over the
**foreign backend** against the oracle's own behavior on the composition corpus.
The first candidate that matches on **every** case is returned; every mismatch is
discarded. No graph is accepted merely because it is the only survivor under
visible examples: the caller must still qualify, challenge, FRF-verify and
promote it, exactly as for any other candidate.

## 4. Demonstrated

The test `test_synthesis_rediscovers_toupper_each` gives the synthesizer the
declared inputs (`[Bytes, Scalar]`), the declared port set (the sealed
`toupper`), the output type (`Bytes`) and the oracle's behavior, and it
rediscovers a graph equivalent to the committed `toupper_each` chain — a slice of
a declared input followed by a byte-mapping port — without being given the
committed graph.

## 5. Bounded claim and limits

The grammar is deliberately the subset the existing chains use. The typed-DAG
search is exact but its branching grows with the number of ports and live values,
so the larger chains (e.g. the seven-node `toupper_memchr`) are **not** reached
within the current budget: the demonstration is the five-node
slice-plus-map shape. A goal-directed or observational-equivalence-pruned search
is the natural next step; it is not implemented.

Compositional authority (§21) is a policy the caller enforces: a composition may
use only dependencies whose seals satisfy its declared minimum policy, and the
synthesizer never widens the port set it was given.
