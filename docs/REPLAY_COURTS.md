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
as `toupper`, `memcmp`, `memchr`, `strlen` and `strrchr`, plus the POSIX `strspn`),
not arbitrary binary translation.

A target's identity is qualified — `dialect:symbol:locale:contract:version`. The
`dialect` names the **specification the contract is drawn from**, not the library
that implements it: the first five targets are ISO C surfaces (`dialect: libc`), and
`strspn` is POSIX (`dialect: posix`) because ISO C does not specify it. See
`docs/DIALECT_QUALIFICATION.md` for what that qualification does and does not claim.

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

### Sixth target: POSIX `strspn`

```text
target id: posix:strspn:c-locale:u64:v1
dialect:   posix  symbol: strspn  version: host-observed-v1
locale:    C
contract:  length of the initial segment of s consisting only of bytes in accept
corpus:    578 bounded deterministic cases
```

A second dialect and a new observable shape. `dialect: posix` is earned, not chosen:
ISO C does not specify `strspn`, so recording it as `libc:strspn:…` would conflate two
standards (`docs/DIALECT_QUALIFICATION.md`). The observable is a **prefix length
decided by set membership** — different from ordering, match index and
length-to-terminator. Arguments are framed as `s_hex:accept_hex:n_hex`.

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
and fail-closed at `n`), `phor_strrchr` (last in-string match index, NUL
needle → length) and `phor_strspn` (POSIX: set-membership span, `n`-bounded, the
terminator always ending the span because a C string set cannot contain NUL).
Unknown target ids return
`CandidateError::UnsupportedTarget` and malformed arguments return
`CandidateError::MalformedArgs`; there is no identity fallback, so an unsupported
candidate can never pass by accident.

### Second dialect: `posix:strspn`

The five ISO C targets carry `dialect: libc`. `strspn` carries `dialect: posix`
because **ISO C does not specify it** — it is a POSIX function, like `strcspn`,
`strpbrk`, `strtok`, `strcasecmp` and `strdup`. The `dialect` field names the
specification the contract is drawn from, so a POSIX-only contract cannot be recorded
as a C-library contract without conflating two standards.

The observable is a new shape next to ordering, match index and length-to-terminator:

```text
posix:strspn:c-locale:u64:v1

  input:  s (terminator inside the first n bytes), accept (NUL-free, <= 8 bytes), n
  output: the length of the initial segment of s whose bytes are all in accept
  ABI:    _phor_phor_strspn_len(ws: u64, wa: u64, n: u64) -> u64 span
```

The corpus (578 cases) is built so a plausible wrong implementation fails: set sizes
1..=8 make a single-byte compare impossible, accepted bytes placed after the
terminator make a scan that ignores the terminator fail, and the two exhaustive
0..=255 sweeps cover the accepted byte and the stopping byte. The candidate is
branchless and unrolled over the 8 packed lanes (membership is an 8-term OR written
as `m + t - m*t`), emits a 25072-byte ELF64 object with **0 relocations**, and is
loaded and called by the execution court for every case.

The qualification is checked, not asserted: the leaf verifier requires the sealed
package's `dialect` to equal the namespace of the target id. The implementation
observed for the seal is the host C library, so this binds the POSIX contract *as
observed*; implementation-independence is a separate axis, closed by the
cross-implementation court below.

### Cross-implementation court

The dialect names the specification; a seal still observes *one* implementation of it.
The cross-implementation court (`phost::porting::cross_impl`) closes that gap for every
sealed leaf: it observes the **same sealed corpus** through a second, independent
implementation and requires agreement on every case.

```text
target -> cases_for(target)                              the sealed corpus
       -> dialect_cage (host C library, in-process)      -> implementation A
       -> musl probe   (static, out-of-process)          -> implementation B
       -> per-case comparison -> cross-implementation verdict
```

The second implementation is musl, reached out-of-process through a statically linked
probe (`phost/foreign/musl_probe.c`, built with `musl-gcc -static`). Independence is
checked rather than asserted: the verifier builds the probe, requires it to report
`libc=musl` from its own `#ifdef __GLIBC__` check, and parses the ELF to require **no
`PT_INTERP`** — a static binary cannot be dynamically linking the host's library. The
probe's per-symbol decoding is deliberately written out again rather than shared with
the Rust cage, because an observer that shared the cage's code could only confirm the
cage.

The verdict (`cross_implementation_verdict.json`) records both oracle hashes. The
stronger claim is not per-case agreement but that the two implementations produced the
**same sealed trace set**: `secondary_oracle_hash == primary_oracle_hash`, and
`primary_oracle_hash` is the leaf's committed sealed oracle hash, so a cross verdict
cannot be detached from the seal it refers to. A probe that answers nothing fails
closed rather than "agreeing" vacuously, and a probe not built against musl is refused
rather than compared.

Claim hygiene: the host library's version string and the compiled probe's hash are
environment-bound, so they are recorded **observed, not asserted** (the same split the
boot manifest uses for the kernel image). The `residual_hash` covers exactly the
asserted claim, and a test asserts the probe hash is outside it.

The limit is load-bearing: **agreement on a bounded corpus is evidence, not proof of
equivalence.** It shows the sealed corpus does not distinguish glibc from musl; it does
not show the contract holds for every implementation or every input. Promotion itself is
unchanged — this is an additional, orthogonal residual.

Reproduce:

```sh
phost port cross toupper      # 256 cases, 0 disagreements
phost port cross strspn       # 578 cases, the POSIX dialect
./verify_cross_implementation.sh
```

The verifier writes only to a temp dir and fails loudly (exit 2) when `musl-gcc` is
absent, so a cross-implementation claim can never pass by skipping the second
implementation.

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
(`dispatch`). The composition courts prove they can be **composed by the
runtime**: composed targets built from already-sealed ports, executed entirely
through `NativeDispatcher`, with no foreign calls in the sealed path and no Rust
mirror consulted. Seven chains are sealed: the first three are pipelines / dataflow
graphs over sealed leaves, the next two prove that a composition is itself a
sealed port the store publishes, the sixth is the first where a derived value
selects a *buffer* rather than a bound, and the seventh is the first where a
sealed **composition consumes a buffer another composition selected**.

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

#### Second composition: `toupper ∘ strlen ∘ memchr`

The first chain is a map stage into a search stage. The second adds the property
that makes composition more than a pipeline: **the middle stage's result is
consumed as the next stage's argument**.

```text
phor:compose:toupper_strlen_memchr:c-locale:index:v1

  input:  haystack, needle, n     (n = precondition bound: a NUL lies inside haystack[..n])
  oracle: foreign C-locale toupper over the haystack,
          then foreign strlen of the folded haystack -> L,
          then foreign C-locale toupper of the needle,
          then foreign memchr over the folded haystack bounded by L -> index or -1
  sealed: dispatch sealed toupper once per haystack byte,
          dispatch sealed strlen once to derive L,
          dispatch sealed toupper once for the needle,
          then dispatch sealed memchr once with n = L
```

Read plainly it is a **case-insensitive search of a C string**: the string's length
is established by the sealed `strlen` rather than supplied by the caller, and the
sealed `memchr` searches exactly that measured prefix. The middle stage is
load-bearing, not decorative — its corpus deliberately places needle-like bytes
*after* the terminator, so a chain that searched with the caller's `n` instead of
the derived `L` cannot pass.

Its verdict records four stage accounts (`toupper_hay_native_cases`,
`strlen_native_cases`, `toupper_needle_native_cases`, `memchr_native_cases`) plus
the same fallback/broken-seal split, and three object hashes (toupper, strlen,
memchr). Its `chain_hash` covers the derived bound as well as the normalized
intermediates and the final index, so the hash proves the chain, not just the
answer.

Reproduce:

```sh
phost port compose --target toupper_strlen_memchr                        # 350 cases
phost port compose --target toupper_strlen_memchr 62006161:41:0400000000000000
./verify_composition_court.sh --target toupper_strlen_memchr
./verify_composition_court.sh --target toupper_strlen_memchr --check-committed
```

The second call above is the decisive demo: the haystack folds to `42 00 41 41`, the
sealed `strlen` derives the bound `1`, and the sealed `memchr` searches only that
one byte — so the needle `41` (`A`) that occurs *after* the terminator is correctly
not found (`Index: -1`).

#### Third composition: `toupper ∘ strlen ∘ memchr ∘ toupper ∘ memchr`

Chains 1 and 2 are pipelines: each derived value has exactly one consumer, and that
consumer is the stage immediately after the producer. The third chain shows the
dependency pattern is not a one-off — **one derived bound is consumed by two
searches**, and the second consumer is deliberately **non-adjacent**.

```text
phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1

  input:  haystack, needleA, needleB, n   (n = bound: a NUL inside haystack[..n])
  oracle: foreign C-locale toupper over the haystack,
          then foreign strlen of the folded haystack -> L,
          then foreign memchr of folded needleA bounded by L -> iA,
          then foreign memchr of folded needleB bounded by L -> iB
  sealed: dispatch sealed toupper per haystack byte, sealed strlen once -> L,
          sealed toupper+memchr for needleA (n = L) -> iA,
          sealed toupper+memchr for needleB (n = L) -> iB

  notice: stage 6 consumes L, which stage 2 produced — stages 3, 4 and 5 sit
          between them
```

The observable is the pair `(iA, iB)`. That choice is deliberate: the runner performs
no combining logic of its own, so the composition still adds no trusted code beyond
the sealed leaves — it simply holds a derived value and feeds it to every stage that
needs it, in any order. A pipeline that kept a single live intermediate could not do
this.

Six stage accounts are recorded (`toupper_hay_native_cases`, `strlen_native_cases`,
`toupper_needle_a_native_cases`, `memchr_a_native_cases`,
`toupper_needle_b_native_cases`, `memchr_b_native_cases`) plus the same
fallback/broken-seal split; `chain_hash` covers the shared derived bound, both
folded needles and both indexes.

Reproduce:

```sh
phost port compose --target toupper_strlen_memchr_pair                       # 474 cases
phost port compose --target toupper_strlen_memchr_pair 61006262:41:42:0400000000000000
./verify_composition_court.sh --target toupper_strlen_memchr_pair
./verify_composition_court.sh --target toupper_strlen_memchr_pair --check-committed
```

The second call is the decisive demo: the haystack folds to `41 00 42 42`, the sealed
`strlen` derives the bound `1`, and **both** searches use that one bound — so needle A
is found at `0` while needle B, which occurs only *after* the terminator, is correctly
not found (`Index B: -1`).

#### Composition as a first-class sealed port

A composition is not only something the runtime *runs*; it is something the store
*publishes*. The port model therefore has two artifact kinds:

```text
SealedArtifact::LeafObject   { object_hash, object_path }
SealedArtifact::Composition  { composition_id, chain_hash, leaves }
```

`NativeDispatcher::dispatch_port(id, args, auth)`:

1. looks the id up in the capability-gated store (leaf or composition);
2. for a **leaf**, verifies the object hash, maps it once and calls the ABI entry;
3. for a **composition**, checks that every port in the chain's `leaves` is itself
   sealed in the same store, resolves the chain runner by id, and recurses through
   the *same* dispatcher — so nested stages are resolved from the same store and the
   recursion bottoms out in verified objects;
4. binds the result to the composition's `chain_hash` (reported by `sealed_binding`
   only once the composition has actually been dispatched).

An unknown composition id, or a composition whose leaves are not sealed, is a
`SealBroken` — terminal, never a fallback. So a composition is consumed like any other
sealed port, and the outer chain holds no fold logic of its own.

`toupper_each` is the map port that makes this useful: `(bytes, n) -> folded bytes`,
the first sealed port whose output is a buffer rather than a scalar. The nested chain
`phor:compose:toupper_each_strlen_memchr:c-locale:index:v1` dispatches it twice — once
over the haystack, once over the needle — and derives its search bound from the sealed
`strlen`:

```sh
phost port compose --target toupper_each                       # 267 cases, 311 dispatches
phost port compose --target toupper_each 616263:0300000000000000            # folded 414243
phost port compose --target toupper_each_strlen_memchr         # 350 cases, 1400 dispatches
phost port compose --target toupper_each_strlen_memchr 62006161:41:0400000000000000
./verify_composition_court.sh --target toupper_each
./verify_composition_court.sh --target toupper_each --check-committed
./verify_composition_court.sh --target toupper_each_strlen_memchr
./verify_composition_court.sh --target toupper_each_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_memchr_suffix          # the slice chain (688)
./verify_composition_court.sh --target toupper_memchr_suffix --check-committed
```

Its corpus and oracle are deliberately identical to `toupper_strlen_memchr`'s, so the
two chains are a controlled experiment: their `chain_hash`es are **equal**
(`97fa3999…`). The chain hash is a *behavior* hash over the corpus, so behavioral
equivalence is exactly what it should report; the implementation boundary is recorded
separately as `fold_composition_id` + `fold_composition_chain_hash`, and the verifier
cross-checks that nested seal against the committed `toupper_each` verdict — a seal of
a seal.

### The slice chain: a derived value selects a buffer

`composition_suffix` adds the sixth chain and the first shape where a derived value
selects a **buffer** rather than a bound:

```text
phor:compose:toupper_memchr_suffix:c-locale:index:v1

  stages:  libc:toupper (fold)  ->  libc:memchr (derive the origin i)
           ->  SLICE the folded haystack at i  ->  libc:memchr (search the suffix)
  output:  i32 absolute index (i + j), or -1
```

The oracle expresses the slice literally: the foreign `memchr` result advances the base
pointer, and the second foreign `memchr` runs over that slice. There is no
bound-only reading of the oracle.

Two things the corpus is built to pin down:

- `B.*` (28 cases): needleB occurs **before** the origin and nowhere in the suffix, so
the answer is `-1`. A chain that searched the caller's window finds the earlier byte and
returns a smaller index — the court fails it. A test asserts a window-searching chain
disagrees with the foreign oracle on at least 80 of the 688 cases.
- The second search is **data-dependent**: with needleA absent there is no origin and no
slice, so it is never dispatched. That is a separate account
(`memchr_b_not_reached_cases`, 254 of 688) and the sealed-eligibility rule requires
`memchr_b_native_cases + memchr_b_not_reached_cases == cases_run`. It is deliberately
not reported as a native run of a stage that did not happen.

`chain_hash` covers the folded haystack, the derived origin, the suffix offset and the
final index.

### The derived-buffer chain: a composition consumes a buffer another composition selected

`composition_slice_search` adds the seventh chain and the last shape in the dataflow
vocabulary. In every earlier chain a stage reads a caller argument or a slice of a
**leaf** fold the runner performed; here the buffer is produced by one sealed
composition and consumed by another.

```text
phor:compose:toupper_each_slice_search:c-locale:index:v1

  input:  haystack, needleA, needleB, n
  stages: phor:compose:toupper_each (fold the haystack) -> H'
          libc:memchr (derive the origin i in H')
          SLICE the ORIGINAL haystack at i -> S
          phor:compose:toupper_memchr (consume S; fold it and search) -> j
  output: i32 absolute index (i + j), or -1
```

Two things make it a new shape rather than a re-labelling of the sixth chain:

- **The buffer the second half reads is the one the first half selected**, and it is
handed to `toupper_memchr` as that composition's *haystack*. The outer runner holds
no fold or search of its own.
- **The consumer must fold the slice itself.** The slice is taken from the *unfolded*
haystack, so a chain that handed the raw slice to a bare `memchr` with a folded
needle would miss every lowercase match in the suffix. A test asserts at least 50 of
the 688 cases falsify that shape, so the composition is load-bearing rather than
decorative.

The corpus, oracle and observable are deliberately the same as
`toupper_memchr_suffix`'s — a controlled experiment: the committed **oracle hash is
identical** (`8c354e38…`), while the implementation boundary is recorded separately as
two nested seals. The verdict adds `fold_composition_id`/
`fold_composition_chain_hash` (`toupper_each`, `196940c2…`) and
`search_composition_id`/`search_composition_chain_hash` (`toupper_memchr`,
`d00bdf26…`), each cross-checked by the verifier against the committed inner verdict —
a seal of a seal. Its `chain_hash` covers the folded haystack, the derived origin, the
**slice handed to the consumer composition**, the suffix offset and the final index.

Reproduce:

```sh
phost port compose --target toupper_each_slice_search                    # 688 cases
phost port compose --target toupper_each_slice_search 62617862:61:62:0400000000000000
./verify_composition_court.sh --target toupper_each_slice_search
./verify_composition_court.sh --target toupper_each_slice_search --check-committed
```

The second call is the decisive demo: `baxb` folds to `BAXB`, needleA `a` gives origin
`1`, so the buffer handed to `toupper_memchr` is the **unfolded** slice `617862`
(`axb`) — which the composition folds to `AXB` before finding `b` at offset `2`, for
the absolute index `3`. A chain that searched the raw slice would report `-1`.

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

### Persistent sealed port store

Deriving a seal is the court's job; **loading** one is the runtime's. The store
index `phost/evidence/store/index.json` is a committed artifact that names every
sealed port — six leaf object hashes and seven composition chain hashes — so a
call site can resolve a seal without invoking `phorc` and without replaying an
oracle.

```text
phost/src/porting/store.rs     StoreDocument + StoreEntry (leaf/composition),
                              load / load_with_document / document_from_evidence
                              / regenerate; StoreError fails closed
```

Loading is verified end to end:

```text
schema == phorensic.porting.store.v1, entry_count matches, residual hash matches
every entry is `sealed`; targets are unique
leaf:        object_path exists and SHA-256(object bytes) == object_hash
composition: chain_hash is a 64-hex digest; every leaf is a sealed entry here
```

The leaf `candidate.o` files **are committed**: the seal is the object's bytes, so
committing them is what lets the store be loaded without a compiler. The verifier
independently recompiles each `.phor` source and requires byte-equality, so the
committed object is checked rather than trusted. A composition's chain hash is
read from its committed verdict, so a nested chain resolves its inner port instead
of re-running the inner court.

`--store` makes the composition court load its index from the store rather than
derive it; the committed verdict must reproduce (same `chain_hash`), and with an
impossible `--phorc` path it proves no compiler is invoked. Loads fail closed:
a missing store is an error, never a silent fallback, and without `PORTING` the
store is not read at all.

### Sealed native service: one verified load, many consumers

The store is committed; the **service** is where it is shared. `SealedNativeService`
(`phost/src/porting/service.rs`) owns one verified index for its lifetime:

```text
SealedNativeService::open(path, auth) -> verify + load the store ONCE
SealedNativeService::call(port_id, args, auth) -> dispatch_port(..)
                                                   (no store access at all)
```

`open` requires `PORTING`; without it the store is never read. The type is the
proof that no call re-reads the store: `call` can only reach the dispatcher the
service already holds.

A **session** is the deterministic plan — `session_plan()` — that serves every
sealed port in the store once, with a recorded expected result:

```text
six leaves          seven compositions
store loads:     1   ports: 13   calls: 13 native   fallback: 0   broken: 0
objects mapped:  6   dispatches: 68
fan-in:  toupper 39  memchr 10  strlen 4  memcmp 1  strrchr 1
         toupper_each 5 (own call + four nested fold stages)
         toupper_memchr 2 (own call + the slice-search chain's stage)
```

`dispatches` and `per_port` count every `dispatch_port` resolution, including the
stages a chain dispatches from inside its own runner — that is what makes the
fan-in visible rather than just "thirteen calls".

Every composition in the store is also a **dispatchable port**: `composition_runner`
resolves all seven chain ids, so `dispatch_port` on `phor:compose:toupper_memchr:…`
returns its chain output — and the nested chains resolve the sealed compositions
`toupper_each` and `toupper_memchr`. A cycle in the composition graph is rejected when the
store loads (`StoreError::CompositionCycle`), because resolving a chain recurses
through the index and a cycle could never terminate.

### Where it lives

```text
phost/src/porting/                target, dialect_cage, oracle_trace,
                                  behavior_signature, candidate, replay_court,
                                  promotion, evidence, compiled, exec, dispatch,
                                  composition, composition_strlen_memchr,
                                  composition_pair, composition_toupper_each,
                                  composition_nested, composition_suffix,
                                  composition_slice_search, store,
                                  json, service, cross_impl
phost/foreign/musl_probe.c        the second-implementation observer (musl-gcc -static)
phost/evidence/store/index.json   persistent store: every sealed port (leaf objects
                                  + composition chain hashes), committed
phost/evidence/session/           session verdict (one load, many consumers)
phost/evidence/cross/             cross-implementation verdicts (one per sealed leaf)
verify_store.sh                   persistent store verifier (load, regenerate,
                                  independent recompilation, no-compiler runtime)
verify_session.sh                 sealed native service verifier (one load, thirteen
                                  ports, six objects, fan-in, fail-closed)
verify_cross_implementation.sh    cross-implementation verifier (rebuilds the musl
                                  probe, checks independence, all six leaves)
```

The examples and evidence:

```text
examples/jit_port_toupper.phor    toupper native candidate, in Phorensic
examples/jit_port_memcmp.phor     memcmp native candidate, in Phorensic
examples/jit_port_memchr.phor     memchr native candidate, in Phorensic
examples/jit_port_strlen.phor     strlen native candidate, in Phorensic
examples/jit_port_strrchr.phor    strrchr native candidate, in Phorensic
examples/jit_port_strspn.phor     POSIX strspn native candidate, in Phorensic
phost/evidence/porting/toupper/   toupper evidence set (256 cases) + candidate.o
phost/evidence/porting/memcmp/    memcmp evidence set (312 cases) + candidate.o
phost/evidence/porting/memchr/    memchr evidence set (482 cases) + candidate.o
phost/evidence/porting/strlen/    strlen evidence set (308 cases) + candidate.o
phost/evidence/porting/strrchr/   strrchr evidence set (336 cases) + candidate.o
phost/evidence/porting/strspn/    strspn evidence set (578 cases, POSIX) + candidate.o
phost/evidence/composition/      composition verdicts (chains over sealed ports)
phost/evidence/cross/            cross-implementation verdicts (one per sealed leaf)
verify_jit_porting_court.sh       leaf court verifier (--target toupper|memcmp|memchr|strlen|strrchr|strspn)
verify_composition_court.sh       composition court verifier (--target toupper_memchr|toupper_strlen_memchr|toupper_strlen_memchr_pair|toupper_each|toupper_each_strlen_memchr|toupper_memchr_suffix|toupper_each_slice_search)
verify_store.sh                   persistent store verifier
verify_cross_implementation.sh    cross-implementation verifier
```

The committed leaf `candidate.o` is the seal, so it is committed; only
`candidate.receipts.json` is regenerated (its hash is sealed). The promoted
artifact is the compiled object, not the Rust mirror — and that object is executed
and dispatched, not merely hashed.

Reproduce:

```sh
cargo run -p phost -- port store                            # load + verify the store
cargo run -p phost -- port store --write                    # regenerate it
cargo run -p phost -- port session                          # one load, many consumers
cargo run -p phost -- port promote toupper
cargo run -p phost -- port native toupper 61                # dispatch one call (from the store)
cargo run -p phost -- port compose --target toupper_memchr --store --phorc /nonexistent/phorc
./verify_store.sh                                           # persistent store
./verify_session.sh                                         # sealed native service
./verify_jit_porting_court.sh --target toupper                 # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
```

`--check-committed` writes only to a temp dir and compares against the checked-in
evidence without touching it — reviewer-grade proof that the committed seal
matches a fresh run.
