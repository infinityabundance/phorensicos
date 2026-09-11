# Immutable store generations and demand-driven porting (Phase 8)

**Status: Phase 8 core implemented.** This document is normative.

## 1. A publication is a new generation, not a mutation

A running `SealedNativeService` binds to one **immutable store generation** for
its whole lifetime. A newly qualified port is published as generation `N+1`
derived from generation `N`; the bound generation is never hot-edited, so a
session cannot silently change semantics halfway through.

`phost/src/porting/store_generation.rs`:

```text
StoreGeneration {
    schema_version,
    generation_id,
    parent,
    entries: sorted [GenerationEntry { target, artifact_hash, seal_profile, evidence_id }],
    evidence_closure,
}
```

The identity is a pure function of `(schema, parent, sorted entries, closure)`
with a domain tag:

```text
StoreGenerationId = SHA-256("PHOR/STORE-GENERATION/v1\0" || canonical_bytes)
```

No wall clock participates. `verify()` recomputes the identity and fails closed
on an unknown schema, an empty generation, a duplicate target, an unsorted
(reordered/substituted) entry list, a self-parent or a missing closure.

## 2. Publication and lineage

`StoreGeneration::publish(parent, new_entries, closure)` carries the parent's
entries forward, replaces any whose target matches, sorts, and derives the child.
`GenerationLedger::append` enforces that the first generation is genesis and each
later one names the current head as its parent — a generation whose parent is not
the head is refused.

## 3. A session serves only its generation's artifacts

`GenerationBinding::bind(generation)` captures the generation's entries.
`SealedNativeService::with_generation(binding)` binds a session; on every native
call the service asserts that the served object hash is exactly the artifact the
generation bound, and a mismatch is a **broken seal** (terminal), never a
fallback. The binding is deliberately *not* part of the session hash, so existing
session evidence is unchanged.

`GenerationLedger::open(id, minimum)` refuses to open a generation older than
`minimum` (a rollback). Fail-closed cases: missing parent, unknown schema, entry
mix-and-match, artifact substitution, evidence-closure mismatch, rollback.

The genesis generation is built from the committed sealed index
(`phorport/src/generations.rs::genesis_from_index`); the legacy evidence did not
carry an autonomous closure, so the genesis closure is the deterministic digest
of the entries it publishes.

## 4. Demand-driven porting, as data

A runtime miss does **not** block. `NativeDispatcher` may be given a bounded,
non-blocking `DemandSink` (`phost/src/porting/demand.rs`); on a fallback it
records a `PortDemand`:

```text
requested_surface, callsite_family, demand_count,
store_generation, available_dependencies
```

Recording never blocks and never fails the call; beyond the cap, records are
dropped and counted. Identical demands (same surface, callsite and generation)
are aggregated. The runtime emits data; the host foundry may later attempt a
reconstruction and publish a future generation.

## 5. Deterministic prioritization

`phorport/src/demand.rs` ranks pending demands with **fixed integer weights** and
records every contribution:

```text
runtime_demand_count                 +100
existing_sealed_fanin                 +40
callers_helped                        +30
sealed_dependency_count                +5 per dependency
oracle_available                      +20
compiler_expressible                  +15
qualification_cost                    -10
historical_failed_attempt_burden      -25 per prior failure
```

The output is a `DemandScore` with the full breakdown, sorted by total then
surface. There is no probability of success: the system may say *"this input
distinguishes 14 of 21 surviving candidate pairs"*, never *"this candidate is
93.2% correct"*.

`phorport demand rank <surface>…` prints the ranking; `phorport store generations`
prints the genesis generation and, with `--publish-target/--publish-artifact/
--closure [--publish-evidence <id>] [--out <file>]`, a child generation. `--out`
writes the published generation as canonical JSON, so a publication can be a
committed, re-verifiable artifact rather than a screen of text.

## 6. The committed generations

The repository publishes one generation from the committed baseline, for the
`strspn` contract-provenance correction (`docs/CONTRACT_PROVENANCE_MIGRATION.md`):

```text
generation 0 (genesis)   f47d1bec90d8bba92f441138fc29db77094c9a35420057a5d60c3ee4163bdf73
                         13 entries, all LegacyV1, from phost/evidence/store/index.json
generation 1            8e6fafec7b2b66ced51d8699d86fc264983fe2cd501c11b9c316f4d125912ba5
                         parent = generation 0
                         14 entries = the 13 carried forward unchanged
                           + libc:strspn:c-locale:u64:v1 under AutonomousV1
                         evidence_closure = the successor's autonomous closure
```

Committed at
`phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1/store_generation.json`
and verified by `./verify_supersession.sh` (identity recomputes, parent bound, the
historical `posix:strspn` leaf carried forward unchanged). Publication is
**additive**: the committed v1 index is not mutated, the long-lived session verdict
(`d219be2c…`) is unchanged, existing sessions stay bound to their generation, and a
new session may open generation 1.

## 7. Consuming a generation: exact materialization

Publishing a generation is only half of §19. `phost/src/porting/generation_session.rs`
closes the other half: a session bound to generation N materializes the **exact**
runtime index N represents and serves every port it binds.

The invariant is **equality**, not containment:

```text
Artifacts(dispatcher) == Artifacts(generation)
```

The materializer takes three committed inputs — the verified baseline index, the
committed `StoreGeneration`, and the committed autonomous artifact registry
(`phost/evidence/autonomous/artifacts.json`; the locator for a leaf a later campaign
published) — and:

1. verifies the generation and that its parent is the genesis of the committed
   baseline index (a foreign parent is refused);
2. resolves each entry against the baseline index (carrying its object path and
   metadata) or, for a new target, against the registry;
3. verifies each artifact against the generation's recorded hash, and each new
   object's bytes against that hash;
4. requires the result to equal the generation exactly, and every baseline entry to
   be carried forward (a substituted, missing, dropped or extra port fails closed).

Only then is the service opened (`SealedNativeService::from_materialized`) with a
`GenerationBinding`. A bound session also **refuses** a port its generation does not
bind (fail closed) rather than falling back, so a session's semantics are exactly
its generation's.

The committed `phost/evidence/session/generation_session.json` records the
generation-1 session under a distinct schema,
`phorensic.porting.generation_session.v1`:

```text
generation:        8e6fafec…   parent: f47d1bec…
ports:             14   calls: 14   native: 14   fallback: 0   broken seal: 0
objects mapped:     7   dispatches: 69   store loads: 1
posix:strspn  -> c93271d0… (historical LegacyV1 object)
libc:strspn   -> c40c4e3a… (successor AutonomousV1 object)
```

The lineage does not merely *remember* both identities: the runtime distinguishes
and serves both, each from its own artifact, in one immutable generation. The
generation binding is deliberately **not** part of the legacy session hash, so the
committed baseline session verdict (`d219be2c…`, 13 ports, 6 objects) is byte
identical. `./verify_generation_session.sh` proves all of the above, and that the
session runs with no compiler, oracle, FRF, FRF-Fuzz or Gemel available.

## 8. What is not claimed

Generations provide content-identity and immutable lineage now; external
signatures are a later layer and are not required by this phase.
