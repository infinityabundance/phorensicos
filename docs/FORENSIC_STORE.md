# Forensic Store Specification

## Overview

The Forensic Store is the immutable, content-addressed, semantically indexed storage substrate of Phorensic OS. It is inspired by the best ideas of content-addressed package stores but reinterpreted through forensic residual primacy.

## Store Object Model

Each store object is:

```text
StoreObject =
  content bytes
  source snapshot hash
  dialect profile
  build recipe
  expansion receipts
  compiler pass receipts
  replay checkpoints
  residual graph
  trust state
  signatures
```

### Store Key

The store key is a hash of the complete semantic context:

```text
StoreKey = hash(
  source,
  dependencies,
  dialect profile,
  compiler version,
  target arch,
  effect profile,
  expansion receipts,
  oracle identities,
  court versions,
  build environment capsule
)
```

This means the same source compiled with different dialect profiles or compiler versions produces different store keys, allowing safe coexistence of multiple versions and configurations.

## Store Operations

### Store Object

```text
store_object(content, metadata) -> StoreKey
```

Validates that:
- content hash matches declared hash
- source snapshot is present (if applicable)
- all required receipts are present
- signatures are valid
- trust state is consistent with evidence

### Resolve Object

```text
resolve_object(key) -> Result[StoreObject, StoreError]
```

### Verify Object

```text
verify_object(key) -> VerificationReport
```

Verification checks:
- content integrity (hash)
- signature validity
- receipt chain integrity
- trust state consistency
- source↔binary correspondence (if applicable)

### Get Provenance

```text
get_provenance(key) -> Provenance
```

### Get Residual History

```text
get_residual_history(key) -> Sequence[ResidualRecord]
```

### Get Trust State

```text
get_trust_state(key) -> TrustState
```

## Generations

A generation is a complete, atomically-activated system state:

```text
Generation =
  kernel container key
  trusted nucleus image key
  Phorensic compiler image key
  runtime service keys
  semantic store root key
  trust roots
  court/verifier version references
  package/container graph
  activation timestamp
  creator identity
  generation number
  parent generation number
```

### Activation

Activation is all-or-nothing:

```text
activate_generation(gen) -> Result[(), ActivationError]
```

If activation fails (integrity check failure, capability violation, missing dependency), the previous generation remains bootable and the failure is recorded as a residual.

### Rollback

Rollback restores the system to a previous generation:

```text
rollback_to_generation(gen) -> Result[(), RollbackError]
```

Rollback preserves:
- executable containers
- trust states
- residual envelopes
- court versions
- replay checkpoints
- package graph
- native clean-room promoted slices

Rollback answers:
- Why was this generation trusted? → get_provenance(gen.store_root)
- Which receipts sealed it? → get_receipt_chain(gen.store_root)
- Which residuals changed in the failed generation? → compare_residuals(failed_gen.store_root, previous_gen.store_root)

## Evidence-Aware Garbage Collection

The garbage collector distinguishes between:

- **Unreachable object** — no remaining references
- **Legally/audit-retained receipt** — retained for compliance or audit requirements
- **Active trust root** — currently referenced by an active generation
- **Rollback generation** — a generation that is a rollback target
- **Court evidence** — residual records still referenced by a promoted object
- **Residual history** — drift analysis references

GC policy is configurable:

```text
GCPolicy =
  retain_receipts: Duration or bool
  retain_rollback_generations: u32
  retain_court_evidence: bool
  min_trust_state_for_gc_exemption: TrustState
  audit_retention_list: List[StoreKey]
```

## Profiles

Users, services, and devices can have profiles:

```text
Profile =
  capability_grants
  package_generation
  residual_trust_threshold: TrustState
  allowed_dialects: List[DialectProfile]
  allowed_effects: List[Effect]
  replay_court_policy: CourtPolicy
```

Switching profiles is atomic and reversible.

## Reproducibility

The store guarantees:

```text
same inputs
+ same dialect profile
+ same courts
+ same residual policy
→ same sealed semantic container
→ StoreKey(same evidence set)
```

This extends Nix-style reproducibility from binary-only to binary + evidence + trust state.

## Store Layout

```text
/store/
  /objects/
    {key_prefix}/
      {key}.phor-object       # the stored object
      {key}.phor-object.sig   # signatures
      {key}.phor-object.meta  # metadata (provenance, trust state, etc.)
  /generations/
    {gen_number}/
      gen.phor-generation      # generation descriptor
      gen.phor-generation.sig  # generation signatures
  /profiles/
    {profile_name}/
      profile.phor-profile     # profile descriptor
      profile.phor-profile.sig # profile signatures
  /residuals/
    {object_key_prefix}/
      {timestamp}_{event_id}.phor-residual
```

## Access Control

Store access is capability-gated:

```text
StoreCapability:
  - store_read: read any object
  - store_write: store new objects
  - store_delete: mark objects for GC (not immediate deletion)
  - store_audit: read residual history and provenance
  - store_verify: run verification on objects
  - store_configure: change GC policy and store configuration
```

## Example: Store Operations

```text
// Store a package
let pkg = PackageContainer { ... }
let key = store.store_object(pkg.content, pkg.metadata)
// key = hash(source, deps, dialect, compiler, ...)

// Resolve and verify
let obj = store.resolve_object(key)?
let report = store.verify_object(key)?
assert(report.integrity == Pass)
assert(report.signatures == Valid)
assert(report.source_binary_chain == Verified)

// Get residual history
let residuals = store.get_residual_history(key)
for residual in residuals {
    process_residual(residual)
}
```
