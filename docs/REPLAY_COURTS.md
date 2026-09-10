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

## JIT-Porting Court

The first concrete replay court is the **JIT-Porting Court**. It observes a
foreign API surface as a black box, seals the observed behavior as oracle
traces, replays a clean-room native candidate against them, and promotes the
candidate only on exact, evidence-backed equivalence.

This is **API-surface** porting (byte-in/byte-out functions such as `toupper`),
not arbitrary binary translation.

### Flow

```text
foreign behavior → dialect cage → oracle traces → behavior signature
→ native candidate → replay court → comparison → promotion → sealed package
```

### First target: libc `toupper`

```text
target id: libc:toupper:c-locale:u8:v1
dialect:   libc   symbol: toupper   version: host-observed-v1
locale:    C      (recorded in every trace as locale_contract)
domain:    exhaustive 0x00..=0xff (256 cases, in order)
```

### Second target: libc `memcmp`

```text
target id: libc:memcmp:c-locale:sign:v1
dialect:   libc   symbol: memcmp   version: host-observed-v1
locale:    C
contract:  sign of the return value (-1 | 0 | 1), unsigned comparison, n-bounded
corpus:    312 bounded deterministic cases
```

The `memcmp` observable is the **sign** of the return value — the exact integer
is not part of the C contract. Its corpus deliberately forces memory, length and
ordering: lengths `0..=8`; five patterns (zero/ones/ascending/descending/
alternating); every first-mismatch position with both orderings; the `n`-boundary
around a mismatch (`n = j` excludes it, `n = j+1` includes it); and the unsigned
edge bytes `00/01/7f/80/fe/ff`. Every case satisfies `n <= min(len(a), len(b))`,
so no observation reads past a buffer. Arguments are framed in the trace as
`a_hex:b_hex:n_hex` (little-endian `n`).

### Target identity is qualified

A target id is `dialect:symbol:locale:contract:version`, so a future
locale-aware `toupper` (or a raw-value `memcmp`) is a *different* target, never a
silent redefinition of an existing one.

### The dialect cage and the candidates

The cage calls the foreign functions through a single narrow FFI shim and records
input/output/locale/status/effects — it never reads or copies foreign source. The
native candidates are clean-room `phor_toupper` (ASCII `a`..`z` fold) and
`phor_memcmp` (unsigned, `n`-bounded, sign result). Unknown target ids return
`CandidateError::UnsupportedTarget` and malformed arguments return
`CandidateError::MalformedArgs`; there is no identity fallback, so an unsupported
candidate can never pass by accident.

### Mapping to court concepts

| Court concept | Artifact |
|---------------|----------|
| Observation / oracle traces | `oracle_traces.json` |
| Behavior signature | `behavior_signature.json` |
| Candidate residual | `candidate_signature.json` |
| Comparison / replay residual | `replay_verdict.json` |
| Promotion residual | `promotion_receipt.json` |
| Sealed package | `sealed_package.json` |

Each artifact carries a `residual_hash` (SHA-256 over its canonical encoding).
The combined oracle hash binds to the full canonical trace content, so any
mutation of a covered field changes the behavior signature.

### Verdict and promotion rules

- The verdict derives from exact case comparisons, never from receipt counts.
- An empty case set is `inconclusive`; any mismatch is `inconsistent`; the court
  fails closed on both.
- Promotion to `sealed` requires: a non-empty full replay, zero failures, a
  consistent replay verdict, the oracle hash, the candidate behavior hash, the
  bound candidate source hash, the sealed package + replay residual written, and
  a **consistent sealed-object execution verdict**. Promotion also requires the
  `PORTING` capability.

### Compiled candidate authority

The promoted implementation is the **compiled** clean-room candidate, not just a
Rust mirror of it:

1. the court invokes `phorc` on the target's `.phor` source;
2. it hashes the emitted ELF64 object (`candidate.o`) and its receipt file
   (`candidate.receipts.json`);
3. it records the compiler provenance (`phorc 0.1.0`);
4. the candidate signature, promotion receipt and sealed package bind
   `candidate_source_hash`, `candidate_object_hash`, `candidate_receipt_hash`
   and `compiler_version`;
5. promotion requires all three hashes (a missing one fails closed);
6. the verifier independently recompiles the source and confirms the object and
   receipt hashes match the committed seal.

Compilation runs from the workspace root with the repo-relative source path, so
the ELF `FILE` symbol is environment-independent and the object hash is stable
across hosts and containers. (This required making `phorc` register allocation
deterministic.)

### Sealed object execution court

Binding the compiled object is not the same as running it. The execution court
(`phost::porting::exec`) closes that loop:

1. it reads the sealed object and verifies its SHA-256 against
   `candidate_object_hash` **before touching it** (a mismatch fails closed);
2. it parses the ELF64 sections and symbol table and locates the ABI entry
   symbol `_phor_<abi_symbol>` (e.g. `_phor_phor_toupper`);
3. it requires the entry to be a `.text` `STT_FUNC` with no relocation landing
   inside its byte range — i.e. a self-contained leaf with no calls and no
   external symbols;
4. it maps `.text` read-only/executable and calls the function through an
   explicit SysV integer ABI harness;
5. it replays the **same** oracle corpus through the compiled object and compares
   exact output bytes;
6. it writes `execution_verdict.json` (`cases_run/passed/failed`, `object_hash`,
   `oracle_hash`, `execution_hash`, `verdict`, `mismatches`).

Promotion to `sealed` now requires **both** courts to be consistent: the replay
court (Rust mirror vs oracle) and the execution court (compiled object vs oracle).
A refusal to load or execute is a hard error (`PortError::Execution`), so no seal
is produced without a verified execution.

Scope is deliberately narrow: leaf, pure, relocation-free integer functions.
There is no dynamic linker, no relocation patching, no heap and no syscalls. This
proves the promoted artifact runs; it is not a general execution engine.

Executed ABI (SysV AMD64):

| Entry symbol | Signature | Result |
|--------------|-----------|--------|
| `_phor_phor_toupper` | `(u64 byte) -> u64` | folded byte in the low 8 bits |
| `_phor_phor_memcmp_sign` | `(u64 wa, u64 wb, u64 n) -> u64` | `i64` sign (`-1`/`0`/`1`); buffers packed big-endian into the word |

### Seal contents

The sealed package binds the qualified target id, the locale contract, the
candidate's **behavior** hash (its outputs over the case domain), the compiled
artifacts: the clean-room **source** hash, the ELF64 **object** hash, the
**receipt** hash, and the **compiler version** — and now also the execution
residual: the **ABI symbol**, the **executed ELF symbol**, the
**execution hash**, and the **execution verdict**. The sealed store entry points
at the compiled object (`candidate.o`), which is the authoritative implementation,
and that same object is the one loaded and executed.

### Capability gating

`PORTING` (capability bit 16) gates both ends:

- observation of foreign behavior (no ambient authority to observe), and
- promotion of a native candidate (no ambient authority to seal).

Sealed store lookups are gated: without `PORTING`, sealed port entries are not
revealed.

### Where it lives

```text
phost/src/porting/                target, dialect_cage, oracle_trace,
                                  behavior_signature, candidate, replay_court,
                                  promotion, evidence, compiled, exec
examples/jit_port_toupper.phor    toupper native candidate, in Phorensic
examples/jit_port_memcmp.phor     memcmp native candidate, in Phorensic
phost/evidence/porting/toupper/   toupper evidence set (256 cases) + candidate.o
phost/evidence/porting/memcmp/    memcmp evidence set (312 cases) + candidate.o
verify_jit_porting_court.sh       court verifier (--target toupper|memcmp)
```

The compiled `candidate.o` / `candidate.receipts.json` are regenerated (and are
not committed); only their hashes are sealed. The promoted artifact is the
compiled object, not the Rust mirror — and that object is executed, not merely
hashed.

Reproduce:

```sh
cargo run -p phost -- port promote toupper
cargo run -p phost -- port promote memcmp
./verify_jit_porting_court.sh --target toupper                 # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
```

`--check-committed` writes only to a temp dir and compares against the checked-in
evidence without touching it — reviewer-grade proof that the committed seal
matches a fresh run.
