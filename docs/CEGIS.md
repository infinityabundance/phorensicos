# Bounded CEGIS and the candidate producer (Phase 6)

**Status: Phase 6 core complete** (`phorport/src/{producer,cegis,compile}.rs`).

Phase 5 remembers what happened. Phase 6 is the loop that makes the foundry
*generate* rather than replay: propose → falsify → revise, bounded, with an
untrusted generator that cannot see the qualification material.

## 1. The producer is an interface, not a wire

```rust
trait CandidateProducer {
    fn name(&self) -> &'static str;
    fn propose(&mut self, request: &CandidateRequest)
        -> Result<CandidateProposal, CandidateProducerError>;
}
```

Two implementations ship, and the loop never changes when a third is added:

* `ScriptedProducer` — a deterministic catalogue (used to exercise the loop where
  no agent is available; it is not a synthesizer);
* `ExternalCommandProducer` — an agent adapter: a command reads the request JSON on
  stdin and writes the proposed source on stdout.

A future enumerative Phor synthesizer slots in behind the same trait. The producer
is untrusted: its output is only ever a `CandidateProposal`.

## 2. Bounded constraints

`check_constraints` enforces the `PortSpec`'s `CandidateConstraints` on every
proposal *before* it is compiled: a non-empty source, the source-size bound, and
the leaf policy (no `extern`/`import`/`asm!`/`unsafe`/`use` in code — comments are
stripped so prose does not false-positive). The ELF-level half (no relocations, a
leaf entry) is the compiler/validator's job. A proposal that violates the search
space is recorded as durable negative knowledge and never compiled.

## 3. The isolated synthesis workspace

`SynthesisWorkspace::materialize` writes only permitted material: the request JSON
(the public contract, the design case *count*, previously discovered
counterexamples, bounded negative knowledge) and the public design case ids. It
never contains qualification fixtures, qualification oracle outputs, challenge
expected outcomes, or a withheld original source. `SynthesisWorkspace::audit`
scans every file for forbidden markers and returns any it finds — a leak is a
defect, so the audit fails closed, and the leaking-marker test asserts a
materialized workspace is clean.

## 4. The bounded CEGIS loop

`run_cegis` drives a monotonic state machine:

```
specified → candidate-proposed → design-checked → discovery-checked → candidate-frozen
                                                  ↘ candidate-rejected → (revise)
```

For each revision:

1. the producer proposes (bounded by `discovery_budget` revisions);
2. the constraints are enforced; violations become negative knowledge;
3. the proposal is compiled into a `CandidateIdentity` (source hash + object hash);
4. the **design court** runs (the port's own corpus). A divergence is minimized
   through the same court (Phase 4) and becomes a known counterexample and durable
   negative knowledge; the revision is rejected and the producer revises;
5. the **discovery court** runs. Its production hook is Phase 4's bounded FRF-Fuzz
   campaign; in the deterministic core it is absent and discovery closes at the
   design corpus;
6. a revision that survives both is **frozen** — the exact source bytes that later
   phases qualify and promote.

A candidate identity is a function of the source bytes: any source-byte change is a
new revision with a new identity, so no downstream evidence is ever reused across a
changed candidate.

## 5. Demonstrated

```sh
$ phorport campaign strspn \
      --source <a strspn that always returns 0> \
      --source examples/jit_port_strspn.phor
=== Candidate CEGIS Campaign ===
manifest:  957a07dd004aee3918416d6e574e8219b9f2cf794fd710b17ed687562b804ca4
revisions: 2
  revision 0: rejected by the design court (305 of 578 cases)
  revision 1: frozen (source f44ee9ed…)
frozen:    revision=1;source_hash=f44ee9ed…;object_hash=0014884a…
```

The design court rejects the wrong revision, the producer revises, and the
survivor is frozen. `revision 0`'s failure is durable negative knowledge; the
frozen identity is exactly the source bytes that would be qualified.

## 6. What Phase 6 does not yet do

* There is no shipped **synthesizer** — the producer in the CLI demo is scripted.
  The interface (and the request/constraint/workspace machinery) is the deliverable;
  a real enumerative Phor synthesizer or an agent adapter is a drop-in.
* **Disagreement-driven experiment selection** (`ExperimentSelector`, candidate
  committees, integer disagreement scores) is Phase 6's remainder; the production
  hook currently drives FRF-Fuzz with its own scheduler.
* **Qualification isolation is structural but not yet adversarial**: the workspace
  audit and the absent qualification universe are in place; the full HELD-OUT
  universe, the isolation tests and the leak-free error-message audit are Phase 7.
* **Reproducible-build provenance**: `compile_source` compiles a *copy* of the
  proposed source at a working path, and phorc's object bytes are path-sensitive;
  a reconstructed candidate therefore has its own object hash rather than the
  sealed one. The candidate's *source* identity is the primary key, and canonical
  invocation for byte-reproducible rebuilds is Phase 7/22 work.
