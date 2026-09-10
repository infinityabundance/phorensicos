# Phorensic OS Kernel Architecture

## Overview

Phorensic OS is an evidence-first, capability-based operating system where the kernel stores, indexes, streams, and verifies residual objects. The kernel is not file-first or process-first. It is evidence-first.

## Core Kernel Primitives

The kernel provides these fundamental object types:

```text
Capability       — typed, affine, generation-tagged authority
Object           — kernel-managed entity with identity and metadata
Session          — capability-gated interaction context
Port             — typed message channel
StoreNode        — immutable content-addressed object in the forensic store
ResidualRecord   — first-class kernel metadata recording evidence
Generation       — atomic system state snapshot
CourtSession     — replay verification context
DialectCage      — foreign code isolation boundary
```

## Object Model

Every kernel object carries:

```text
Object =
  data
  identity
  capabilities
  provenance
  residual history
  replay state
  trust state
  residual fingerprints
```

### Identity

Each object has a globally unique identity derived from its creation context:

```text
Identity = hash(creator, creation_time, creation_epoch, nonce)
```

### Capabilities

Capabilities are typed, affine, generation-tagged, and explicitly granted. There is no ambient authority.

```text
Capability[Type, Generation] =
  object_id: Identity
  type_tag: TypeTag
  generation: u64
  rights: RightsSet
```

Rights are individually grantable:

```text
RightsSet = bitfield {
    read: 1,
    write: 1,
    execute: 1,
    delete: 1,
    transfer: 1,
    delegate: 1,
    observe: 1,     // read residual history
    audit: 1,       // generate residual records
    promote: 1,     // change trust state
}
```

### Provenance

Provenance records the causal history:

```text
Provenance =
  creator_identity
  creation_receipt
  parent_objects
  compilation_receipt (if applicable)
  porting_receipt (if applicable)
  court_verdicts
```

### Residual History

Residual history is an append-only sequence of residual records:

```text
ResidualRecord =
  timestamp
  event_type
  event_data_hash
  cause_identity
  prior_state_hash
  new_state_hash
  delta_description
```

### Trust State

Every object has a trust state on the formal trust ladder:

```text
TrustState = enum {
    Unknown = 0,
    Observed = 1,
    Replayed = 2,
    OracleCompared = 3,
    ResidualStable = 4,
    Sealed = 5,
    Promoted = 6,
}
```

## Kernel Services

### Capability Manager

The capability manager tracks all live capabilities, enforces affine ownership, validates generation tags, and mediates all authority grants.

```text
CapabilityManager:
  - create_capability(object, rights) -> Capability
  - transfer_capability(source, target) -> Result[(), Error]
  - revoke_generation(generation) -> Result[(), Error]
  - check_capability(cap, required_rights) -> Result[(), Error]
  - enumerate_object_capabilities(object) -> List[Capability]
```

### Session Manager

Sessions are capability-gated interaction contexts. Every user/system interaction occurs within a session.

```text
SessionManager:
  - create_session(initial_capabilities) -> Session
  - close_session(session) -> Result[(), Error]
  - grant_session_capability(session, cap) -> Result[(), Error]
  - revoke_session_capability(session, cap) -> Result[(), Error]
```

### Port Manager

Ports are typed message channels supporting structured communication.

```text
PortManager:
  - create_port(type, session) -> Port
  - send(port, message) -> Result[(), Error]
  - receive(port) -> Result[Message, Error]
  - bind_port_to_service(port, service) -> Result[(), Error]
```

### Store Manager

The forensic store is the kernel's persistent storage layer — immutable, content-addressed, and semantically indexed.

```text
StoreManager:
  - store_object(data, metadata) -> StoreNode
  - resolve_node(hash) -> Result[StoreNode, Error]
  - verify_node(hash) -> Result[VerificationReport, Error]
  - get_provenance(hash) -> Provenance
  - get_residual_history(hash) -> Sequence[ResidualRecord]
  - get_trust_state(hash) -> TrustState
```

### Generation Manager

Generations are atomic system state snapshots.

```text
GenerationManager:
  - create_generation(store_root, trust_roots, package_graph) -> Generation
  - activate_generation(gen) -> Result[(), Error]
  - rollback_to_generation(gen) -> Result[(), Error]
  - list_generations() -> Sequence[Generation]
  - describe_generation(gen) -> GenerationReport
```

### Court Session Manager

Court sessions manage replay verification and trust promotion.

```text
CourtSessionManager:
  - open_court_session(object, oracle) -> CourtSession
  - submit_evidence(session, evidence) -> Result[(), Error]
  - run_replay(session) -> Result[ReplayReport, Error]
  - compare_with_oracle(session) -> Result[ComparisonReport, Error]
  - promote_trust(session) -> Result[TrustState, Error]
  - close_court_session(session) -> Verdict
```

### Dialect Cage Manager

Dialect cages isolate foreign code and capture residuals.

```text
DialectCageManager:
  - create_cage(dialect_profile) -> DialectCage
  - load_into_cage(cage, code) -> Result[(), Error]
  - execute_in_cage(cage, entry_point, args) -> Result[CageResult, Error]
  - extract_residuals(cage) -> Sequence[ResidualRecord]
  - close_cage(cage) -> CageReport
```

## Scheduler

The scheduler is capability-aware and effect-aware:

- Threads are capabilities with explicit effect declarations.
- Scheduling accounts for declared effects (io, deterministic, non_deterministic, realtime).
- The scheduler produces residual records for thread scheduling decisions.
- Priority inheritance is explicit and capability-gated.

```text
Scheduler:
  - spawn(entry, capabilities, effects) -> ThreadId
  - yield() -> Result[(), Error]
  - set_priority(thread, priority) -> Result[(), Error]
  - get_schedule_residuals(thread) -> Sequence[ResidualRecord]
```

## Inter-Process Communication (IPC)

IPC is through typed ports with capability gating:

- Each port has a declared message type.
- Both sender and receiver must hold appropriate capabilities.
- Messages are residualized — every IPC event produces a residual record.
- Ports can be bound to services for request/response patterns.

## Error Model

All kernel errors are typed and carry stable diagnostic codes:

```text
KernelError = enum {
    CapabilityNotFound = 0x01,
    InsufficientRights = 0x02,
    StaleGeneration = 0x03,
    ObjectNotFound = 0x04,
    SessionClosed = 0x05,
    PortFull = 0x06,
    PortEmpty = 0x07,
    TrustStateInsufficient = 0x08,
    DialectViolation = 0x09,
    ProvenanceMismatch = 0x0A,
    ResidualIntegrityFailure = 0x0B,
    CourtVerdictNegative = 0x0C,
    StoreHashMismatch = 0x0D,
    GenerationActivationFailed = 0x0E,
    RollbackRejected = 0x0F,
}
```

## Kernel Answerable Questions

The kernel can directly answer:

- Why does this object exist? → `get_provenance(hash)`
- What source produced it? → `resolve_provenance_source(provenance)`
- What generated this byte? → `trace_byte(hash, offset)`
- Which compiler pass emitted it? → `trace_compilation_pass(hash, region)`
- Which dialect assumption caused it? → `trace_dialect_assumption(hash, residual)`
- Which residual changed? → `compare_residuals(hash_old, hash_new)`
- Can this object be replayed? → `can_replay(hash)`
- Is this object promoted or provisional? → `get_trust_state(hash)`

## Security Model

Security decisions depend on behavioral evidence:

- A driver starts at TrustState::Unknown and gains privilege only through court sessions.
- Service promotion requires replay and court evidence.
- Drift outside a sealed residual envelope triggers access denial.
- Supply-chain verification includes source, binary, receipt, and signature verification.
- The kernel explains why access or promotion was refused via residual records.
- Binary trust is behavioral, not only signer-based.

## Example: Kernel Session Flow

```text
1. User opens session → SessionManager.create_session(root_capabilities)
2. User requests object → StoreManager.resolve_node(hash)
3. Kernel checks capabilities → CapabilityManager.check_capability(user_cap, read)
4. Kernel checks trust state → get_trust_state(hash) >= TrustState::Sealed
5. Kernel returns object → Object { data, capabilities, provenance, ... }
6. User performs operation → operation produces ResidualRecord
7. Session closes → SessionManager.close_session(session)
```

## JIT-Porting Court

The first implementation of the Dialect Cage Manager + Court Session Manager is
the **JIT-Porting Court**. It ports behavior at the API boundary:

```text
foreign behavior → dialect cage → oracle traces → behavior signature
→ native candidate → replay court → comparison → promotion → sealed package
→ execution court (load and call the sealed object)
→ dispatch court (runtime serves calls from the sealed object) → runtime prefers native
```

- **Dialect cage** — observes a foreign API surface as a black box. Three targets
  so far: libc `toupper` (exhaustive `0x00..=0xff`), libc `memcmp` (a bounded
  deterministic corpus forcing length, buffers and ordering) and libc `memchr`
  (a bounded deterministic corpus forcing search: first-match index, absent
  needle, repeated needles, the `n`-boundary, and an exhaustive 256-value needle
  sweep). `memchr`'s pointer result is normalized to the index, which is the
  portable part of its contract. No foreign source is read or copied.
- **Court session** — replays a clean-room native candidate against sealed oracle
  traces and compares exact output/status/effects. The verdict derives from case
  comparisons, not receipt counts, and fails closed.
- **Execution court** — loads the sealed ELF64 object, verifies its hash against
  the seal, locates the ABI entry symbol, rejects any entry with relocations or
  external symbols, maps it executable and replays the same corpus through the
  compiled code. This is what makes the promoted artifact *run*, not just hash.
- **Dispatch court** — the runtime path: a call site looks the target up in the
  capability-gated sealed store and, if a sealed entry is present, verifies the
  object, maps the entry function once and calls it. With no usable sealed
  artifact the call falls back to the foreign implementation; a *broken seal* is
  terminal and never falls back. The court requires every case to be served
  natively.
- **Promotion** — advances the candidate to `sealed` only when the replay court,
  the execution court *and* the dispatch court all match exactly, with a complete
  evidence set. Gated by the `PORTING` capability.
- **Runtime preference** — the sealed package (e.g.
  `native:libc:memcmp:c-locale:sign:v1`) binds the compiled ELF64 candidate
  object; that object is the artifact the runtime prefers over the foreign
  implementation, and dispatch is the call site that honours it.

This is API-surface porting, not arbitrary binary translation. The execution
court runs only leaf, pure, relocation-free integer functions (no dynamic linker,
no relocation patching, no heap, no syscalls). Eager JIT of arbitrary foreign
binaries is a later phase.

See `docs/REPLAY_COURTS.md` (JIT-Porting Court) and `phost/src/porting/`.
