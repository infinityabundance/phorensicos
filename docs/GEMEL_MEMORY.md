# Gemel longitudinal memory (Phase 5)

**Status: Phase 5 core complete** (`phorport/src/memory.rs`).

A foundry that runs the same search again is not learning. Phase 5 makes prior work
— including **wrong** work — durable and retrievable, so a later campaign is better
informed because earlier campaigns happened.

## 1. Gemel is the memory, not a copy of it

`PortMemory` uses Gemel's own object model (`Family`, `Field`, `Value`), its
content-addressed store (`insert_object`, `scan_canonical`) and its Gids. It does
not reimplement Gemel semantics and does not maintain a parallel journal.

Records ride Gemel's `Residual` schema (the negative-knowledge family): a `summary`,
a `classification` (`contract_mismatch` — a differential divergence is exactly
that), timestamps, and the structured phorport fields in the residual
`scope.paths` array (`["phorport.port.memory.v1", kind, target, candidate_hash,
residual, detail]`). Because the schema is Gemel's, the record is a first-class
Gemel object, not an opaque blob.

Gemel is **optional**. `PortMemory::open` discovers a repository by walking up from
the workspace; if there is none, every campaign runs standalone. A missing memory
layer is never a campaign failure.

## 2. What is remembered

| Kind | Meaning |
|---|---|
| `counterexample` | a differential counterexample was discovered and minimized |
| `candidate-rejected` | a candidate failed the court (durable negative knowledge) |

Both are keyed by the candidate's **source** identity (`sha256` of the `.phor`
source), so "the same candidate" means the same source bytes.

Identity namespaces are kept separate: Gemel Gids are opaque (`residual.<64-hex>`)
and are never reinterpreted or merged with Phorensicos `object_hash`/`chain_hash`
values or FRF receipt ids. The memory record retains the Gemel Gid verbatim.

## 3. Gate F, demonstrated

An already-failed candidate is not new work — the reason is retrieved, not
rediscovered:

```sh
# First campaign: the memory is empty, so exploration runs and records the failure.
$ phorport fuzz strspn --candidate-src examples/jit_port_strspn_xor_lane.phor \
      --gemel .phorport/gemel --frf-fuzz-bin <frf-fuzz> --frf-fuzz-root <repo> --max-time 90
source hash: 252ff565ca232e667be3afbaeeac0ea174deb7f4a91282d8d1d116db832cac8c
findings:    463
memory:      residual.a4d7c659863c50d45b52f2e5767748f02f85808908e5768c87a2eec0c6f91f2d

# Second campaign: the identical source is already recorded as rejected.
$ phorport fuzz strspn --candidate-src examples/jit_port_strspn_xor_lane.phor \
      --gemel .phorport/gemel … 
already known: this candidate's failure is in Gemel memory
  prior rejection residual.a4d7c6…: PORT.LENGTH (PORT.LENGTH -> PORT.LENGTH (minimal 0302232323))

$ phorport history strspn --gemel .phorport/gemel
  counterexample    PORT.LENGTH residual.4baa3c… (… minimal 0302232323)
  candidate-rejected PORT.LENGTH residual.a4d7c6… (… minimal 0302232323)
```

The second campaign does not build or run the target at all: it retrieves the
durable reason and stops.

## 4. What Phase 5 does not yet do

* **Intents and trajectories** are not yet published: only counterexamples and
  rejected candidates are durable boundaries. Gemel checkpoints for campaign
  start/complete are not wired (`MemoryKind::CampaignCompleted` exists but is
  unused).
* **Revision trajectories** across compiler revisions (replaying historically
  sealed candidate sources through a new `phorc` and comparing) are not yet
  recorded.
* The mechanism deliberately stays bounded: bounded, semantically relevant history
  is what a later `CandidateProducer` (Phase 6) will consume — never a dump of the
  whole repository.
