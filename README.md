# Phorensicos — Phorensic OS

[![Phorensic OS CI](https://github.com/infinityabundance/phorensicos/actions/workflows/ci.yml/badge.svg)](https://github.com/infinityabundance/phorensicos/actions/workflows/ci.yml)

<img src="assets/screen.png" alt="Phorensic OS boot screen: 1024x768 QEMU framebuffer with the boot phase table and compositor window" width="100%">

**Phorensic OS** is a research operating system built around *residual primacy*:
every state change emits a **residual** — a signed, replayable evidence record —
so that the system's history is always reconstructable and auditable. Authority
is granted only through **affine capabilities** (no ambient authority), and there
is no unverified state change.

The project is deliberately small in its trusted core and aspirational in its
design: it pairs a `no_std` x86-64 kernel with a bootstrap compiler for a
purpose-built language (`.ph` / `.phor`).

> **Status: bootstrapping / observed.** The compiler pipeline, the host runtime
> and the booted GUI kernel are all working end-to-end and covered by tests.
> The language checker and the code generator are still maturing, so parts of the
> `src/**/*.phor` corpus lower successfully while still producing checker
> diagnostics. See [Current status](#current-status) for exact numbers.

---

## Repository layout

| Path | What it is |
|------|------------|
| `phorc/` | **Bootstrap compiler** — `.ph`/`.phor` → ELF64 object, byte-attribution receipts and sealed packages. Lexer → parser → checker → PHIR → x86-64 → ELF64. |
| `phost/` | **Host / runtime library** — canvas, console, compositor, shell, status screen, serial + PS/2 keyboard drivers, and the kernel `nucleus` (GDT/IDT/TSS/paging, assembly shims). Builds with `std`, `alloc`, or `no_std`. |
| `phost_kernel/` | **`no_std` kernel staticlib** — links the boot stub into a Multiboot image that renders a GUI to the QEMU VGA framebuffer. |
| `src/` | **Phorensic OS system source** (~232 `.phor` modules): kernel, memory, drivers, forensic store, courts, dialect cages, GUI, package system, porting engine, … |
| `examples/` | 51 runnable `.phor`/`.ph` examples (shell, compositor, drivers, courts, porting). |
| `tests/` | `.phor` test suites plus `compile-pass/` fixtures. |
| `fixtures/` | Golden residual/oracle/court fixtures and package specs. |
| `docs/` | Language, compiler, kernel, store, courts, GUI and reviewer specifications. |

## Toolchain

- Rust (stable) — builds `phorc`, `phost`.
- `rustup target add x86_64-unknown-none` — required for the kernel.
- `nasm`, `ld.lld` and `objcopy` (binutils) — kernel stub assembly + link.
- `qemu-system-x86_64` — optional, for booting the kernel.

## Quick start

```sh
# 1. Build and test the host workspace (phorc + phost)
cargo test

# 2. Compile a Phorensic program end-to-end to an ELF64 object
cargo run -p phorc -- examples/hello.phor /tmp/hello.o \
    --emit-receipts --emit-seal

# 3. Verify the seal and run the evidence court
cargo run -p phorc -- --verify-seal /tmp/hello.sealed_package.json
cargo run -p phorc -- --court-replay /tmp/hello.sealed_package.json
```

`phorc --help` lists the full option set (`--emit-receipts`, `--emit-kernel`,
`--emit-seal`, `--verify-seal`, `--court-verify`, `--court-replay`).

### Build and boot the kernel in QEMU

```sh
cd phost_kernel
rustup target add x86_64-unknown-none   # once
./build_kernel.sh                       # staticlib → nasm stub → ld.lld → flat image
./boot_qemu.sh phorensic-kernel.elf evidence 8
./verify_evidence.sh evidence           # proof bytes, ABI @0x300000, screen palette
./evidence_manifest.sh evidence         # refresh evidence_manifest.json
cat evidence/serial.log evidence/debug.log
```

On a successful boot the kernel writes its proof bytes (`Ph`) to both COM1
(`0x3F8`) and the QEMU debug port (`0xE9`), and the 1024×768 boot GUI is rendered
to the linear framebuffer (`evidence/screen.ppm`). `verify_evidence.sh` checks
all of that (13 checks) and `evidence_manifest.sh` records the hashes, the
framebuffer ABI location (`0x300000`) and the exact commands/toolchain used.
The committed manifest is `phost_kernel/evidence_manifest.json`; the raw dumps
stay gitignored.

The manifest is deliberately two-tier, so its claim cannot be misread:

- `asserted_reproducible_evidence` — the five captured boot-evidence artifacts
  (`serial.log`, `debug.log`, `screen.ppm`, `fb-abi.bin`, `lfb.bin`). These are
  the court evidence and **are** asserted byte-identical across hosts and
  containers (the kernel CI checks all five against the committed manifest).
- `observed_toolchain_bound_build` — the kernel image hash/size, recorded with
  `asserted_reproducible: false` because image bytes depend on the
  rustc/nasm/ld.lld/binutils versions. Useful as an observed build fact; **not**
  a cross-toolchain reproducibility guarantee.

A top-level `manifest_claim` states exactly what is and is not asserted.

> `phost_kernel` is a `no_std` staticlib cross-compiled for
> `x86_64-unknown-none`, so it is excluded from the default workspace build. Use
> `build_kernel.sh` (it sets the target via `phost_kernel/.cargo/config.toml`).

## JIT-Porting Court

Phorensic OS ports *behavior*, not binaries. Six leaf courts have run
end-to-end at the API boundary — five ISO C surfaces and one POSIX surface:

```text
foreign behavior → dialect cage       (observe a named specification as a black box)
                 → oracle traces      (sealed, ordered, locale-recorded cases)
                 → behavior signature (combined SHA-256 oracle hash)
                 → native candidate   (clean-room phor_*)
                 → replay court       (replay the full case set)
                 → comparison         (exact output / status / effects)
                 → promotion          (only on an exact match)
                 → sealed package     (native:<qualified target id>)
                 → execution court    (load the sealed ELF64 object and call it)
                 → dispatch court     (the runtime serves calls from the sealed object)
                 → composition court  (sealed ports become runtime building blocks)
                 → persistent store   (the seal is committed; the runtime loads, not re-derives)
                 → sealed native service (one verified load, many consumers)
                 → cross-implementation court (the same seal observed through a second implementation)
```

| Target | Corpus | Replay | Executed object | Runtime dispatch |
|--------|--------|--------|-----------------|------------------|
| `libc:toupper:c-locale:u8:v1` | exhaustive `0x00..=0xff` (256 cases) | **256/256 pass** | **256/256 pass** | **256/256 native**, 0 fallback |
| `libc:memcmp:c-locale:sign:v1` | bounded deterministic corpus (312 cases) | **312/312 pass** | **312/312 pass** | **312/312 native**, 0 fallback |
| `libc:memchr:c-locale:index:v1` | bounded deterministic corpus (482 cases) | **482/482 pass** | **482/482 pass** | **482/482 native**, 0 fallback |
| `libc:strlen:c-locale:u64:v1` | bounded deterministic corpus (308 cases) | **308/308 pass** | **308/308 pass** | **308/308 native**, 0 fallback |
| `libc:strrchr:c-locale:index:v1` | bounded deterministic corpus (336 cases) | **336/336 pass** | **336/336 pass** | **336/336 native**, 0 fallback |
| `posix:strspn:c-locale:u64:v1` | bounded deterministic corpus (578 cases) | **578/578 pass** | **578/578 pass** | **578/578 native**, 0 fallback |

The `memcmp` corpus deliberately forces length, buffers and ordering: lengths
`0..=8`, five patterns, every first-mismatch position, the `n`-boundary around a
mismatch, and the unsigned edge bytes `00/01/7f/80/fe/ff`. Its observable is the
**sign** of the return value (`-1 | 0 | 1`), which is the C contract.

The `memchr` corpus forces search semantics: lengths `0..=8`, the first match at
every index, an absent needle, repeated needles (first occurrence wins), the
`n`-boundary around a match, the unsigned edge bytes, and an exhaustive sweep of
all 256 needle values. `memchr` returns a *pointer*, which is not portable
behavior, so the observable is normalized to the **index** of the first match or
`-1` when absent — the actual C contract (`result - s`).

The `strlen` corpus forces NUL-termination semantics: the complete `(k, n)` grid
of terminator index `k` and scan bound `n` with `0 <= k < n <= 8`, non-NUL
filler bytes (including `0x7f`/`0x80`) at every prefix position, tails after the
terminator that are themselves NUL so the *first* NUL must win, buffers longer
than the bound, and an exhaustive sweep of all 256 byte values proving that only
`0x00` terminates. `strlen` takes no length argument, so the observable is the
**length** (index of the first NUL), and the ABI carries `n` only as a
precondition bound so the foreign observation cannot read past the caller's
window; a terminator outside the bound is out of contract and fails closed at `n`.

The `strrchr` corpus is the mirror of `memchr` — it forces **last**-match semantics
inside the **string**, not the window: unique occurrences at every in-string index,
repeated occurrences so the last wins, the empty string, needles that occur only
after the terminator (which must never match), needles that occur both before and
after it (the in-string occurrence wins), the unsigned edge bytes with copies of
`0x7f`/`0x80` after the terminator, and an exhaustive sweep of all 256 needle
values against a haystack whose tail repeats bytes from the string. A needle of
`0` yields the terminator index (the length), and every case has a NUL inside its
bound. `strrchr` returns a pointer, so the observable is normalized to the index;
`n` is the ABI precondition bound.

### A second dialect: `posix`

The five ISO C targets all carry `dialect: libc`. The sixth does not:

```text
posix:strspn:c-locale:u64:v1
```

**ISO C does not specify `strspn`** — it is POSIX (so are `strcspn`, `strpbrk`,
`strtok`, `strcasecmp`, `strdup`). Recording it as `libc:strspn:…` would conflate two
standards, which is exactly what the qualified id exists to prevent. The `dialect`
field names the **specification the contract is drawn from**, not the library that
implements it.

The observable is also a new shape next to ordering (`memcmp`), first/last match
(`memchr`/`strrchr`) and length-to-terminator (`strlen`):

```text
input:  s (a buffer whose NUL terminator lies within the first n bytes),
        accept (a NUL-free set of at most 8 bytes), n
output: the length of the initial segment of s whose bytes are all in accept
```

A prefix length decided by **set membership** — so the corpus pins every set size
1..=8 (a single-byte compare cannot pass), the empty set, the empty string, a stop
byte before the terminator, and both exhaustive 0..=255 sweeps. Because a C string
set can never contain NUL, the terminator always ends the span, which is what keeps
the scan inside one packed word.

The honest limit is stated in the qualification review, `docs/DIALECT_QUALIFICATION.md`:
`posix:` records the *specification namespace*; the *implementation* observed is still
the host C library, and the seal binds the observed behavior by oracle hash, so a
different implementation with different behavior cannot pass silently. Making the
result implementation-independent is a separate axis, and it is now closed by the
cross-implementation court described in the next section.

The verifier now requires the sealed package's `dialect` to equal the namespace of
the target id, so this cannot drift back into an unqualified claim.

### The implementation axis: a second implementation

A seal records an observation of **one** implementation — the host C library — so a
`libc:` or `posix:` id names the contract, not the implementation that was observed.
The **cross-implementation court** closes that gap: for every sealed leaf it observes
the *same sealed corpus* through a **second, independent implementation** — musl,
compiled statically by `musl-gcc` so an out-of-process observer cannot be the host's
library in disguise — and requires agreement on every case:

```text
sealed corpus → dialect cage (host libc, in-process)   → implementation A
              → musl probe  (static, out-of-process)  → implementation B
              → per-case comparison → cross-implementation verdict
```

The claim is deliberately narrow and checkable: the two implementations produced the
same **sealed trace set** (`secondary_oracle_hash == primary_oracle_hash`), not merely
the same answers — and `primary_oracle_hash` is the leaf's committed sealed oracle
hash, so the cross verdict cannot be detached from the seal it refers to:

| Target | Cases | Agreements | Disagreements |
|--------|-------|-----------|---------------|
| `libc:toupper:c-locale:u8:v1` | 256 | **256** | 0 |
| `libc:memcmp:c-locale:sign:v1` | 312 | **312** | 0 |
| `libc:memchr:c-locale:index:v1` | 482 | **482** | 0 |
| `libc:strlen:c-locale:u64:v1` | 308 | **308** | 0 |
| `libc:strrchr:c-locale:index:v1` | 336 | **336** | 0 |
| `posix:strspn:c-locale:u64:v1` | 578 | **578** | 0 |

Two facts about the run are environment-bound and are recorded as **observed, not
asserted** (the same split the boot manifest uses for the kernel image): the host
library's version string, and the compiled probe's hash (`musl-gcc`-version-bound).
The asserted claim is reproducible instead: the verifier rebuilds the probe, reruns
the corpus, and requires the asserted fields to match the committed verdict without
touching it. The probe is edited independently of the Rust cage and its per-symbol
decoding is written out again, because an observer that shared the cage's code could
only confirm the cage.

The honest limit stays: **agreement on a bounded corpus is evidence, not proof of
equivalence.** It shows the sealed corpus does not distinguish the two implementations;
it does not show the contract holds for every implementation or every input.

### Composition: sealed ports as runtime building blocks

Seven composed targets prove the sealed artifacts are not isolated tricks. The
first chains a map stage into a search stage:

```text
phor:compose:toupper_memchr:c-locale:index:v1

oracle:  foreign toupper over the haystack + needle, then foreign memchr
sealed:  dispatch the sealed toupper once per haystack byte and once for the
         needle, then dispatch the sealed memchr once — no foreign calls in the
         sealed path, no Rust mirror consulted
```

Their ids are `phor:compose:…`, not `libc:…`: these are Phorensic compositions
over already-sealed ports, and they add no new trusted code.

| Composition | Cases | Shape | Fallback | Broken seal |
|-------------|-------|-------|----------|-------------|
| `toupper_memchr` | 560 | map → search | **0** | **0** |
| `toupper_strlen_memchr` | 350 | map → measure → search (bound derived) | **0** | **0** |
| `toupper_strlen_memchr_pair` | 474 | one derived bound, two searches (non-adjacent) | **0** | **0** |
| `toupper_each` | 267 | buffer → folded buffer (a nested-usable port) | **0** | **0** |
| `toupper_each_strlen_memchr` | 350 | **nested**: composition → measure → search | **0** | **0** |
| `toupper_memchr_suffix` | 688 | **slice**: first match selects the buffer a second search reads | **0** | **0** |
| `toupper_each_slice_search` | 688 | **composition consumes a derived buffer**: nested fold → slice → nested search | **0** | **0** |

The first corpus reuses the `memchr` corpus and adds C-locale fold cases where a
needle only matches *after* `toupper` normalizes both sides — so a composition
that skipped the toupper stage would fail. The `composition_verdict.json` records
the per-stage source, the two sealed object hashes the chain dispatched to (which
the verifier cross-checks against the committed leaf seals), and a `chain_hash`
that covers the normalized intermediates and the stage statuses, not just the
final index.

The second is the generalization: three stages, and the middle stage's **result is
consumed as the next stage's argument**, not merely carried along.

```text
phor:compose:toupper_strlen_memchr:c-locale:index:v1

oracle:  foreign toupper over the haystack, foreign strlen of the folded
         haystack -> L, foreign toupper of the needle, then foreign memchr
         over the folded haystack bounded by L
sealed:  dispatch the sealed toupper once per haystack byte, dispatch the sealed
         strlen once to derive L, dispatch the sealed toupper once for the
         needle, then dispatch the sealed memchr once with n = L
```

Read plainly, it is a **case-insensitive search of a C string**: the string's
length is established by the sealed `strlen` rather than supplied by the caller,
and the sealed `memchr` searches exactly that measured prefix. That makes the
middle stage load-bearing, not decorative — its corpus deliberately places
needle-like bytes *after* the terminator, so a chain that searched with the
caller's `n` instead of the derived `L` fails. Its `chain_hash` covers the derived
bound as well as the normalized intermediates, so the hash proves the *chain*, not
just the answer.

The third shows the dependency pattern is not a one-off **pipe**: one derived value
is consumed by **two** stages, and the second consumer is deliberately **not
adjacent** to the producer.

```text
phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1

oracle:  foreign toupper over the haystack, foreign strlen of the folded
         haystack -> L, then foreign memchr of folded needleA bounded by L -> iA,
         then foreign memchr of folded needleB bounded by L -> iB
sealed:  dispatch sealed toupper per haystack byte, sealed strlen once -> L,
         sealed toupper+memchr for needleA (n = L) -> iA, then
         sealed toupper+memchr for needleB (n = L) -> iB

notice:  the sixth stage consumes L, which stage two produced — stages three,
         four and five sit between them
```

Its observable is the pair `(iA, iB)`, so the runner performs no combining logic of
its own: it holds a derived value and feeds it to every stage that needs it, in any
order. That is what makes it a dataflow graph rather than a pipeline. The corpus
pins both indexes independently (every ordered pair of in-string match positions,
one-present/one-absent, identical needles, and occurrences that exist only after
the terminator — which the shared bound must exclude *for both* searches), and its
`chain_hash` covers the derived bound and both indexes.

### Composition as a sealed port

A composition is not only something the runtime *runs* — it is something the store
*publishes*. The port model therefore has two artifact kinds:

```text
SealedArtifact::LeafObject   { object_hash, object_path }      a compiled ELF64 object
SealedArtifact::Composition  { composition_id, chain_hash, leaves }   a sealed chain
```

A composition id resolves to a **chain runner** by id; the dispatcher checks that the
chain's leaves are themselves sealed, recurses through the *same* dispatcher, and
binds the result to the composition's chain hash. An unknown composition id is a
broken seal — never an implicit fallback. So a composition is consumed exactly like a
leaf: looked up, seal-checked, dispatched.

The fourth chain is the buffer-to-buffer map that makes this useful — it is the first
sealed port whose output is a *buffer* rather than a scalar:

```text
phor:compose:toupper_each:c-locale:u8s:v1

  input:  bytes, n          (fold the first n bytes)
  alias:  (u8[] bytes, usize n) -> u8[] folded
```

The fifth chain uses it as a **stage**, so its fold is a lookup rather than runner
code:

```text
phor:compose:toupper_each_strlen_memchr:c-locale:index:v1

  input:  haystack, needle, n
  oracle: foreign toupper over the haystack -> H', foreign strlen of H' -> L,
          foreign toupper of the needle -> c', foreign memchr of H' for c' bounded by L
  sealed: dispatch the sealed COMPOSITION toupper_each -> H',
          dispatch the sealed strlen -> L,
          dispatch the same sealed COMPOSITION over the needle -> c',
          dispatch the sealed memchr with n = L
```

Two details make this a real check rather than a label:

- Its `stages` list contains a **composition** id, and the verifier cross-checks the
  recorded `fold_composition_chain_hash` against the committed `toupper_each`
  verdict — a seal of a seal.
- Its corpus and oracle are **deliberately identical** to `toupper_strlen_memchr`'s, so
  the two chains are a controlled experiment. Their `chain_hash`es are equal
  (`97fa3999…`), which is the point: the chain hash is a *behavior* hash over the
  corpus, and the nested implementation is behaviorally identical to the inline one.
  The implementation boundary is recorded separately, not smuggled into the behavior
  hash.

### A derived value that selects a buffer

The first five chains derive a **scalar**: `strlen` yields a length used as a search
bound, and the pair chain shares one bound between two searches. A bound narrows how
far a search looks; every stage still reads the same buffer from the same origin.

The sixth chain adds the missing shape — a derived value that selects a **buffer**:

```text
phor:compose:toupper_memchr_suffix:c-locale:index:v1

  input:  haystack, needleA, needleB, n
  oracle: foreign toupper over the haystack -> H',
          foreign memchr of H' for folded needleA, bounded by n -> i,
          and only if that matched, foreign memchr of H'[i..] for folded
          needleB, bounded by n - i -> j, reporting i + j or -1
  sealed: fold with the sealed toupper, derive i with the sealed memchr, SLICE the
          folded haystack at i, search the suffix with the sealed memchr again
```

Two properties are new, and the 688-case corpus is built so a chain that lacks either
fails rather than merely differing:

- **The slice is load-bearing.** A needleB that occurs *before* the derived origin but
  not in the suffix must return `-1`. A chain that kept the caller's window finds that
  earlier occurrence and returns a smaller index — a wrong answer, not a rounding
  difference. The `B.` group is exactly those 28 cases, and the tests assert that a
  window-searching chain disagrees with the oracle on at least 80 cases.
- **A stage is data-dependently skipped.** When needleA is absent there is no origin,
  no slice, and no reason to dispatch the second search. That is counted separately as
  `memchr_b_not_reached_cases` (254 of 688) rather than dressed up as a native run of a
  stage that never happened; the sealed-eligibility rule requires the two accounts
  (`memchr_b_native_cases + memchr_b_not_reached_cases`) to cover every case.

The `chain_hash` covers the derived **origin** and the **suffix offset** as well as the
final index, so a chain that skipped the slice but coincidentally produced the right
answer still differs.

### A composition that consumes a derived buffer

The seventh chain closes the dataflow vocabulary. In every earlier chain a stage reads
either a caller argument, or a slice of a **leaf** fold the runner performed itself.
Here the buffer is produced by one sealed composition and consumed by another:

```text
phor:compose:toupper_each_slice_search:c-locale:index:v1

  input:  haystack, needleA, needleB, n
  oracle: foreign toupper over the haystack -> H',
          foreign memchr of H' for folded needleA, bounded by n -> i,
          and only if that matched, foreign memchr of H'[i..] for folded
          needleB, bounded by n - i -> j, reporting i + j or -1
  sealed: dispatch the sealed COMPOSITION toupper_each over the haystack -> H',
          dispatch the same sealed COMPOSITION over needleA -> a',
          dispatch the sealed memchr over H' for a' -> i,
          SLICE the original haystack at i -> S,
          dispatch the sealed COMPOSITION toupper_memchr over S for needleB -> j
```

Two things make it a genuinely new shape rather than a re-labelling of the sixth
chain:

- **A composition consumes a derived buffer.** The second half of the chain reads the
  buffer the first half selected, and hands it to `toupper_memchr` as that
  composition's *haystack*. The outer runner holds no fold and no search of its own.
- **The consumer must fold the slice itself.** The slice is taken from the
  *unfolded* haystack, so a chain that handed the raw slice to a bare `memchr` with a
  folded needle would miss every lowercase match in the suffix. The tests assert that
  at least 50 of the 688 cases falsify that plausible wrong implementation, so the
  composition is load-bearing rather than decorative.

The corpus, oracle and observable are **deliberately the same** as
`toupper_memchr_suffix`'s — a controlled experiment: the two chains report the same
answer for every case, and the difference is the implementation boundary alone. The
committed oracle hash is identical (`8c354e38…`); the nested implementation boundary is
recorded separately as two chain hashes, `fold_composition_chain_hash`
(`toupper_each`, `196940c2…`) and `search_composition_chain_hash` (`toupper_memchr`,
`d00bdf26…`), each cross-checked by the verifier against the committed inner verdict —
a seal of a seal.

### Persistent sealed port store

A court *derives* a seal: it observes foreign behavior, compiles the clean-room
candidate, replays it, and publishes what survives. That derivation is expensive —
it invokes `phorc` and re-runs nested composition courts — and a running system
should not do it at a call site.

So the store is itself **a committed artifact**. `phost/evidence/store/index.json`
names every sealed port and its verified hashes — leaf object hashes *and*
composition chain hashes — and the runtime **loads** it instead of re-deriving it:
no compiler invocation, no oracle replay.

Loading is **verified**, and fails closed:

```text
schema match + residual-hash match     (the entries are covered, not appended to)
every entry is `sealed`, targets unique
leaf:  object_path exists and SHA-256(object bytes) == object_hash
composition: chain_hash is a SHA-256, and every leaf is itself a sealed entry
```

One store holds thirteen ports:

| Kind | Ports |
|------|-------|
| Leaf objects | the six compiled `.phor` candidates (`toupper`, `memcmp`, `memchr`, `strlen`, `strrchr` — ISO C — and `strspn` — POSIX) |
| Compositions | `toupper_memchr`, `toupper_strlen_memchr`, `toupper_strlen_memchr_pair`, `toupper_each`, `toupper_each_strlen_memchr`, `toupper_memchr_suffix`, `toupper_each_slice_search` |

This is what makes the store a real artifact rather than a cache: the committed leaf
`candidate.o` files **are** committed evidence (the seal is the object's bytes), and
the composition chain hashes are read from the committed verdicts instead of being
recomputed by re-running the inner court.

```sh
cargo run -p phost -- port store              # load + verify the committed index
cargo run -p phost -- port store --write      # regenerate it from committed evidence
cargo run -p phost -- port native toupper 61  # call site: loads the store, no compiler
# The composition court can also run from the store; an impossible --phorc path
# proves no compiler is invoked:
cargo run -p phost -- port compose --target toupper_memchr --store --phorc /nonexistent/phorc
./verify_store.sh                             # the store verifier
```

`--store` on the composition court is the A/B: the same verdict must reproduce from
the committed index alone, and its `chain_hash` must equal the committed one. The
single composed **call** (`port compose <args>`) always runs from the store — that is
the runtime path.

### Sealed native service: one verified load, many consumers

Loading the store per call is right for a one-shot CLI and wrong for a running
system. `SealedNativeService` (`phost/src/porting/service.rs`) owns **one verified
index** for its whole lifetime. The type is the proof that the store is loaded
once: `open` is the only place with a path to `store`, and `call` has access to
nothing but the dispatcher it already holds.

A **session** is a deterministic plan that serves every sealed port in the store —
six leaves (one of them POSIX) and seven compositions — through that one service, with a recorded
expected result for each. Thirteen ports, one load, **six mapped objects**, and the
same leaf reused by every chain that consumes it:

```text
store loads:     1        ports in store: 13
calls:          13        native:        13        fallback: 0   broken seal: 0
objects mapped:  6        dispatches:    68
fan-in (resolutions, nested stages included):
toupper 39   memchr 10   strlen 4   memcmp 1   strrchr 1   strspn 1
toupper_each 5 (its own call + four nested fold stages)
toupper_memchr 2 (its own call + the slice-search chain's stage)   others 1
```

Every composition in the store is also a **dispatchable port**, not just a court
result: the dispatcher resolves a composition id to its typed `CompositionIR`
(Phase 2 — a composition is *data*, evaluated by one interpreter), so
`dispatch_port` on `phor:compose:toupper_memchr:…` runs the sealed chain — and the
nested chains resolve the sealed compositions `toupper_each` and `toupper_memchr`
through the same index. The committed session verdict is byte-identical under the
IR runtime: the mechanism changed, not the behavior. See `docs/COMPOSITION_IR.md`.

```sh
cargo run -p phost -- port session                 # one load, thirteen ports, six objects
cargo run -p phost -- port session --no-capability  # store never read (capability denied)
./verify_session.sh                                # the session verifier
```

The store index is also checked for **composition cycles** when it loads: the
runtime resolves a chain by recursing through the index, so a cyclic index is
rejected as data rather than followed.

```sh
cargo run -p phost -- port promote toupper   # observe → replay → execute → dispatch → seal
cargo run -p phost -- port promote memcmp
cargo run -p phost -- port promote memchr
cargo run -p phost -- port promote strlen
cargo run -p phost -- port promote strrchr
cargo run -p phost -- port promote strspn     # the POSIX dialect
cargo run -p phost -- port cross toupper      # the same sealed corpus through musl
cargo run -p phost -- port cross strspn       # the POSIX dialect, second implementation
cargo run -p phost -- port native toupper 61              # call site: run the sealed object
cargo run -p phost -- port native memcmp 616263:616264:0300000000000000
cargo run -p phost -- port native memchr 616263:62:0300000000000000
cargo run -p phost -- port native strlen 61626300:0400000000000000
cargo run -p phost -- port native strrchr 616261626300:62:0600000000000000
cargo run -p phost -- port native toupper 61 --no-capability   # → foreign fallback
cargo run -p phost -- port compose                             # composition court
cargo run -p phost -- port compose 614263:62:0300000000000000   # one composed call
cargo run -p phost -- port compose --target toupper_strlen_memchr   # the 3-stage chain
cargo run -p phost -- port compose --target toupper_strlen_memchr 62006161:41:0400000000000000
cargo run -p phost -- port compose --target toupper_strlen_memchr_pair   # the 2-search chain
cargo run -p phost -- port compose --target toupper_strlen_memchr_pair 61006262:41:42:0400000000000000
cargo run -p phost -- port compose --target toupper_each                # the map port
cargo run -p phost -- port compose --target toupper_each 616263:0300000000000000
cargo run -p phost -- port compose --target toupper_each_strlen_memchr  # the nested chain
cargo run -p phost -- port compose --target toupper_each_strlen_memchr 62006161:41:0400000000000000
cargo run -p phost -- port compose --target toupper_memchr_suffix       # the slice chain
cargo run -p phost -- port compose --target toupper_memchr_suffix 62617862:61:62:0400000000000000
cargo run -p phost -- port compose --target toupper_each_slice_search   # the composition-consumes-buffer chain
cargo run -p phost -- port compose --target toupper_each_slice_search 62617862:61:62:0400000000000000
./verify_jit_porting_court.sh --target toupper             # determinism court
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memchr
./verify_jit_porting_court.sh --target strlen
./verify_jit_porting_court.sh --target strrchr
./verify_jit_porting_court.sh --target strspn                 # the POSIX dialect
./verify_jit_porting_court.sh --target memchr --check-committed   # fresh == checked-in
./verify_composition_court.sh                     # composition #1 court
./verify_composition_court.sh --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr              # composition #2
./verify_composition_court.sh --target toupper_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr_pair         # composition #3
./verify_composition_court.sh --target toupper_strlen_memchr_pair --check-committed
./verify_composition_court.sh --target toupper_each                       # composition #4 (map)
./verify_composition_court.sh --target toupper_each --check-committed
./verify_composition_court.sh --target toupper_each_strlen_memchr         # composition #5 (nested)
./verify_composition_court.sh --target toupper_each_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_memchr_suffix              # composition #6 (slice)
./verify_composition_court.sh --target toupper_memchr_suffix --check-committed
./verify_composition_court.sh --target toupper_each_slice_search         # composition #7 (derived buffer)
./verify_composition_court.sh --target toupper_each_slice_search --check-committed
./verify_store.sh                                                          # persistent store
./verify_session.sh                                                        # sealed native service
./verify_cross_implementation.sh                                           # the implementation axis
```

Verified: the seals bind the **qualified target id** (`libc:memcmp:c-locale:sign:v1`),
the **locale contract** (`C`), the **candidate behavior hash**, and the compiled
candidate artifacts — **source hash**, **ELF64 object hash**, **receipt hash**,
and **compiler version**. So a locale change, an edited `.phor` candidate, or a
recompiled object invalidates the seal.

The **compiled `.phor` object is the authoritative promoted implementation**, and
it is **loaded and executed**: the court invokes `phorc` on the target's `.phor`
source, hashes the emitted object and its receipt file, then maps the sealed ELF64
object, locates the ABI entry symbol (`_phor_phor_toupper` / `_phor_phor_memcmp_sign` /
`_phor_phor_memchr_index` / `_phor_phor_strlen_len` / `_phor_phor_strrchr_index`),
rejects any entry point with relocations or external symbols, and replays the exact
same corpus through the compiled code. Promotion now requires *both* the replay
court and the execution court to match. The verifier independently recompiles and
confirms the object/receipt hashes match the committed seal (`Compiled object:
MATCH`) and that the executed object is the sealed object (`Execution object:
MATCH`). Evidence is committed under
`phost/evidence/porting/{toupper,memcmp,memchr,strlen,strrchr}/` including
`execution_verdict.json`; the compiled `candidate.o` **is committed** (it is the
seal, and the store loads it), while `candidate.receipts.json` is regenerated.

Executed ABI (SysV AMD64, leaf/pure integer functions only):

| Entry symbol | Signature | Result |
|--------------|-----------|--------|
| `_phor_phor_toupper` | `(u64 byte) -> u64` | folded byte in the low 8 bits |
| `_phor_phor_memcmp_sign` | `(u64 wa, u64 wb, u64 n) -> u64` | `i64` sign `-1 \| 0 \| 1`; buffers packed big-endian |
| `_phor_phor_memchr_index` | `(u64 wh, u64 needle, u64 n) -> u64` | `i64` index or `-1`; haystack packed little-endian |
| `_phor_phor_strlen_len` | `(u64 w, u64 n) -> u64` | `u64` length (first NUL index), `n` when out of contract; buffer packed little-endian |
| `_phor_phor_strrchr_index` | `(u64 w, u64 needle) -> u64` | `i64` index of the last in-string match or `-1`; buffer packed little-endian and zero-extended |

At a call site the **runtime dispatcher** (`phost::porting::dispatch`) looks the
target up in the capability-gated sealed store and, if a sealed entry is present,
verifies the object hash, maps the entry function **once** and calls it. With no
usable sealed artifact the call reports a **foreign fallback** so the caller uses
the foreign implementation — except when a *sealed* entry exists but fails
verification, which is terminal (fail closed) and never falls back. The dispatch
court replays the whole corpus through this path and requires every case to be
served natively with zero fallbacks.

The verifier has two courts: the default regenerates the evidence twice and
requires byte-identical runs; `--check-committed` writes only to a temp dir and
compares against the checked-in evidence without touching it (reviewer-grade).
An unsupported candidate fails closed (`UnsupportedTarget`) and can never pass as
an identity transform; malformed arguments fail closed too.

This is **API-surface** JIT-porting, not arbitrary binary translation — eager JIT
of arbitrary foreign binaries is a later phase. The execution court deliberately
executes **only** leaf, pure, relocation-free integer functions: no dynamic
linker, no heap, no syscalls, no arbitrary binary translation. It is a proof that
the promoted artifact itself runs, not a general execution engine.
See `docs/REPLAY_COURTS.md` and `docs/PHORENSIC_OS.md`.

## Reproducible runs with Docker

Two minimal, resource-capped containers (QEMU boots inside the kernel one):

```sh
docker compose run --rm host     # cargo test + the store, session, court and composition verifiers
docker compose run --rm kernel   # build kernel + QEMU boot + evidence verify
docker compose up --build        # both, one shot
```

`host` runs the compiler/runtime tests and the court verifiers (`verify_store.sh`,
`verify_session.sh`, `verify_cross_implementation.sh`, `verify_jit_porting_court.sh`,
`verify_composition_court.sh`).
`kernel` builds the
Multiboot kernel, boots it under QEMU, verifies the boot evidence (13 checks) and
checks the boot evidence is byte-reproducible against the committed
`phost_kernel/evidence_manifest.json`. Boot evidence is byte-identical across
hosts and containers (the manifest's `asserted_reproducible_evidence`); kernel
*image* bytes additionally depend on the linker toolchain
(`nasm`/`ld.lld`/binutils), so they are recorded under
`observed_toolchain_bound_build` with `asserted_reproducible: false`.

Public CI runs exactly these two commands on a clean checkout: the
[`ci` workflow](.github/workflows/ci.yml) builds and runs the `host` and `kernel`
services on every push to `main`, every pull request, and on manual dispatch. There
is no GitHub-specific logic — the badge above reflects the same containers a
reviewer runs locally, so the external and local results cannot drift.

## Current status

All numbers below were reproduced on a clean checkout.

| Check | Result |
|-------|--------|
| `cargo test` (phorc) | **50 / 50 pass** (44 unit + 6 lowering-integration) |
| `cargo test` (phost) | **265 pass, 0 fail, 4 ignored** (the 4 ignored read privileged CR0/CR2/CR3/CR4 and require ring 0) |
| `.phor` / `.ph` → ELF64 | every corpus source emits a non-empty object: **369 / 369, 0 failures** (`src/` 232, `examples/` 51, `tests/` 83 incl. 46 `compile-pass`, `fixtures/` 2, `README.phor`) |
| Compiler pipeline | `hello.phor` → 6960-byte ELF64 relocatable + receipts + sealed package |
| Seal verification | source hash **MATCH**, object hash **MATCH** |
| Court replay | 6 / 6 phases **PASS**, verdict `consistent` |
| Kernel | builds a valid Multiboot v1 image (magic `02 b0 ad 1b`) |
| Kernel boot (QEMU) | `Ph` on COM1 and `0xE9`; 1024×768 boot GUI rendered to the LFB |
| Boot evidence | `verify_evidence.sh`: **13 / 13 checks pass**; manifest committed; evidence byte-reproducible |
| JIT-porting court | `toupper`: **256/256**, `memcmp`: **312/312**, `memchr`: **482/482**, `strlen`: **308/308**, `strrchr`: **336/336**, `strspn` (POSIX): **578/578**, hashes MATCH, source+object+receipt bound, promotion `Sealed` |
| Sealed-object execution | the sealed ELF64 object is loaded and called: `toupper` **256/256**, `memcmp` **312/312**, `memchr` **482/482**, `strlen` **308/308**, `strrchr` **336/336**, `strspn` **578/578**; executed object == sealed object |
| Sealed native dispatch | the runtime serves calls from the sealed object: `toupper` **256/256 native**, `memcmp` **312/312 native**, `memchr` **482/482 native**, `strlen` **308/308 native**, `strrchr` **336/336 native**, `strspn` **578/578 native**, **0 fallbacks / 0 broken seals**; no capability → foreign fallback |
| Second dialect | `posix:strspn:c-locale:u64:v1` — a POSIX contract (ISO C does not specify `strspn`) and a new observable shape (a prefix length decided by set membership); the verifier requires the sealed package's `dialect` to match the target id's namespace; see `docs/DIALECT_QUALIFICATION.md` |
| Sealed composition | `toupper ∘ memchr` composed from two sealed ports: **560/560**, all three stages native, **0 fallbacks / 0 broken seals**, 4776 sealed dispatches; dispatched objects match the committed leaf seals |
| Sealed composition (3-stage) | `toupper ∘ strlen ∘ memchr`, where the sealed `strlen` result becomes the search bound: **350/350**, all four stage-accounts native, **0 fallbacks / 0 broken seals**, 3729 sealed dispatches; dispatched objects match the committed leaf seals |
| Sealed composition (dataflow) | `toupper ∘ strlen ∘ memchr ∘ toupper ∘ memchr`, where **one derived bound is consumed by two searches**, the second non-adjacent to the stage that produced it: **474/474**, all six stage-accounts native, **0 fallbacks / 0 broken seals**, 6102 sealed dispatches; dispatched objects match the committed leaf seals |
| Composition as a sealed port | the store publishes two artifact kinds (`LeafObject` and `Composition`); a nested chain resolves the sealed **composition** `toupper_each` from the store, checks its seal and recurses: `toupper_each` **267/267** (311 dispatches) and `toupper_each_strlen_memchr` **350/350**, all four stage-accounts native, **0 fallbacks / 0 broken seals**; the recorded nested seal matches the committed `toupper_each` chain hash |
| Derived value selects a buffer | `toupper ∘ memchr ∘ slice ∘ memchr`: the folded haystack is sliced at the origin the first `memchr` derived, and the second search reads that slice — **688/688**, 0 fallbacks / 0 broken seals, 7654 dispatches; the second search is data-dependent (ran 434, never reached 254, the two accounts covering every case); the `B.` group of 28 cases has needleB **before** the origin and must return -1; `chain_hash` covers the origin and the suffix offset |
| Composition consumes a derived buffer | `toupper_each ∘ memchr ∘ slice ∘ toupper_memchr`: a sealed **composition consumes a buffer another composition selected** — `toupper_each` folds the haystack, a leaf derives the origin, and `toupper_memchr` consumes the derived slice, folding it itself because the slice is taken unfolded: **688/688**, 0 fallbacks / 0 broken seals, 2498 dispatches; the consumer composition is data-dependent (ran 434, never reached 254); both nested seals (`196940c2…` `toupper_each`, `d00bdf26…` `toupper_memchr`) match the committed inner chain hashes; the oracle hash is identical to the sixth chain's (`8c354e38…`), a controlled experiment |
| Compiled candidate authority | `phorc` compiles each `.phor` candidate; object/receipt hashes match an independent recompilation and a fresh container run |
| Persistent sealed port store | `phost/evidence/store/index.json` commits all **13** sealed ports (6 leaf object hashes + 7 composition chain hashes); loading verifies every object's bytes against its seal and fails closed on a missing/broken entry, and rejects a composition cycle; the composition court reproduces its committed verdict from the store with an impossible `--phorc` path, and a fresh index from committed evidence is byte-identical |
| Sealed native service | one verified store load serves many consumers: **13 ports from 6 mapped objects**, **68** sealed-port resolutions including nested stages, **0 fallbacks / 0 broken seals**; `toupper` fan-in 39, `memchr` 10, `strlen` 4, `toupper_each` 5, `toupper_memchr` 2; the session reproduces from a copy of the store at another path, a missing store fails closed, and without `PORTING` the store is never read |
| Cross-implementation | every sealed leaf observed through a **second, independent implementation** (musl, statically linked, `libc=musl` from its own check, no `PT_INTERP`): `toupper` 256, `memcmp` 312, `memchr` 482, `strlen` 308, `strrchr` 336, `strspn` 578 — **2272 cases, 0 disagreements**; `secondary_oracle_hash == primary_oracle_hash ==` the committed sealed oracle hash; agreements on a bounded corpus are evidence, not proof |
| Docker | `docker compose run --rm host` / `kernel` reproduce the tests, the store, the courts, and the QEMU boot |
| Foundry baseline (Phase 0) | the four-repository epistemic stack (phorensicos, `frf` 0.1.86, `frf-fuzz` 0.8.0, `gemel` 0.11.1) is pinned in `foundry/baseline/dependency_pins.json` and sealed by `foundry/baseline/integration_baseline_receipt.json`; `scripts/integration_baseline.sh` derives the receipt from the executable courts and verifies the external pins when their working copies are present; see `docs/AUTONOMOUS_PORTING_ARCHITECTURE.md` |
| Canonical `PortSpec` (Phase 1) | the typed port specification replaces the stringly `PortTarget`: `ContractSource` separates the contract from the implementation observed, `ObservableSpec`/`ObservationProjectionSpec` make normalisation explicit, `PreconditionSpec` is machine-checked (`validate_case` → `ValidatedCase`; only a validated case reaches the foreign oracle), and `PortSpecId = SHA-256("PHOR/PORTSPEC/v1\0" ‖ canonical_bytes)` is a domain-separated content identity; the generic machinery is registry-driven (`registry.rs`: spec + CaseGenerator + candidate adapter + ABI adapter), so a new leaf target needs no engine change, and a static audit fails the build if a `.id ==` target branch reappears; see `docs/PORT_SPEC.md` |
| Composition IR (Phase 2 core) | `composition_ir.rs` makes composition **data**: a typed, bounded, acyclic `CompositionIR` (SSA-like values; `Call`/`MapBytes`/`Slice`/`Compare`/`Select`/`FoldBytes`) with a domain-separated `CompositionIrId` and **one** `eval` over a `PortBackend`, so the oracle side and the sealed side run the same graph and cannot drift; a test expresses `toupper ∘ memchr` as IR and reproduces the foreign oracle over the whole corpus; see `docs/COMPOSITION_IR.md` |

### Known gaps

- **Checker depth.** The bootstrap checker reports diagnostics for constructs
  outside its current subset (e.g. `Option`/`Result` as bare enums, some
  top-level forms) while still lowering them. It is a working front end, not a
  finished type system.
- **Backend maturity.** Register allocation, memory operands and the call ABI
  are minimal; codegen focuses on the boot/runtime path.
- **Interactive runtime.** The boot path renders the GUI surface and halts;
  the interactive shell/presentation loop is future work.
- **`.phor` ↔ Rust convergence.** Drivers/compositor exist both as `.phor`
  sources and as `phost` Rust modules; unifying them is in progress.
- **Porting scope.** The JIT-porting court covers API-surface (byte-in/byte-out)
  targets only; arbitrary binary translation is not implemented.
- **Store index upkeep.** `phost/evidence/store/index.json` is committed and
  regenerated explicitly (`phost port store --write`), as the leaf `candidate.o`
  files and composition verdicts change. It is not rebuilt automatically, so a
  stale index is a real possibility — `verify_store.sh` fails closed on one
  (object hash mismatch, dangling stage, or a regeneration diff).
- **Kernel image bytes.** Boot evidence is byte-reproducible, but kernel image
  bytes depend on the linker toolchain; cross-toolchain bit-reproducibility is
  not yet asserted.

## Documentation

- `docs/PHORENSIC_LANGUAGE.md`, `docs/PHORENSIC_GRAMMAR.md` — the language.
- `docs/DIALECT_QUALIFICATION.md` — how a `dialect:` namespace is earned (`posix` vs `libc`).
- `docs/PHORENSIC_COMPILER.md` — pipeline and artifact tiers.
- `docs/FORENSIC_STORE.md`, `docs/REPLAY_COURTS.md` — evidence store and courts.
- `docs/PHORENSIC_OS.md`, `docs/FORENSIC_OS_VISION.md` — the OS.
- `docs/AUTONOMOUS_PORTING_ARCHITECTURE.md` — the autonomous foundry: invariants, phase plan, acceptance gates.
- `docs/PORT_SPEC.md` — the canonical typed `PortSpec`: content identity, preconditions, the six pinned leaf specs.
- `docs/COMPOSITION_IR.md` — the typed `CompositionIR`: nodes, validation, content identity, the generic interpreter.
- `foundry/` — the foundry baseline (`dependency_pins.json`, the integration baseline receipt).
- `docker-compose.yml`, `docker/` — reproducible host + kernel containers.
- `docs/REVIEWER_PROTOCOL.md` — how to review claims in this repo.
- `VERIFICATION_REPORT.md` — detailed build/boot verification notes.

## License

Licensed under either of **MIT** or **Apache-2.0**, at your option
(`LICENSE-MIT`, `LICENSE-APACHE`).
