# Threat and failure model — autonomous porting (Phase 7)

**Status: Phase 7 core implemented.** This document is normative. For every
failure class it states what detects it, what evidence remains, and whether the
campaign may continue.

The governing rule: **automation may propose everything; automation may approve
nothing.** A candidate producer, an FRF-Fuzz finding, a Gemel precedent, an LLM
verdict or a mutation score can never reach `Sealed`.

| Class | Detected by | Evidence retained | Campaign |
|---|---|---|---|
| Incorrect candidate producer | design/discovery courts; multi-oracle; held-out qualification | rejected revision + counterexample + negative knowledge | revise |
| Malicious candidate source | source constraints; ELF verification in `SealedObjectHandle::load` (hash, format, arch, entry, no relocations, load bounds) | load failure recorded deterministically | revise / refuse |
| Candidate crash / `SIGILL` / abort | contained worker (separate process) | `CandidateFailure::Crashed`, worker restarted | continue |
| Candidate hang | contained worker wall-clock timeout + `RLIMIT_CPU` | `CandidateFailure::Timeout` | continue |
| Resource exhaustion | `RLIMIT_AS`, `RLIMIT_CORE=0`, `RLIMIT_NOFILE` | load/exec failure | continue |
| Compiler miscompile | execution court vs oracle on the uninstrumented object | execution mismatch, promotion refused | refuse |
| Compiler version drift | build provenance (compiler identity bound in O4) | new candidate identity | new campaign |
| Oracle binary drift | cross-implementation court re-hashes the probe; FRF re-hashes the admitted authority | refusal (admission is once) | refuse |
| Undefined / out-of-contract oracle input | `validate_case` precondition gate before the oracle | invalid case, never an oracle observation | drop |
| Normalization bug | projections are sealed in the `PortSpec`; a projection change changes the `PortSpecId` | new spec identity | new campaign |
| Court blindness | challenge court (`MutationProfile`); equivalent mutants excluded, undetermined fails closed | `ChallengeReport` | refuse seal (O8) |
| Equivalent mutant | equivalence note under the declared domain | recorded as equivalent | not counted |
| Fuzzing false lead | counterexample minimized and replayed through the same court on the uninstrumented object | original + reduced cases with lineage | continue |
| Minimizer destroys the residual | the minimizer proposes, the comparison court decides | refused reduction retained | continue |
| Stale Gemel precedent | memory is evidence-bearing, never authority; a precedent never bypasses current verification | precedent reference | continue |
| FRF refusal | FRF returns `Failed` with a note; a parity run is preserved | run id + receipt + note | no receipt → O9 refuses |
| Qualification leakage | synthesis-workspace audit; error messages name the obligation without echoing evidence | `IsolationReport.leaks` | refuse seal (O7) |
| Instrumented artifact reaching promotion | `ExecutionEvidence.uninstrumented` must hold; search-only types are distinct | refusal (O10) | refuse |
| Store artifact substitution | object hash re-verified before mapping | `ObjectHashMismatch` | refuse |
| Composition cycle / dependency substitution | `CompositionIR` validation (acyclic, typed) | refusal | refuse |
| Non-deterministic ordering | canonical encodings; no wall clock in any identity | deterministic identities | — |

## Fail-closed rules

Unknown schema version, malformed identity, missing required oracle, broken seal,
corrupted store entry, mismatched artifact, stale qualification premise, missing
challenge requirement, failed FRF verification or a qualification leak are all
terminal for the affected obligation. A high-assurance campaign never degrades
silently to a weaker policy.

## A refusal is evidence

A refused seal names the first missing/inconsistent obligation. A failed
candidate proposal is durable negative knowledge; a parity FRF run is preserved
as evidence of non-reproduction. The interesting failure is not deleted merely
because the campaign cannot seal.
