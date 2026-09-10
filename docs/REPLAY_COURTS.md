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

This is **API-surface** porting (byte-in/byte-out and small buffer functions such
as `toupper`, `memcmp`, `memchr`, `strlen` and `strrchr`), not arbitrary binary
translation.

### Flow

```text
foreign behavior → dialect cage → oracle traces → behavior signature
→ native candidate → replay court → comparison → promotion → sealed package
→ execution court (load and call the sealed object)
→ dispatch court (runtime serves calls from the sealed object) → runtime prefers native
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

### Third target: libc `memchr`

```text
target id: libc:memchr:c-locale:index:v1
dialect:   libc   symbol: memchr   version: host-observed-v1
locale:    C
contract:  index of the first matching byte, or -1 when absent (unsigned, n-bounded)
corpus:    482 bounded deterministic cases
```

`memchr` returns a *pointer* to the first match, or NULL. A pointer value is not
portable behavior — it depends on the caller's buffer address — so the observable
is normalized to the **index** of the first match (`0..n-1`), or `-1` when the
byte is absent. That is the portable part of the C contract (`result - s`) and
the value a native caller actually needs. Its corpus forces search semantics:
lengths `0..=8`; the first match at every index (on distinct-byte patterns);
repeated needles (first occurrence wins); an absent needle; the `n`-boundary
around a match (`n = j` excludes it, `n = j+1` includes it); the unsigned edge
bytes `00/01/7f/80/fe/ff`; and an exhaustive sweep of all 256 needle values.
Arguments are framed in the trace as `hay_hex:needle_hex:n_hex` (little-endian
`n`).

### Fourth target: libc `strlen`

```text
target id: libc:strlen:c-locale:u64:v1
dialect:   libc   symbol: strlen   version: host-observed-v1
locale:    C
contract:  length of a NUL-terminated string (index of the first NUL byte)
corpus:    308 bounded deterministic cases
```

`strlen` takes no length argument, so the observable is the **length** — the
index of the first NUL byte — which is the portable part of its contract. The ABI
carries `n` only as a *precondition bound*: the terminator lies within the first
`n` bytes, which is what makes the buffer packable into one word and keeps the
foreign observation from reading past the caller's window. A case whose terminator
falls outside the bound is out of contract, and the native candidate fails closed
by returning `n`. Its corpus forces NUL-termination semantics: the complete
`(k, n)` grid of terminator index `k` and scan bound `n` with `0 <= k < n <= 8`;
non-NUL filler bytes (including `0x7f`/`0x80`) at every prefix position; tails
after the terminator that are themselves NUL so the *first* NUL must win; buffers
longer than the bound (bytes past `n` are ignored); and an exhaustive sweep of all
256 byte values proving that only `0x00` terminates. Arguments are framed in the
trace as `buf_hex:n_hex` (little-endian `n`).

### Fifth target: libc `strrchr`

```text
target id: libc:strrchr:c-locale:index:v1
dialect:   libc   symbol: strrchr   version: host-observed-v1
locale:    C
contract:  index of the last occurrence in the C string, or -1 when absent
corpus:    336 bounded deterministic cases
```

`strrchr` is the mirror image of `memchr`: `memchr` finds the *first* match in a
*bounded window*, while `strrchr` finds the *last* match in the *string*. It
returns a pointer, so the observable is normalized to the **index**
(`result - s`); the search domain ends at (and includes) the first NUL, so bytes
after the terminator are never matched and a needle of `0` yields the terminator
index (the string's length). `n` is the ABI precondition bound: the terminator
lies within the first `n` bytes, which the execution harness enforces, so the
packed word encodes the whole string. Its corpus forces last-match semantics:
unique occurrences at every in-string index; repeated occurrences so the *last*
wins; the empty string; needles that occur only after the terminator (never a
match); needles that occur both before and after it (the in-string occurrence
wins); the unsigned edge bytes with copies of `0x7f`/`0x80` after the terminator;
and an exhaustive sweep of all 256 needle values against a haystack whose tail
repeats bytes from the string. Arguments are framed in the trace as
`buf_hex:needle_hex:n_hex` (little-endian `n`).

### Target identity is qualified

A target id is `dialect:symbol:locale:contract:version`, so a future
locale-aware `toupper` (or a raw-value `memcmp`) is a *different* target, never a
silent redefinition of an existing one.

### The dialect cage and the candidates

The cage calls the foreign functions through a single narrow FFI shim and records
input/output/locale/status/effects — it never reads or copies foreign source. The
native candidates are clean-room `phor_toupper` (ASCII `a`..`z` fold),
`phor_memcmp` (unsigned, `n`-bounded, sign result), `phor_memchr` (unsigned,
`n`-bounded, first-match index), `phor_strlen` (first-NUL length, `n`-bounded
and fail-closed at `n`) and `phor_strrchr` (last in-string match index, NUL
needle → length). Unknown target ids return
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
  bound candidate source hash, the sealed package + replay residual written, a
  **consistent sealed-object execution verdict**, and a **consistent dispatch
  verdict with zero foreign fallbacks and zero broken seals**. Promotion also
  requires the `PORTING` capability.

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
| `_phor_phor_memchr_index` | `(u64 wh, u64 needle, u64 n) -> u64` | `i64` index or `-1`; haystack packed little-endian into the word |
| `_phor_phor_strlen_len` | `(u64 w, u64 n) -> u64` | `u64` length (first NUL index), `n` when out of contract; buffer packed little-endian |
| `_phor_phor_strrchr_index` | `(u64 w, u64 needle) -> u64` | `i64` index of the last in-string match or `-1`; buffer packed little-endian and zero-extended |

### Sealed native dispatch court

Executing the object proves the artifact is *correct*. Dispatch proves the
runtime **prefers** it. `phost::porting::dispatch` is the call-site path:

1. look the target up in the capability-gated sealed store
   (`SealedPortIndex::lookup_gated`);
2. with no usable sealed artifact, return a **foreign fallback** (the caller uses
   the foreign implementation);
3. with a **sealed** entry, verify the object's SHA-256 against the seal, map the
   entry function once, and call it;
4. `NativeDispatcher` caches the mapped object, so steady-state dispatch is a
   plain indirect call.

Fail-closed rule: a *broken seal* is never a fallback. If a sealed entry exists
but its object is missing, stale or malformed, dispatch returns
`DispatchError::SealBroken` — the runtime must not silently run the foreign
implementation while believing it is running verified native code.

The **dispatch court** replays the entire corpus through the dispatcher and
records `dispatch_verdict.json`:

```text
target  cases_run  native_cases  fallback_cases  broken_seal_cases
        cases_passed  cases_failed  oracle_hash  object_hash  elf_symbol
        dispatch_hash  verdict
```

`verdict = consistent` only when **every** case was served by the sealed object
(`native_cases == cases_run`, `fallback_cases == 0`, `broken_seal_cases == 0`) and
every output matched the oracle. It is also a promotion precondition, alongside
replay and execution, and the `dispatch_hash` is bound into the promotion receipt
and the sealed package.

`fallback_cases` counts only legitimate foreign fallbacks (no capability, no
entry, entry not sealed). A sealed entry that fails verification is counted as
`broken_seal_cases` — terminal, and never reported as a fallback, so the
accounting matches the fail-closed philosophy.

`dispatch_hash` is SHA-256 over `case_id:source:output_hex` per case, so a
fallback (different `source`) changes the hash even if the bytes happened to
match.

Call site (CLI):

```sh
phost port native toupper 61                                    # sealed-object → 41
phost port native memcmp 616263:616264:0300000000000000         # sealed-object → ffffffff
phost port native memchr 616263:62:0300000000000000             # sealed-object → 01000000
phost port native strlen 61626300:0400000000000000              # sealed-object → 0300000000000000
phost port native strrchr 616261626300:62:0600000000000000      # sealed-object → 03000000
phost port native toupper 61 --no-capability                    # foreign-fallback
```

### Sealed composition dispatch court

The leaf courts prove a sealed artifact is correct (`exec`), then preferred
(`dispatch`). The composition court proves they can be **composed by the
runtime**: one composed target built from already-sealed ports, executed
entirely through `NativeDispatcher`, with no foreign calls in the sealed path and
no Rust mirror consulted.

```text
phor:compose:toupper_memchr:c-locale:index:v1

  input:  haystack, needle, n
  oracle: foreign C-locale toupper over the haystack and the needle,
          then foreign memchr over the normalized haystack -> index or -1
  sealed: dispatch sealed toupper once per haystack byte and once for the
          needle, then dispatch sealed memchr once
```

The id is `phor:compose:…`, not `libc:…`: this is no longer a single foreign API
surface but a Phorensic composition over already-sealed ports, and it adds no new
trusted code.

`composition_verdict.json` records the per-stage accounting:

```text
target  stages  locale_contract
cases_run
toupper_hay_native_cases  toupper_needle_native_cases  memchr_native_cases
fallback_cases  broken_seal_cases
cases_passed  cases_failed  dispatches_run
toupper_object_hash  toupper_elf_symbol
memchr_object_hash   memchr_elf_symbol
oracle_hash  chain_hash  verdict  mismatches
```

`verdict = consistent` only when **every** stage was served by the sealed object
for **every** case (`*_native_cases == cases_run`), with zero foreign fallbacks
and zero broken seals, and every index matched the oracle. A broken seal is again
counted separately from a fallback.

`chain_hash` covers the whole chain per case — the stage statuses, the normalized
`toupper` intermediates and the final index — so a skipped or fallback stage
changes the hash even when the index coincides.

The dispatched object hashes are cross-checked by the verifier against the
committed leaf seals, so the composition provably used the exact sealed artifacts
under `phost/evidence/porting/{toupper,memchr}/`.

The corpus reuses the `memchr` corpus and adds C-locale fold cases (`G.*`) where a
needle only matches after `toupper` normalizes both sides — a composition that
dropped the toupper stage cannot pass.

Reproduce:

```sh
phost port compose                                  # composition court (560 cases)
phost port compose 614263:62:0300000000000000       # one composed call
./verify_composition_court.sh                       # determinism
./verify_composition_court.sh --check-committed     # fresh == checked-in
```

### Seal contents

The sealed package binds the qualified target id, the locale contract, the
candidate's **behavior** hash (its outputs over the case domain), the compiled
artifacts: the clean-room **source** hash, the ELF64 **object** hash, the
**receipt** hash, and the **compiler version** — and the runtime residuals: the
**ABI symbol**, the **executed ELF symbol**, the **execution hash**, the
**execution verdict**, the **dispatch hash**, the native/fallback/broken-seal case
counts and the **dispatch verdict**. The sealed store entry points at the compiled
object (`candidate.o`), which is the authoritative implementation, and that same
object is the one loaded, executed and dispatched.

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
                                  promotion, evidence, compiled, exec, dispatch,
                                  composition
examples/jit_port_toupper.phor    toupper native candidate, in Phorensic
examples/jit_port_memcmp.phor     memcmp native candidate, in Phorensic
examples/jit_port_memchr.phor     memchr native candidate, in Phorensic
examples/jit_port_strlen.phor     strlen native candidate, in Phorensic
examples/jit_port_strrchr.phor    strrchr native candidate, in Phorensic
phost/evidence/porting/toupper/   toupper evidence set (256 cases) + candidate.o
phost/evidence/porting/memcmp/    memcmp evidence set (312 cases) + candidate.o
phost/evidence/porting/memchr/    memchr evidence set (482 cases) + candidate.o
phost/evidence/porting/strlen/    strlen evidence set (308 cases) + candidate.o
phost/evidence/porting/strrchr/   strrchr evidence set (336 cases) + candidate.o
phost/evidence/composition/      composition verdict (chain over sealed ports)
verify_jit_porting_court.sh       leaf court verifier (--target toupper|memcmp|memchr|strlen|strrchr)
verify_composition_court.sh       composition court verifier
```

The compiled `candidate.o` / `candidate.receipts.json` are regenerated (and are
not committed); only their hashes are sealed. The promoted artifact is the
compiled object, not the Rust mirror — and that object is executed, not merely
hashed.

Reproduce:

```sh
cargo run -p phost -- port promote toupper
cargo run -p phost -- port promote memcmp
cargo run -p phost -- port native toupper 61                 # dispatch one call
./verify_jit_porting_court.sh --target toupper                 # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
```

`--check-committed` writes only to a temp dir and compares against the checked-in
evidence without touching it — reviewer-grade proof that the committed seal
matches a fresh run.
