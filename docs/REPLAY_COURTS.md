# Replay Courts and Oracles

## Overview

Forensic parity courts compare native behavior against oracle behavior. Oracles increase kernel trust. Residuals record exact divergence. Kernel trust promotion depends on court verdicts.

## Trust Ladder

```text
unknown (0)
→ observed (1)
→ replayed (2)
→ oracle-compared (3)
→ residual-stable (4)
→ sealed (5)
→ promoted (6)
```

Kernel load policy, driver privilege, service privilege, package activation, and native replacement promotion depend on this ladder.

## Court Architecture

```text
CourtSession =
  object_under_judgment: StoreKey
  oracle: OracleConnection
  replay_engine: ReplayEngine
  evidence_log: Sequence[Evidence]
  comparison_results: Sequence[Comparison]
  verdict: Option[Verdict]
  residuals: Sequence[ResidualRecord]
```

### Court Session Lifecycle

```text
1. Open session: identify object, select oracle(s)
2. Submit evidence: residuals, receipts, provenance
3. Run replay: re-execute object's compilation or execution
4. Compare with oracle: run reference and compare
5. Analyze divergence: classify and document each difference
6. Issue verdict: accept, refine, reject, or request more evidence
7. Promote or deny trust state change
8. Close session: seal verdict and evidence
```

## Oracle Types

### Compiler Oracle

A reference compiler version that produces canonical output:

```text
CompilerOracle:
  - reference compiler binary (sealed, promoted)
  - known good outputs for standard inputs
  - version-matched to the compiler under judgment
```

### Execution Oracle

A reference execution environment:

```text
ExecutionOracle:
  - reference platform (could be sealed native, dialect cage, or external)
  - captures outputs, residuals, and behavior
  - provides canonical behavior for comparison
```

### Dialect Oracle

A reference dialect implementation:

```text
DialectOracle:
  - reference implementation of a dialect (e.g., reference POSIX implementation)
  - used to determine "what does the dialect specification say?"
  - not native law — it is a reference point for comparison
```

### Hardware Oracle

A reference hardware platform:

```text
HardwareOracle:
  - known-good hardware behavior
  - used to verify driver and nucleus behavior
  - typically a simpler, well-understood reference platform
```

## Comparison Types

### Byte-Identical Comparison

```text
ByteIdentical:
  - output A and output B are exactly the same bytes
  - verdict: match
  - trust effect: strong positive
```

### Structurally Equivalent Comparison

```text
StructurallyEquivalent:
  - output A and output B differ in non-semantic bytes (e.g., timestamps, addresses)
  - but semantic structure is identical
  - verdict: equivalent (with residual record of differences)
  - trust effect: positive, with noted non-semantic drift
```

### Behaviorally Equivalent Comparison

```text
BehaviorallyEquivalent:
  - outputs differ in structure but produce same observable behavior
  - example: different instruction scheduling but same program output
  - verdict: behavioral match (requires additional evidence)
  - trust effect: conditional positive
```

### Divergent Comparison

```text
Divergent:
  - outputs differ in semantic content
  - verdict: divergent (requires investigation)
  - trust effect: negative, object cannot be promoted
  - residuals: exact divergence description
```

## Divergence Classification

When divergence is detected, it is classified:

```text
DivergenceClass:
  SemanticDivergence:
    - different computation results
    - different control flow
    - severity: high

  OptimizationDivergence:
    - different instruction selection
    - different register allocation
    - different scheduling
    - severity: medium (if behavior preserved) or high (if behavior affected)

  DialectDivergence:
    - different dialect assumption resolution
    - different ABI choices
    - severity: medium

  NondeterministicDivergence:
    - timing-dependent behavior
    - ASLR-related differences
    - unseeded random variation
    - severity: high (requires investigation)

  AcceptableDivergence:
    - pre-declared acceptable differences
    - documented non-semantic variation
    - severity: low (annotated in residual)
```

## Verdict Types

```text
Verdict:
  Accept:
    - evidence sufficient for trust promotion
    - object moves to next trust state
    - residual chain sealed

  Refine:
    - evidence suggests correct direction but insufficient
    - object remains at current trust state
    - specific additional evidence requested

  Reject:
    - evidence shows behavioral divergence
    - object cannot be promoted
    - object may be demoted if previously promoted

  MoreEvidence:
    - insufficient evidence to make a determination
    - specific evidence types requested
    - object stays at current state
```

## Trust Promotion Flow

```text
// Object starts at Unknown
let obj = store.resolve(key)
assert(obj.trust_state == TrustState::Unknown)

// Step 1: Observe
let observed = observe_execution(obj)
if observed.success {
    promote(obj, TrustState::Observed)
}

// Step 2: Replay
let court = CourtSession::open(obj, oracle)
court.submit_evidence(observed.residuals)
let replay = court.run_replay()
if replay.matches {
    promote(obj, TrustState::Replayed)
}

// Step 3: Oracle Compare
let oracle_result = court.compare_with_oracle()
if oracle_result == ByteIdentical or StructurallyEquivalent {
    promote(obj, TrustState::OracleCompared)
}

// Step 4: Residual Stable (requires multiple replay sessions with consistent residuals)
let stable = check_residual_stability(obj)
if stable {
    promote(obj, TrustState::ResidualStable)
}

// Step 5: Seal
let seal = seal_object(obj)
promote(obj, TrustState::Sealed)

// Step 6: Promote (full trust)
let final_verdict = court.issue_verdict()
if final_verdict == Accept {
    promote(obj, TrustState::Promoted)
}
```

## Multiple Oracle Sessions

An object may be evaluated by multiple oracle sessions:

```text
// Oracle session 1: compiler version 2.0.0
let oracle1 = Oracle::compiler("phc", "2.0.0")
let session1 = CourtSession::open(obj, oracle1)
session1.run_replay()?
session1.compare_with_oracle()?
// verdict: Accept

// Oracle session 2: reference hardware platform
let oracle2 = Oracle::hardware("reference-x86_64-v1")
let session2 = CourtSession::open(obj, oracle2)
session2.run_replay()?
session2.compare_with_oracle()?
// verdict: Accept

// Both oracles agree → strong trust promotion
promote(obj, TrustState::Promoted)
```

## Residual Recording

Every court operation produces residuals:

```text
CourtResidual =
  court_session_id
  operation: "open" | "submit_evidence" | "run_replay" | "compare" | "verdict" | "close"
  input_refs: list of Hash256
  output_refs: list of Hash256
  verdict: Option[VerdictType]
  residual_hash: Hash256
  timestamp
```

## Court Requirements

For an object to be promoted, it must have:

- At least one successful replay session
- At least one successful oracle comparison (if oracle available)
- No unclassified divergences
- All residuals recorded and sealed
- Evidence chain complete and signed

## Example: Full Court Flow

```text
// Object: compiled "editor.phor-spec" version 1.2.3
// Current trust state: Unknown

// Open court with compiler oracle
let oracle = Oracle::compiler("phc", "2.0.0", "reference-x86_64.phor-spec")
let session = CourtSession::open("editor.phor-spec", oracle)

// Submit compilation evidence
session.submit_evidence(compile_residuals)?
session.submit_evidence(source_receipts)?
session.submit_evidence(expansion_receipts)?

// Run replay compilation
let replay = session.run_replay()?
// replay status: full
// replay hash chain matches original

// Compare with oracle
let comparison = session.compare_with_oracle()?
// comparison type: ByteIdentical
// no divergence

// Issue verdict
let verdict = session.issue_verdict()?
// verdict: Accept
// promoted to: Promoted

// Close session
session.close()?

// Object is now promoted
let obj = store.resolve("editor.phor-spec-key")
assert(obj.trust_state == TrustState::Promoted)
```
