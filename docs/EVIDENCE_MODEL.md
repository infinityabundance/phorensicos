# The evidence model and `AUTONOMOUS-SEAL/v1` (Phase 7)

**Status: Phase 7 core implemented.** This document is normative for the
identity namespaces, the evidence closure and the autonomous promotion
obligations.

## 1. Identity namespaces never collapse

A Phorensicos SHA-256 object hash, an FRF `run-`/`receipt-`/claim id, an FRF-Fuzz
`ContentId` (BLAKE3) and a Gemel `Gid` are four distinct identity systems.
`phost/src/porting/ident.rs` makes each an opaque newtype whose `canonical()`
token carries a namespace tag, so two identical payloads from different systems
can neither be swapped in code nor collide in a hash. A cross-system record
carries a **reference**, never a reinterpretation.

The wrappers: `PortSpecId`, `CampaignManifestId`, `CandidateSourceHash`,
`CandidateObjectHash`, `CompilerReceiptHash`, `QualificationReceiptId`,
`ChallengeReceiptId`, `OracleWitnessId`, `EvidenceClosureId`,
`PromotionReceiptId`, `StoreGenerationId`, `CompositionIrId`,
`DependencyBindingHash`, `ArtifactHash`, `FrfRunId`, `FrfReceiptId`,
`FrfClaimId`, `FrfFuzzContentId`, `GemelGid`.

## 2. The evidence closure

`EvidenceClosure` is the explicit, content-addressed set of evidence a seal
binds. Members are `(role, namespaced identity)` edges, canonicalized by sorting
so the identity is order-independent. The encoding is length-framed and
domain-separated:

```text
EvidenceClosureId = SHA-256("PHOR/EVIDENCE-CLOSURE/v1\0" || canonical_bytes)
```

An empty closure, an unknown schema version, or a malformed member fails closed.
A seal whose closure does not bind the candidate object is refused.

## 3. `AUTONOMOUS-SEAL/v1`

`phost/src/porting/autonomous_seal.rs` defines the profile. A candidate receives
it only when every obligation is present **and** cross-consistent:

```text
O1  PortSpec identity valid
O2  all oracle cases passed typed precondition validation
O3  required oracle witnesses bound (>= policy; every witness complete)
O4  candidate source → compiler → object provenance bound (rebuild recorded)
O5  design replay consistent (cases run, none failed)
O6  all admitted discovery regressions consistent (none outstanding)
O7  held-out qualification consistent and isolated
O8  court sensitivity demonstrated for the required mutation families
O9  FRF receipts bound (the required receipts are emitted FRF receipts)
O10 ordinary uninstrumented candidate execution consistent
O11 dispatch court serves the native artifact with zero fallbacks/broken seals
O12 evidence closure complete and binds the candidate object
O13 store-generation parent/current relation valid
```

Cross-checks that make the profile non-vacuous: O10 and O11 must bind the same
object O4 built; O11 must have served every case natively; O8 must have detected
every declared family that is not excluded as equivalent, with no undetermined
outcome; O12's closure must contain the candidate object's canonical token.

The verification is a pure, deterministic function of the evidence references —
no clock, no I/O. The receipt records which obligation each reference satisfied.

## 4. Seal profiles and history

`SealProfile::LegacyV1` is the pre-autonomous seal made by `promotion.rs`; its
evidence is immutable and is never re-described as having passed tests that did
not exist when it was made. `SealProfile::AutonomousV1` is this profile. The two
coexist; the new system may regenerate stronger evidence from the same source but
never rewrites history.

## 5. FRF is a separate court

An FRF receipt and a Phorensicos promotion receipt are not the same object and
are never called the same thing. FRF supplies durable external evidence; it does
not replace Phorensicos promotion, and Phorensicos does not reimplement FRF's
object model. `phorport/src/frf.rs` runs FRF's own court/receipt/claim commands
and retains the ids verbatim.

**Build-bound identities (honest limit).** FRF binds the *runner executable hash*
into its run and receipt ids. The promotion receipt therefore reproduces for a
fixed coordinator build, and across builds it differs only in those ids and the
identities that reference them. `verify_autonomous_seal.sh` therefore checks the
promotion receipt's **structure** (profile, the thirteen obligations, their
roles, the candidate-object binding, the FRF receipts present) and compares the
build-independent **qualification receipt byte-for-byte**. This is recorded as an
observed property, not hidden.

## 6. Not claimed

Bound evidence is not a proof of equivalence. `AUTONOMOUS-SEAL/v1` is a bounded
statement: every declared obligation is present and cross-consistent under the
declared policies and budgets.
