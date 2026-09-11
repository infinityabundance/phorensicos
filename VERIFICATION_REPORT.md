# Phorensic OS — Verification Report

## Status: Bootstrapping / Observed

### Boot Status
| Aspect | Status | Notes |
|--------|--------|-------|
| ASM stub → UEFI GOP | ✅ | Boot params passed, framebuffer info available |
| StatusScreen | ✅ | Boot phases, progress bar, log messages operational |
| Canvas module | ✅ | Shapes, text, compositing primitives |
| Scrolling Console | ✅ | Text console on framebuffer |
| Minimal Shell | ✅ | Command support on console |
| UART Serial Driver | ✅ | Capability model, COM1–COM4 support |
| End-to-end boot path | ✅ | ASM → GOP → StatusScreen → Canvas → Console → Shell |
| **Booted GUI Runtime (QEMU)** | ✅ | **Real framebuffer boot**: Bochs-VBE LFB → long mode → IDT → `.bss` → FB ABI → kernel_main → allocator → Canvas → Compositor window → boot GUI surface → proof bytes to serial + debug ports** |

### Build Status
| Component | Status | Notes |
|-----------|--------|-------|
| `phorc` (Rust compiler) | ✅ Builds | 0 errors, 0 warnings |
| `phost` (kernel runtime) | ✅ Builds | 0 errors (pre-existing `static mut` reference lints) |
| All tests (phost) | ✅ 265 pass, 4 ignored | 269 total: 208 porting + 49 nucleus + 8 drivers + 4 kernel (the 4 ignored are the ring-0 control-register reads) |
| All tests (phorc) | ✅ 50 pass | 44 unit (parser/checker/lower/codegen) + 6 integration lowering regressions |
| Full pipeline (`.ph` → ELF64) | ✅ Works | lex → parse → check → lower → codegen → emit |
| `canvas` module | ✅ | Shapes, text, compositing primitives |
| `console` module | ✅ | Scrolling text console on framebuffer |
| `shell` module | ✅ | Interactive shell with keyboard input |
| `serial` driver | ✅ | UART serial driver with capability model |
| `status_screen` | ✅ | Boot phase display, progress bar, log messages |
| `input/keyboard` | ✅ | PS/2 keyboard driver with scancode→ASCII |
| `.phor` corpus | ✅ 369/369 → ELF64 | `src/` 232 + `examples/` 51 + `tests/` 83 (incl. 46 `compile-pass`) + `fixtures/` 2 + `README.phor`, all emit non-empty objects (checker diagnostics may be emitted; see Known Gaps) |
| `compositor` module | ✅ | Window manager, surface blitting to canvas |
| `phorc_bridge` | ✅ | Compiler invocation from shell with result parsing |
| `pub` visibility tracking | ✅ | FnDecl/StructDecl/EnumDecl/ImplBlock/ConstDecl |
| Register allocator | ✅ | x86-64 register allocator (deterministic, spill-correct) in codegen |
| Sealed-object execution | ✅ | `exec.rs` loads the sealed ELF64 object, verifies its hash, maps it and calls the ABI entry |
| Sealed native dispatch | ✅ | `dispatch.rs`: the runtime prefers the sealed object at a call site; broken seals fail closed, no capability → foreign fallback |
| Cross-implementation court | ✅ | `cross_impl.rs`: the same sealed corpus observed through a second, independent implementation (musl via a statically linked probe); all six leaves agree, 2272 cases / 0 disagreements |

### Compiler Pipeline
```
.ph source → lex → parse → check → lower(PHIR) → x86-64 select → encode → ELF64 .o + .receipts
```

### Compiler Modules
| Module | Status | Description |
|--------|--------|-------------|
| `lex` | ✅ | Full Phorensic token set |
| `parse` | ✅ | Functions, structs, enums, types, expressions, statements |
| `check` | ✅ | Symbol table, type checking, capability tracking, effects, loop bounds, handles |
| `ir` (PHIR) | ✅ | Mid-level IR: blocks, ops, types, values |
| `lower` | ✅ | AST → PHIR: nested blocks, early returns, break/continue propagation |
| `codegen/x86_64` | ⚠️ | x86-64 instruction encoding via iced-x86: prologue, epilogue, binops |
| `codegen/object` | ⚠️ | ELF64 object writer via `object` crate: .text/.data/.bss |
| `receipts` | ⚠️ | Byte attribution, function receipts, compilation residuals |

### Known Gaps
1. **Backend maturity**: register allocation is now spill-correct for the subset exercised by the porting courts, but memory operands and the general call ABI remain minimal
2. **GUI compositor chain**: window manager renders to the real QEMU LFB at boot; interactive shell/presentation loop still pending
3. **Documentation**: CORPUS.md reference files do not exist
4. **`.phor`→Rust convergence**: serial, canvas, compositor, input handler, porting engine exist in `.phor` (examples/) but not yet integrated as system `.phor` modules
5. **Input handling**: keyboard scancode→ASCII works in Rust and .phor, but mouse/touch not yet supported
6. **Native execution**: the sealed ELF64 candidate objects are loaded and executed (leaf, relocation-free integer functions only). There is no dynamic linker, relocation patching, heap or syscall path yet, and no general binary translation
7. **Control-flow lowering**: `phorc`'s lowerer still emits a single basic block (no branch/loop lowering), so the porting candidates are written branchless. Branch-target patching is a later phase

### Next Build Target
```
GUI compositor → window manager → surface management → inspector
(Boot path, StatusScreen, Console, Shell, Canvas, Serial, Keyboard now operational)
```

### Recent Milestones (parse26)
| Milestone | Detail |
|-----------|--------|
| **Booted GUI Runtime Court** | **QEMU boot renders the full boot surface to the real VGA LFB: dark background, accent bars, title/subtitle, 9-phase boot table, compositor "Boot Console" window (focused title bar + separators + accent blocks), status bar; `Ph` proof bytes on COM1 + 0xE9** |
| Boot fix — stack placement | Kernel stack moved from 0x100000 (inside the BIOS ROM shadow 0xF0000-0xFFFFF, where writes are dropped) to 0x90000 (conventional RAM) |
| Boot fix — FB ABI | ABI write moved after `.bss` zeroing (`.bss` 0x154000-0x9571A8 covered the old 0x300000 ABI location) |
| Boot fix — identity map | PDPT[1..3] now point at PDs so the full 4 GB is mapped (framebuffer at 0xFD000000 is at ~3.9 GB); PD area zeroed so 32-bit entry writes don't leave reserved-bit garbage in high dwords |
| Boot fix — debug port asm | `write_debug` uses `out 0xE9, al` (immediate form) so the asm can no longer clobber live `edx` values (the boot bpp was coming back as 0x1e) |
| Boot fix — compositor z-order | `render_boot_surface` renders the compositor first (its `render()` clears the canvas), then paints the boot GUI on top |
| Capability-gated store | `lookup_gated()` requires DRIVER_LOAD bit; `can_load()` checks trust >=4 |
| Verified loading | `load_verified()` gates on DRIVER_LOAD, verifies store, then loads |
| Z-order compositing | `render_zordered()` sorts by z_order, AABB damage intersection |
| Single-surface render | `render_surface_to_canvas()` for independent surface redrawing |
| `.phor` live revocation | Driver with periodic trust checks, drift detection, bounded recovery |
| `.phor` full port demo | 6-stage toupper port: observe->analyze->spec->implement->verify->promote |
| `.phor` corpus | 369/369 files lower to non-empty ELF64 objects |
| Keyboard→compositor routing | Focus-aware input dispatch, Tab focus cycling |
| Self-consuming impl methods | `ReturnType::SelfConsuming` pattern for builder-style methods |
| `residual emit` checker | Type-checking for residual emit field expressions |
| phost reach | 331 tests, loader, compositor, phorc_bridge, keyboard, serial, canvas, shell, JIT-porting court (toupper + memcmp + memchr + strlen + strrchr + `strspn`, the last recorded under a historical `posix:` identity and corrected by a `libc:strspn` successor via evidence-preserving supersession) + sealed-object execution + sealed native dispatch + seven sealed composition courts (incl. nested ones, a buffer-slicing one and one where a composition consumes a derived buffer) + persistent sealed port store + sealed native service + cross-implementation court + canonical registry-driven `PortSpec` + typed `CompositionIR` and one generic IR composition engine on the runtime path + the court-sensitivity (challenge) court + the autonomous identity namespaces, evidence closure, oracle witnesses, `AUTONOMOUS-SEAL/v1` obligation profile, immutable store generations, the demand model, bounded `CompositionIR` synthesis, the guarded-arena memory-effect court and the contract-provenance supersession record |

### JIT-Porting Court
| Aspect | `toupper` | `memcmp` | `memchr` | `strlen` | `strrchr` |
|--------|-----------|----------|----------|----------|-----------|
| Qualified target id | `libc:toupper:c-locale:u8:v1` | `libc:memcmp:c-locale:sign:v1` | `libc:memchr:c-locale:index:v1` | `libc:strlen:c-locale:u64:v1` | `libc:strrchr:c-locale:index:v1` |
| Corpus | exhaustive `0x00..=0xff` (256) | bounded deterministic (312) | bounded deterministic (482) | bounded deterministic (308) | bounded deterministic (336) |
| Dialect cage observation | ✅ 256/256 | ✅ 312/312 | ✅ 482/482 | ✅ 308/308 | ✅ 336/336 |
| Replay court | ✅ 256/256, `consistent` | ✅ 312/312, `consistent` | ✅ 482/482, `consistent` | ✅ 308/308, `consistent` | ✅ 336/336, `consistent` |
| Contract | `u8` (C locale) | sign `(-1 \| 0 \| 1)`, unsigned, n-bounded | first-match index or `-1`, unsigned, n-bounded | first-NUL length, `u64`, n-bounded (fail closed at `n`) | last in-string match index or `-1`; NUL needle → length |
| Oracle hash | `8daf25ed2482b1258bdef6b618ea2c2ef17c22ef45b47ba3a45ded2cfc05ac91` | `918c86d5302aaa0394a782b68e4aed7494549bcaaf06a82948c6d945ccb43210` | `bd89e67c4d7cd22d5006c1c3a7c3759dd47a218de9f977f2f8db549b77d43498` | `6a0b3f237e0346d3556f6f68828ce6ad723a8e62f7d28108d0031dc8dc4d580e` | `e868e2831c6ad8ce71e0c753f85543b8660ead3b04f112a7edb93cc8f3798bbe` |
| Candidate behavior hash | `777f11a0264a69b20f5c8a87d9c0687768d3e11216a43dd95698b5dd119fefc2` | `e7fcf296920ea7a179a218202a059665dab2ee6cd38ade094696b44b19553033` | `5e987fae450eb623a749e2b662b84c59ad390fb2eade75b728ae86399687cba5` | `93324220b42484122fbd804533318c17f01d7cedbae7bb3a50d1493dfa9d2ffe` | `c184e068075c407ba8ef7987c9a613dfdd03aa9c6e5292a9e3f1fbf1dc6d56ae` |
| Candidate source hash | `a23bea4da98b4526635d295b6f71f1dbfc14e1a219fb2849d94ee89ca80a37b8` | `adcb65fbd2e54a04c78ed019947d157d0935247b69be828ceb0702af75088a0c` | `ccce8c195bb826866e5505aebbc9a4e38334f58529b2f80a3777e959747e23ec` | `d508323ce7cb44f944c0d748f1cea250f29b49d7a5a29e9a281e6de33dd0a14e` | `30c1dd887cfab48f75662cd88f45a04dba9f78543134e6ea18eec506f994284f` |
| Candidate object hash | `05c175a89a25d339f22793193860045e94cdf5c4a2f85cfba3dea3677ab4e8d4` | `23ae1e515557ab9449838ad1f72666e70d7ff9f689acf306877a9f3a0a1bd854` | `90d35156eef8b7009352ebb0ac6fa0f9137c95a515ae1a1e760a37147032bbb8` | `ffb0f5699df2455d6f1597c3bb2bbf1d94596cd772d2fd4b84d6036b99967b94` | `212e69a95f783ecab83f334678969752128eb2f152e9fa09951458564c83a4a2` |
| Candidate receipt hash | `614fd9a0860eeee816512a26320001a24217fcfaf2c37344d2a528ca24133485` | `83f0698ed7c1d684be3b0522cfa0e816e00a2708a9f67acb295e3d68c6446aa7` | `e09e4c739b32f9287e27dc4f91f4a3b2c1f6b346eb7d53b060600bf66680c51e` | `d5fa62f3dffc7b1c689e87044527e7970d002350d01bd373503c33616ccfefa7` | `a222e015f060e81ca497099f2ff25c6859916ff075b7a3459acf29db0bc5fc1f` |
| Compiler | `phorc 0.1.0` | `phorc 0.1.0` | `phorc 0.1.0` | `phorc 0.1.0` | `phorc 0.1.0` |
| Replay residual | `c5a4a2b88a72e56c3a7ea6d1a248723a337a38cca9f001e50f868e37abaed349` | `98843827cbbf97866221127651840eaaee6dbb17dc1bf3e0053476ce30a8290d` | `8855cf626df5d8e6e75520f80149938a00505596b080bbb2105ba79b13599c66` | `e942f6fd7de3d654fe8063d097d7040a41b54e9ee9d903e7c97386be6502b8fc` | `dff938d2b303ac48815587c40f0d544629fe89d3a5c59107fd07bbea68bdde53` |
| Executed ELF symbol | `_phor_phor_toupper` | `_phor_phor_memcmp_sign` | `_phor_phor_memchr_index` | `_phor_phor_strlen_len` | `_phor_phor_strrchr_index` |
| Execution court | ✅ 256/256, `consistent` | ✅ 312/312, `consistent` | ✅ 482/482, `consistent` | ✅ 308/308, `consistent` | ✅ 336/336, `consistent` |
| Dispatch court | ✅ 256/256 native | ✅ 312/312 native | ✅ 482/482 native | ✅ 308/308 native | ✅ 336/336 native |
| Dispatch hash | `a3f7c0606ae6b6da6353c1b532957dbe40022e206cb059c1512620ecaa0df90d` | `fb00385a5ecaf78c2031d25d7d6c98b7a040d44c53fd12579fa6931394387231` | `a611cc809aa507350fd6b6aba824f681a5e3e977c0fcd7eb8300f58ed995f3d7` | `fd8f09df324dd6feaa111e5c8d79c0ae5d446fa5e62d36d88251ff29180b3ae2` | `c14ffa2bb564d385cf660431a7bbf9dbd61fabec552e399d881b0282edc15b9b` |
| Promotion | ✅ → `sealed` | ✅ → `sealed` | ✅ → `sealed` | ✅ → `sealed` | ✅ → `sealed` |
| Sealed package | `native:libc:toupper:c-locale:u8:v1` | `native:libc:memcmp:c-locale:sign:v1` | `native:libc:memchr:c-locale:index:v1` | `native:libc:strlen:c-locale:u64:v1` | `native:libc:strrchr:c-locale:index:v1` |

For every target the **execution behavior hash equals the candidate behavior hash**: the
Rust mirror and the compiled object reproduce the identical output for every case,
computed through different code paths (slice-based vs packed-word).

Shared properties (all targets):
| Aspect | Result |
|--------|--------|
| Compiled candidate authority | ✅ candidate.o is the promoted implementation; object + receipt hashes bound |
| Independent recompilation | ✅ verifier recompiles the `.phor` source; object/receipt hashes MATCH the seal |
| Sealed-object execution | ✅ object hash verified against the seal, ELF64 symbol located, executed; every case matched |
| Sealed native dispatch | ✅ every case served from the sealed object through the runtime dispatcher; 0 foreign fallbacks, 0 broken seals |
| Dispatch fail-closed | ✅ a sealed entry whose object does not verify is counted as a broken seal (terminal), never as a foreign fallback; no capability / no sealed entry → foreign fallback |
| Leaf/relocation guard | ✅ entry with an undefined symbol or a relocation in its range is rejected (fail closed) |
| Determinism court | ✅ two fresh runs byte-identical (8/8 artifacts) |
| Committed-evidence court | ✅ `--check-committed`: fresh run == checked-in evidence (8/8) |
| Cross-environment | ✅ committed evidence matches a fresh container run (8/8), incl. object hashes |
| Tamper detection | ✅ editing the `.phor` candidate invalidates the seal; locale is hash-covered; a mutated sealed object fails the pre-execution hash check |
| Capability gating | ✅ `PORTING` required to observe, to promote, and to execute |
| Fail-closed candidate | ✅ unknown target id → `UnsupportedTarget`; malformed args → `MalformedArgs`; missing object/receipt hash blocks promotion; inconsistent execution blocks promotion |

Compiled candidate: the court invokes `phorc` on the target's `.phor` source (from
the workspace root, with the repo-relative path so the ELF `FILE` symbol is
environment-independent), then binds SHA-256 of the emitted ELF64 object and of
its receipt file. To make this possible, `phorc` register allocation was made
deterministic (it iterated a `HashMap`; now a `BTreeMap`), so object bytes are
reproducible across runs and environments.

Sealed-object execution: the object is then **loaded and executed**. The execution
court verifies the object's SHA-256 against the seal before use, parses the ELF64
sections/symbol table, locates the ABI entry symbol (`_phor_phor_toupper` /
`_phor_phor_memcmp_sign` / `_phor_phor_memchr_index` / `_phor_phor_strlen_len` /
`_phor_phor_strrchr_index`),
rejects an entry whose byte range contains a relocation
or which is an undefined/external symbol, maps `.text` read-only/executable, calls
the function through an explicit SysV integer ABI harness, and replays the exact
same corpus through the compiled code. Promotion requires both the replay court
and the execution court to be `consistent` and to reference the same object hash.
Scope: leaf, pure, relocation-free integer functions only — no dynamic linker, no
relocation patching, no heap, no syscalls.

`memcmp` corpus axes: lengths `0..=8`; patterns zero/ones/ascending/descending/
alternating; every first-mismatch position with both orderings; the `n`-boundary
around a mismatch (`n = j` excludes it, `n = j+1` includes it); and the unsigned
edge bytes `00/01/7f/80/fe/ff`. Every case satisfies `n <= min(len(a), len(b))`.

`memchr` corpus axes: lengths `0..=8`; the first match at every index (on
distinct-byte patterns); repeated needles (first occurrence wins); an absent
needle; the `n`-boundary around a match; the unsigned edge bytes; and an
exhaustive sweep of all 256 needle values. Every case satisfies `n <= len(hay)`.

`strlen` corpus axes: the complete `(k, n)` grid of terminator index `k` and scan
bound `n` with `0 <= k < n <= 8` (36 cases); non-NUL filler bytes at every prefix
position; tails after the terminator that are themselves NUL (the *first* NUL must
win); buffers longer than the bound; and an exhaustive sweep of all 256 byte
values proving only `0x00` terminates (308 cases). Every case has a NUL inside its
bound, so libc `strlen` is well defined and never reads beyond it.

`strrchr` corpus axes: unique occurrences at every in-string index (28 cases);
repeated occurrences so the last wins (21); the empty string (16); needles that
occur only after the terminator, which must never match (5); needles that occur
both before and after it, where the in-string occurrence wins (3); an exhaustive
sweep of all 256 needle values against a haystack whose tail repeats bytes from
the string; and the unsigned edge bytes with copies of `0x7f`/`0x80` after the
terminator (336 cases total). A needle of `0` yields the terminator index (the
length). Every case satisfies `n <= len(buf) <= 8` and has a NUL inside `buf[..n]`.

Sealed native dispatch: the runtime call site (`phost::porting::dispatch`) looks
the target up in the capability-gated sealed store and, if a sealed entry is
present, verifies the object hash, maps the entry function once and calls it;
otherwise it reports a foreign fallback. A *broken seal* is counted separately
(`broken_seal_cases`) and is terminal — it never falls back. The dispatch court
replays the whole corpus through this path and requires 256/256 (`toupper`),
312/312 (`memcmp`), 482/482 (`memchr`), 308/308 (`strlen`) and 336/336
(`strrchr`) cases served natively with zero fallbacks and zero broken seals;
`dispatch_hash` is bound into the promotion receipt and the sealed package.
Try it: `phost port native toupper 61` (native) and
`phost port native toupper 61 --no-capability` (foreign fallback).

### Sealed Composition Dispatch Court

The leaf courts prove a sealed artifact is correct and preferred. The composition
courts prove sealed artifacts are **runtime building blocks**: composed targets
whose implementation is a chain of already-sealed objects, executed entirely
through `NativeDispatcher` with **no foreign calls in the sealed path** and no Rust
mirror consulted. Two chains are sealed, and the second one generalizes the
machinery in a way the first cannot.

#### Chain 1: `toupper ∘ memchr`

| Aspect | `phor:compose:toupper_memchr:c-locale:index:v1` |
|--------|--------------------------------------------------|
| Stages | `libc:toupper:c-locale:u8:v1` → `libc:memchr:c-locale:index:v1` |
| Oracle | foreign C-locale `toupper` over the haystack and needle, then foreign `memchr` |
| Corpus | 560 cases (the `memchr` corpus + C-locale fold cases) |
| toupper (haystack) stage | ✅ 560/560 native |
| toupper (needle) stage | ✅ 560/560 native |
| memchr stage | ✅ 560/560 native |
| Foreign fallback | **0** |
| Broken seal | **0** |
| Passed / failed | 560 / 0 |
| Sealed dispatches | 4776 |
| Dispatched toupper object | `05c175a89a25d339f22793193860045e94cdf5c4a2f85cfba3dea3677ab4e8d4` (= committed leaf seal) |
| Dispatched memchr object | `90d35156eef8b7009352ebb0ac6fa0f9137c95a515ae1a1e760a37147032bbb8` (= committed leaf seal) |
| Chain hash | `d00bdf2697e24d2aa947cfafb9b37be24e93516d88531cb312fba9f2eafc0c22` |
| Oracle hash | `974140b6e04643eef0ce7a78b5fb604a83201c375d69049c6cbdae667e2d06a9` |
| Verdict | `consistent` |

The composition adds no new trusted code; its `oracle_hash` and `chain_hash` are
bound in `phost/evidence/composition/toupper_memchr/composition_verdict.json`.
`chain_hash` covers the per-stage status, the normalized intermediates and the
final index, so a skipped or fallback stage changes it even if the index matches.
The fold cases (`G.*`) only match *after* `toupper` normalizes both sides, so a
composition that dropped the toupper stage cannot pass.

#### Chain 2: `toupper ∘ strlen ∘ memchr`

The second chain is the proof the machinery generalizes: three stages, and the
middle stage's **result is consumed as the next stage's argument**. The sealed
`strlen` derives the search bound `L`, and the sealed `memchr` searches exactly
that measured prefix instead of the caller's `n`.

| Aspect | `phor:compose:toupper_strlen_memchr:c-locale:index:v1` |
|--------|--------------------------------------------------------|
| Stages | `libc:toupper:c-locale:u8:v1` → `libc:strlen:c-locale:u64:v1` → `libc:memchr:c-locale:index:v1` |
| Oracle | foreign C-locale `toupper` over the haystack, foreign `strlen` → `L`, foreign `toupper` of the needle, foreign `memchr` bounded by `L` |
| Corpus | 350 cases (NUL-terminated strings: folded match at every index, absent needles, needles only after the terminator, terminator-as-needle, edge bytes, exhaustive 0..=255 needle sweep) |
| Data flow | the `strlen` stage's u64 result is the `memchr` stage's `n` |
| toupper (haystack) stage | ✅ 350/350 native |
| strlen (derived bound) stage | ✅ 350/350 native |
| toupper (needle) stage | ✅ 350/350 native |
| memchr stage | ✅ 350/350 native |
| Foreign fallback | **0** |
| Broken seal | **0** |
| Passed / failed | 350 / 0 |
| Sealed dispatches | 3729 |
| Dispatched toupper object | `05c175a89a25d339f22793193860045e94cdf5c4a2f85cfba3dea3677ab4e8d4` (= committed leaf seal) |
| Dispatched strlen object | `ffb0f5699df2455d6f1597c3bb2bbf1d94596cd772d2fd4b84d6036b99967b94` (= committed leaf seal) |
| Dispatched memchr object | `90d35156eef8b7009352ebb0ac6fa0f9137c95a515ae1a1e760a37147032bbb8` (= committed leaf seal) |
| Chain hash | `97fa39998f98f99f2bc130ca8eb6ec1b7cd5805d3c75ada3af742d6c34e28899` |
| Oracle hash | `5f0b62615f9c8fb569de5f6f42e67fb8b7fdc0cdde944497a2683f03a615517f` |
| Verdict | `consistent` |

The chain is a case-insensitive search of a C string: `strlen` establishes the
string's length, so the caller only supplies a precondition bound. Its `chain_hash`
covers the **derived bound** as well as the normalized intermediates, so the hash
proves the chain, not just the answer. The corpus deliberately places needle-like
bytes after the terminator (group `D`), so a chain that searched with the caller's
`n` rather than the derived `L` cannot pass. Bound in
`phost/evidence/composition/toupper_strlen_memchr/composition_verdict.json`.

#### Chain 3: `toupper ∘ strlen ∘ memchr ∘ toupper ∘ memchr` (dataflow, not a pipe)

Chains 1 and 2 are pipelines: each derived value has exactly one consumer, and it is
the stage immediately after the producer. Chain 3 shows the dependency pattern is
not a one-off: **one derived bound is consumed by two searches**, and the second
consumer is deliberately **non-adjacent** to the producer.

| Aspect | `phor:compose:toupper_strlen_memchr_pair:c-locale:index_pair:v1` |
|--------|------------------------------------------------------------------|
| Stages | `libc:toupper:c-locale:u8:v1` → `libc:strlen:c-locale:u64:v1` → `libc:memchr:c-locale:index:v1` (dispatched twice) |
| Oracle | foreign `toupper` over the haystack, foreign `strlen` → `L`, then foreign `memchr` per needle, **both** bounded by `L` |
| Corpus | 474 cases (every ordered pair of in-string match positions, one-present/one-absent, identical needles, tail-only occurrences, edge bytes, exhaustive 0..=255 sweep of needleA) |
| Data flow | `L` (stage 2) is consumed by stage 4 **and by stage 6** — stages 3, 4 and 5 sit between them |
| Observable | the pair `(iA, iB)`, so the runner performs no combining logic of its own |
| toupper (haystack) stage | ✅ 474/474 native |
| strlen (derived bound) stage | ✅ 474/474 native |
| toupper (needle A) stage | ✅ 474/474 native |
| memchr (needle A) stage | ✅ 474/474 native |
| toupper (needle B) stage | ✅ 474/474 native |
| memchr (needle B) stage | ✅ 474/474 native |
| Foreign fallback | **0** |
| Broken seal | **0** |
| Passed / failed | 474 / 0 |
| Sealed dispatches | 6102 |
| Dispatched toupper object | `05c175a89a25d339f22793193860045e94cdf5c4a2f85cfba3dea3677ab4e8d4` (= committed leaf seal) |
| Dispatched strlen object | `ffb0f5699df2455d6f1597c3bb2bbf1d94596cd772d2fd4b84d6036b99967b94` (= committed leaf seal) |
| Dispatched memchr object | `90d35156eef8b7009352ebb0ac6fa0f9137c95a515ae1a1e760a37147032bbb8` (= committed leaf seal) |
| Chain hash | `9bdf4605bcd7fbac85d8bef9bd10050278e6074ff605a4434bcbe0ebad50bfb8` |
| Oracle hash | `5a5c015ee9f6a64e84674cfbbc90be796bb65c22bf11118d3b792d3b5039074d` |
| Verdict | `consistent` |

The decisive case is group `F`: both needles occur **only after the terminator**, so
the single derived bound must exclude the tail for *both* searches — a runner that
kept one search's bound live but re-derived or reused the caller's `n` for the other
cannot pass. `chain_hash` covers the derived bound and both indexes. Bound in
`phost/evidence/composition/toupper_strlen_memchr_pair/composition_verdict.json`.

#### Chains 4, 5 and 6: composition as a first-class sealed port

The first three chains are built from sealed *leaves*. These two close the loop: a
composition is itself a sealed port the store publishes and another chain dispatches.

```text
SealedArtifact::LeafObject   { object_hash, object_path }
SealedArtifact::Composition  { composition_id, chain_hash, leaves }
```

The dispatcher resolves a composition id to a chain runner, checks that the chain's
leaves are themselves sealed in the same store, recurses through the same dispatcher,
and binds the result to the composition's chain hash. An unknown composition id, or a
composition whose leaves are not sealed, is a **broken seal** — never a fallback.

| Aspect | `toupper_each` | `toupper_each_strlen_memchr` |
|--------|----------------|------------------------------|
| Id | `phor:compose:toupper_each:c-locale:u8s:v1` | `phor:compose:toupper_each_strlen_memchr:c-locale:index:v1` |
| Shape | buffer → folded buffer | **nested**: composition → measure → search |
| Stages | `libc:toupper` | `phor:compose:toupper_each` (a composition) → `libc:strlen` → `libc:memchr` |
| Corpus | 267 cases (runs 1..=8, exhaustive 0..=255 sweep, edge/mixed/prefix buffers) | 350 cases (deliberately the same domain as chain 2) |
| Stage accounts | ✅ 267/267 native | ✅ fold(hay) 350, strlen 350, fold(needle) 350, memchr 350 — all native |
| Foreign fallback | **0** | **0** |
| Broken seal | **0** | **0** |
| Passed / failed | 267 / 0 | 350 / 0 |
| Dispatches | 311 | 1400 |
| Chain hash | `196940c211b4f2f57b17c6decb39495e74083714d85cb489d194c879948ad3b4` | `97fa39998f98f99f2bc130ca8eb6ec1b7cd5805d3c75ada3af742d6c34e28899` |
| Oracle hash | `2d41d021953c044cd43e63867d35e2493325f025dd4457ab97c53091955ed01d` | `4c0e5294321a71f54b8fe6e8201684acdc3af2cd294ccf0201ad6a1d6774fe20` |
| Nested seal | — | `fold_composition_chain_hash` = `196940c2…` = the committed `toupper_each` chain hash |
| Verdict | `consistent` | `consistent` |

The fifth chain's `chain_hash` is **byte-identical** to `toupper_strlen_memchr`'s,
because the corpus and oracle are deliberately shared and the two implementations are
behaviorally equivalent over that domain. That equality is not a collision to hide —
it is the court's result: the nested port reproduces the inline chain exactly. The
implementation boundary is recorded separately (`fold_composition_id` +
`fold_composition_chain_hash`), not folded into the behavior hash.

Bound in `phost/evidence/composition/toupper_each/composition_verdict.json` and
`phost/evidence/composition/toupper_each_strlen_memchr/composition_verdict.json`.

#### Chain 6: `toupper ∘ memchr ∘ slice ∘ memchr` — a derived value selects a buffer

The first five chains derive a **scalar**: a length used as a search bound. A bound
narrows how far a search looks, but every stage still reads the same buffer from the
same origin. This chain derives an **origin**: the folded haystack is sliced at the
index the first `memchr` returned, and the second `memchr` searches the suffix.

```text
phor:compose:toupper_memchr_suffix:c-locale:index:v1

  input:  haystack, needleA, needleB, n
  oracle: foreign toupper over the haystack -> H',
          foreign memchr of H' for folded needleA, bounded by n -> i,
          and only if that matched, foreign memchr of H'[i..] for folded
          needleB, bounded by n - i -> j, reporting i + j or -1
```

| Aspect | Result |
|--------|--------|
| Corpus | 688 cases (needleB before the origin, needleB in the suffix, needleA absent, origin at 0, identical needles, the n-boundary, both exhaustive 0..=255 needle sweeps, the edge bytes) |
| Cases passed | **688 / 0 failed** |
| Stage accounts | fold 688 ✔, needleA fold 688 ✔, needleB fold 688 ✔, `memchr` origin 688 ✔ |
| Suffix search | **data-dependent**: ran natively on **434**, never reached on **254** (needleA absent, so no origin and no slice) — the two accounts cover every case |
| Foreign fallback | **0** |
| Broken seal | **0** |
| Dispatches | 7654 |
| Chain hash | `feabc0067db6e908e4a21a0b3b7dbd273529564e849b0657a57ee3b6812bfd80` |
| Oracle hash | `8c354e3801dd656689747aae557ed5eaaf7debce5eb6c0b273bd2fd83192ed9a` |
| Verdict | `consistent` |

The corpus is built so a chain without the slice *fails*, not merely differs:

- Group `B` (28 cases) has needleB at index 0 and needleA later, with no needleB in the
  suffix. The answer is `-1`; a chain that searched the caller's window would find the
  `b` at 0 and return `0`. The tests assert a window-searching chain disagrees with the
  foreign oracle on at least 80 cases.
- Groups `A` (56 cases) place needleB both **before** and **in** the suffix, so the
  sliced answer is `i + (j_in_suffix)` — strictly greater than the window answer.

The `chain_hash` covers the derived origin, the suffix offset and the final index, so a
chain that skipped the slice but landed on the same answer still differs.

Bound in `phost/evidence/composition/toupper_memchr_suffix/composition_verdict.json`.

#### Chain 7: `toupper_each ∘ memchr ∘ slice ∘ toupper_memchr` — a composition consumes a derived buffer

Chains 1–6 read either a caller argument or a slice of a **leaf** fold the runner
performed. This chain adds the last piece of the dataflow vocabulary: a sealed
**composition consumes a buffer another composition selected**.

```text
phor:compose:toupper_each_slice_search:c-locale:index:v1

  input:  haystack, needleA, needleB, n
  sealed: dispatch the sealed COMPOSITION toupper_each over the haystack -> H',
          dispatch the same sealed COMPOSITION over needleA -> a',
          dispatch the sealed memchr over H' for a' -> origin i,
          SLICE the original haystack at i -> S,
          dispatch the sealed COMPOSITION toupper_memchr over S for needleB -> j,
          report i + j or -1
```

Two properties make it a genuinely new shape:

| Property | Result |
|----------|--------|
| Fold stage | the sealed **composition** `toupper_each` — the runner holds no fold logic |
| Consumer stage | the sealed **composition** `toupper_memchr`, handed the derived slice as its *haystack* |
| Slice provenance | taken from the **unfolded** haystack, so the consumer must fold it itself |
| Consumer data-dependent | ran natively on **434**, never reached on **254** (needleA absent) — the two accounts cover every case |
| Replay / execution | **688/688 pass**, 0 fallbacks, 0 broken seals |
| Dispatches | 2498 |
| Chains 6 and 7 | a controlled experiment: same corpus and oracle, so the committed **oracle hash is identical** (`8c354e38…`); the implementation boundary is recorded separately as two nested seals |
| Nested seals | `fold_composition_chain_hash` = `196940c2…` = the committed `toupper_each` chain hash; `search_composition_chain_hash` = `d00bdf26…` = the committed `toupper_memchr` chain hash |
| Chain hash | `e86583c06118728c32ff219d21550554bff33ee7c7ecf000b0e5e596a49ca8c5`, covering the origin and the slice handed to the consumer |

The corpus falsifies the plausible wrong implementation: a chain that hands the raw
(unfolded) slice to a bare search misses every lowercase match in the suffix. The
tests assert at least 50 of the 688 cases discriminate that shape, and the
`toupper_memchr` dispatch is only reachable if the composition is actually consumed —
the verifier cross-checks both nested seals against the committed inner verdicts.

Bound in
`phost/evidence/composition/toupper_each_slice_search/composition_verdict.json`.

#### Generic (IR) composition court — Phase 2

The seven chains above are now also driven by the **generic** court: each is a
typed `CompositionIR` (data) evaluated by one interpreter on both the foreign and
the sealed side. This is the court that would run for a *new* composition, since a
new chain is data rather than a new runner. The v2 evidence lives in
`phost/evidence/composition_ir/<name>/composition_verdict.json` (schema
`…composition_verdict.v2`) and binds four identities:

| Identity | Meaning |
|---|---|
| `composition_ir_hash` | the graph itself (canonical content identity) |
| `dependency_binding_hash` | every dependency + the seal it published |
| `behavior_hash` | normalized observed behavior over the corpus |
| `composition_artifact_hash` | the bound composition artifact |

All seven pass **over the persistent store with no compiler**: e.g. `toupper_memchr`
560/560, `toupper_strlen_memchr` 350/350, `toupper_strlen_memchr_pair` 474/474,
`toupper_each` 267/267, `toupper_each_strlen_memchr` 350/350, `toupper_memchr_suffix`
688/688, `toupper_each_slice_search` 688/688 — 0 fallbacks, 0 broken seals. The two
data-dependent chains report the skipped stage explicitly (`not_reached_cases` 254 of
688 for the suffix search / the consumer composition), never as a native success.

Two independent equivalence proofs tie the generic engine to the legacy court:
`test_ir_court_agrees_with_the_legacy_court_for_every_composition` asserts identical
external answers, fallback/broken accounting, and dispatch counts (where the
definition coincides), and the runtime migration leaves every committed v1 verdict
byte-identical. `verify_composition_ir_court.sh` checks the four identities, that
every stage accounts for exactly `cases_run` cases, and that each stage's seal equals
the committed store's seal for that port — a seal of a seal.

### Persistent Sealed Port Store

The courts derive a seal; the runtime loads one. `phost/evidence/store/index.json`
is the committed index that makes that possible, and `phost/src/porting/store.rs`
loads and verifies it.

| Aspect | Result |
|--------|--------|
| Ports covered | 13 — 6 leaf objects (`toupper`, `memcmp`, `memchr`, `strlen`, `strrchr` — ISO C — plus `strspn`, recorded historically as `posix:` and corrected to `libc:` by a successor) and 7 compositions (incl. the nested `toupper_each_strlen_memchr`, the buffer-slicing `toupper_memchr_suffix`, and the derived-buffer-consuming `toupper_each_slice_search`) |
| Leaf artifact | the committed `candidate.o` bytes; its SHA-256 equals the recorded `object_hash` and the committed `candidate_signature.json` object hash |
| Composition artifact | the `chain_hash` read from the committed `composition_verdict.json` — not re-derived by re-running the inner court |
| Load verification | schema + residual hash; unique sealed targets; object bytes hashed against the seal; composition stages resolved in the same store; **fails closed** on any failure |
| Determinism | a fresh index generated from committed evidence is byte-identical to the committed one |
| No compiler at runtime | `port compose --target <X> --store --phorc /nonexistent/phorc` reproduces the committed verdict (`chain_hash` MATCH); `port native` dispatches a sealed object |
| Independent recompilation | `verify_store.sh` recompiles each `.phor` candidate with `phorc` and requires byte-equality with the committed object |
| Commit policy | the leaf `candidate.o` files are committed (the seal *is* the object's bytes); `candidate.receipts.json` and the large composition oracle traces stay generated |

Store residual hash: `181ace8308112f04dea51dee34eeb90059d6a8a639743d63597c35f32aaada1d`.

```sh
./verify_store.sh
./verify_session.sh
```

### Sealed Native Service

The courts derive a seal and the store commits one; the **service** is where many
consumers share it. `phost/src/porting/service.rs` owns one verified index for its
lifetime, so no call re-reads the store.

| Aspect | Result |
|--------|--------|
| Store loads | **1** for the whole session (the type has no store access after `open`) |
| Ports served | **13** — the six leaves and the seven compositions, each once |
| Calls | 13 native, **0** foreign fallback, **0** broken seals |
| Objects mapped | **6** — the leaf objects are mapped once and reused by every chain |
| Sealed-port resolutions | **68**, including the stages a chain dispatches from inside its runner |
| Fan-in | `toupper` **39**, `memchr` **10**, `strlen` **4**, `memcmp` **1**, `strrchr` **1**, `strspn` **1**; `toupper_each` **5** (its own call plus four nested fold stages); `toupper_memchr` **2** (its own call plus the slice-search chain's stage); the other chains **1** |
| Every port dispatchable | the dispatcher resolves a composition id to its typed `CompositionIR` (Phase 2: a composition is data), so `dispatch_port` on a composition runs the sealed chain — and the nested chains resolve the sealed compositions `toupper_each` and `toupper_memchr` through the same index. The IR runtime reproduces this verdict byte-for-byte (`d219be2c…`) |
| Cycle safety | a composition cycle in the index is rejected at load (recurring through the index could never terminate) |
| Independent of path | the same verdict reproduces from a copy of the store at another path |

Session hash: `d219be2c207da11caf9a1b258e166bb7d535c36ce26452354cf232d4e425d431`.
Evidence: `phost/evidence/session/session_verdict.json`.

```sh
./verify_session.sh
```

### Corrected Contract Provenance: `strspn`

`strspn` is an **ISO C** surface — specified by ISO C (C90 4.11.5.4; C99 and later
7.21.5.4), with POSIX stating that its `strspn` specification is aligned with and
defers to ISO C. The committed baseline nevertheless recorded it as
`posix:strspn:c-locale:u64:v1` on the mistaken claim that ISO C does not specify it
(so were `strcspn`, `strpbrk` and `strtok`). That claim was wrong. The historical
target, its `PortSpecId`, its seal and its evidence are **preserved unchanged**; the
correction is an evidence-preserving **successor**, requalified and resealed on its own
identity. See `docs/CONTRACT_PROVENANCE_MIGRATION.md`.

The committed baseline's sealed `strspn` leaf (historical record, preserved):

| Aspect | Result |
|--------|--------|
| Qualified id | `posix:strspn:c-locale:u64:v1` (historical record; dialect `posix`, symbol `strspn`, locale `C`) |
| Observable | the length of the initial segment of `s` consisting only of bytes in `accept` — a prefix length decided by **set membership** |
| Corpus | 578 cases: the complete `(span, n)` grid (36), the empty set and empty string, a stop byte before the terminator, every set size 1..=8, a disjoint set, the terminator at the bound, and two exhaustive 0..=255 sweeps (the accepted byte and the stopping byte) |
| Replay | **578/578 pass**, 0 failed |
| Executed object | **578/578 pass** — the branchless `.phor` candidate (`_phor_phor_strspn_len`, 25072-byte ELF64, **0 relocations**) matches the foreign oracle case for case |
| Runtime dispatch | **578/578 native**, 0 fallback, 0 broken seals |
| Verdict / promotion | `consistent` / `sealed` |
| Oracle hash | `ab62795e45833c41d2feab54c13379ef670c1ee25f9f9325bbb9bc166f32591d` |
| Candidate object hash | `c93271d069fd998e6de8c2cd07e665bd098f6fb64072cdd98fb44ae1375dbafc` |

The corrected successor `libc:strspn:c-locale:u64:v1` re-earns its authority rather
than inheriting the historical seal: bounded CEGIS (an incorrect first revision is
falsified and minimized before the survivor is frozen), a 498-case held-out universe
built **after** the freeze (997 isolation markers, 0 leaks), a sensitivity challenge,
the FRF outer court, ordinary uninstrumented execution, native dispatch, and
`AUTONOMOUS-SEAL/v1` with **13/13 obligations**. Its `PortSpecId` is `a4ee309a…` and its
evidence closure is `b7ec423a…`; the historical `PortSpecId` `bc0420f0…` is unchanged.

Why this is a correction and not a rename:

- the historical target, its golden `PortSpecId` and its evidence are unchanged, and
  bare-symbol resolution (`strspn`) still returns the historical target;
- the successor is a **distinct** `PortSpecId` with its own autonomous seal and its own
  evidence closure (no identity collapse);
- the corpus still falsifies a *single-byte compare* standing in for a set test (set
  sizes 1..=8 are all present) and a scan that forgets that the terminator is never a
  set member;
- `verify_jit_porting_court.sh` requires the sealed package's `dialect` to equal the
  **namespace of the target id** (`posix:strspn…` seals as `dialect: posix`), so the
  historical qualification is preserved exactly rather than silently rewritten.

`./verify_supersession.sh` asserts all of the above: the correction record is
consistent, the historical evidence still verifies, the successor carries its own seal,
the two closures differ, and the successor is published as an immutable child
generation whose parent and carried-forward baseline entries check out.

The honest limit, stated in **`docs/DIALECT_QUALIFICATION.md`**: the dialect records the
*specification namespace*. The implementation observed for the seal is the host C
library, and the seal binds the observed behavior by oracle hash, so a differing
implementation cannot pass silently. Implementation-independence is a **separate axis**,
now closed by the cross-implementation court below.

### Cross-Implementation Court

A seal binds an observation of *one* implementation: the host C library. So
the dialect cage's contract for `strspn` is really that contract *as this host
implements it*. The cross-implementation court (`phost/src/porting/cross_impl.rs`,
`phost/foreign/musl_probe.c`) closes that gap by observing the **same sealed corpus**
through a **second, independent implementation** and requiring agreement on every case.

The second implementation is musl, reached out-of-process through a statically linked
probe (`musl-gcc -static`). Independence is checked, not asserted: the verifier builds
the probe itself, requires the probe to report `libc=musl` from its own `#ifdef`
check, and parses the ELF to require **no `PT_INTERP`** — a static binary cannot be
dynamically linking the host's library. The probe's per-symbol decoding is written out
again rather than shared with the Rust cage, because an observer that shared the cage's
code could only confirm the cage.

| Aspect | Result |
|--------|--------|
| Targets | all six sealed leaves: `toupper`, `memcmp`, `memchr`, `strlen`, `strrchr`, `strspn` |
| Primary | `host-libc` — in-process FFI (the dialect cage), i.e. exactly the sealed path |
| Secondary | `musl` — out-of-process statically linked probe (`libc=musl`, no `PT_INTERP`) |
| Cases | **2272** total (256 + 312 + 482 + 308 + 336 + 578), **0 disagreements** |
| Trace sets | `secondary_oracle_hash == primary_oracle_hash` for every target — the same sealed trace set, not merely the same answers |
| Seal binding | `primary_oracle_hash ==` the leaf's committed `combined_oracle_hash` |
| Verdict / promotion | `consistent` for every target; promotion is unchanged and separate |

Two facts about the run are environment-bound and are recorded as **observed, not
asserted** (the same split the boot manifest uses for the kernel image): the host
library's version string (here `gnu libc 2.44`) and the compiled probe's hash
(`musl-gcc`-version-bound). The `residual_hash` covers exactly the asserted claim; a
test asserts that changing the probe binary hash does **not** change the residual, while
changing the second implementation's identity **does**.

Reproduce:

```sh
cargo run -p phost -- port cross toupper      # 256 cases, 0 disagreements
cargo run -p phost -- port cross strspn       # 578 cases, the historical posix: surface
./verify_cross_implementation.sh              # rebuild the probe + verify all six
```

The verifier writes only to a temp directory and fails loudly (exit 2) when `musl-gcc`
is absent, because a cross-implementation claim that cannot observe a second
implementation must never pass quietly. The honest limit is unchanged: **agreement on a
bounded corpus is evidence, not proof of equivalence.**

### Autonomous Porting Foundry — Phase 0

The autonomous JIT-porting foundry integrates this repository's courts with
**FRF** (the epistemic outer court), **FRF-Fuzz** (the counterexample engine) and
**Gemel** (longitudinal memory). The normative architecture, invariants, phase
plan and acceptance gates are in `docs/AUTONOMOUS_PORTING_ARCHITECTURE.md`.

Phase 0 has two parts, both complete:

**Compatibility reconciliation.** `frf-fuzz 0.8.0` pinned `frf =0.1.72` and
`gemel =0.11.0`, while the current released repositories are `frf` 0.1.86 and
`gemel` 0.11.1. Every FRF/Gemel surface FRF-Fuzz consumes was compared symbol for
symbol against the new sources; **no semantic mismatch was found**, so nothing
had to be adapted. The reconciliation is committed in `frf-fuzz` (`610b688`) with
`docs/DEPENDENCY_RECONCILIATION.md`. All FRF-Fuzz gates pass: 350 coordinator
tests, 124 target-runtime tests, the I15 dependency closure (`libc` + `memmap2`
only), fmt, clippy `-D warnings`, the golden demo (a real FRF receipt + three
Gemel boundaries on the pinned nightly) and the Phase-8 ablation demo.

**Executable baseline seal.** `foundry/baseline/dependency_pins.json` pins the
four repositories; `scripts/integration_baseline.sh` runs the courts and writes
`foundry/baseline/integration_baseline_receipt.json`. The receipt records the
four commits, the toolchain, the `phorc` identity, the phorc/phost test counts,
the sealed leaf/composition counts, the store residual identity, the session
identity and the evidence roots. It uses this repository's asserted/observed
split — environment-bound build bytes are excluded from the residual hash — and
is byte-reproducible at a given toolchain (no wall-clock value enters the hash).

| Aspect | Result |
|--------|--------|
| Pinned stack | `frf` 0.1.86, `frf-fuzz` 0.8.0, `gemel` 0.11.1, phorensicos `11ee334` |
| Sealed ports | **13** (6 leaf objects + 7 compositions) |
| Store residual | `181ace8308112f04dea51dee34eeb90059d6a8a639743d63597c35f32aaada1d` |
| Session | 13 calls, 13 native, 68 dispatches, 6 objects, `d219be2c…` |
| Tests | phorc 50, phost 241 (4 ignored), 0 failures |
| Baseline residual | `1c52a04bf43283d2367c74a6266246a101b67125ea752330e3d61d05b7f121d1` |

### Phase 7 — held-out qualification, the implementation axis, the FRF outer court, `AUTONOMOUS-SEAL/v1`

The host-only foundry (`phorport`) gained the held-out qualification universe
(role-lattice, constructed after the freeze, redacted receipt, leakage audit), the
multi-oracle policy (host-observed / multi-implementation, never a majority
vote), the FRF outer court bound to its own store (receipts retained verbatim),
an out-of-process contained candidate worker (`no_new_privs`, `RLIMIT_AS/CPU/CORE/NOFILE`,
timeout, crash recovery), and the end-to-end pipeline. `phost` gained the
identity namespaces, the content-addressed evidence closure, `OracleWitness` /
`MultiOracleVerdict`, and the `AUTONOMOUS-SEAL/v1` obligation profile (O1–O13).

`verify_autonomous_seal.sh` checks the committed evidence for the reconstructed
`strspn` candidate and proves the promotion and qualification receipts reproduce
byte-for-byte. Committed identities:

| Aspect | Result |
|--------|--------|
| Seal profile | `AutonomousV1`, 13/13 obligations |
| Promotion receipt | `b82e62678b905bf10ab737bfc248a912ed1d6d1fad9ff17fee2edc13ae5833a0` |
| Evidence closure | `cf8b202a568d63dae771800f1a9ce953b46dca9f05817cccb43109d2729bad84` |
| Held-out qualification | 498/498 cases, `isolated`, universe `3bdde558…` |
| Multi-oracle | `concordant`, 578 cases, `host-observed` |
| Isolation audit | 997 markers checked, 0 leaks, universe built after freeze |
| FRF receipts | one `verified` divergence (revision 0) + one preserved parity receipt |

Details: `docs/QUALIFICATION_POLICY.md`, `docs/EVIDENCE_MODEL.md`,
`docs/THREAT_MODEL_AUTONOMOUS_PORTING.md`,
`docs/AUTONOMOUS_PORTING_VERIFICATION_REPORT.md`.

### Phase 8 — immutable store generations and demand-driven porting

`StoreGeneration`/`GenerationLedger` make publication immutable: a new qualified
port is generation `N+1` derived from `N`, with a content identity over
`(schema, parent, sorted entries, closure)` and fail-closed verification. A
session binds a generation and refuses to serve an artifact that generation did
not bind. A runtime miss emits a bounded, non-blocking `PortDemand`; the host
ranks pending demands with fixed integer weights and a recorded breakdown. See
`docs/STORE_GENERATIONS.md`. `phorport store generations` and `phorport demand
rank` expose both.

### Phase 9 — blind regeneration and controlled ablation

A bounded enumerative Phor synthesizer (`phorport/src/synth.rs`) reconstructs
`strlen` from the public `PortSpec` alone — falsifying a wrong-first lane-scan
family, then qualifying, challenging, FRF-verifying, executing, dispatching and
sealing it — without ever reading the withheld candidate source (`memchr` is
supported by the same family). A five-arm controlled ablation measures
executions-to-first-distinguishing-input with a shared seed and a fixed budget;
on the declared `strspn` families it is an honest negative (all arms distinguish
4/4, because the design corpus already suffices). See
`docs/AUTONOMOUS_PORTING_VERIFICATION_REPORT.md` and
`phost/evidence/phorport/ablation/strspn.json`.

### Phase 10 — bounded CompositionIR synthesis

`phost/src/porting/composition_synth.rs` is a bounded typed-graph enumerator over
a declared grammar and port set. It rediscovers the `toupper_each` composition
(a slice of a declared input followed by the sealed `toupper` map) from the
declared inputs, ports, output type and oracle behavior alone, validating each
candidate and matching it against the oracle on the whole corpus. The larger
chains are not reached within the current budget. See
`docs/COMPOSITION_SYNTHESIS.md`.

### Phase 11 — ABI v2 (court side)

`phost/src/porting/abi_v2.rs` implements the memory-effect court: guarded arenas,
before/after images, the actual write set, guard-zone observation, and overlap
classification with forward/backward reference models. Hostile tests show it
detects a guard clobber, a write outside the allowed range, and separates a naive
forward `memcpy` from an overlap-correct `memmove` in both directions. `phorc`
does not yet emit pointer/region arguments, so **no memory-effect port is
sealed**; the compiler half is future work. See `docs/ABI_V2.md`.

```sh
./scripts/integration_baseline.sh
```

**Phase 1 (complete): the canonical `PortSpec` and a registry-driven engine.**
`phost/src/porting/portspec.rs` replaces the stringly `PortTarget` with a typed
spec that separates the contract from the implementation observed
(`ContractSource`), makes normalisation explicit (`ObservableSpec` +
`ObservationProjectionSpec`), validates cases against declared `PreconditionSpec`s
(`validate_case` → `ValidatedCase`; only a validated case reaches the foreign
oracle), and carries a domain-separated content identity
`PortSpecId = SHA-256("PHOR/PORTSPEC/v1\0" ‖ canonical_bytes)`. All six leaves are
expressed; their identities are pinned by a golden test, and a test proves every
existing leaf corpus satisfies its own declared preconditions.

`phost/src/porting/registry.rs` is the single extension table (spec + bootstrap
target + CaseGenerator + candidate adapter + ABI adapter). The generic machinery
no longer branches on a target id: `target::cases_for`, `target::resolve_target`,
`candidate::run_candidate` and the execution court's `call_target` all look the
target up in the registry; `dialect_cage.rs` remains the oracle-adapter boundary.
A static audit reads every generic `porting/` module and fails if a `.id ==`
target branch reappears. The refactor is behavior-preserving: every committed
leaf, composition, store, session and cross-implementation verdict is
byte-identical (Gate A). See `docs/PORT_SPEC.md`.

```sh
cargo test -p phost --lib porting::portspec
cargo test -p phost --lib porting::registry
```

**Phase 2 (complete): the typed `CompositionIR` and one generic engine.**
`phost/src/porting/composition_ir.rs` turns composition from bespoke Rust runners
into **data**: a typed, bounded, acyclic graph over sealed ports with a
domain-separated content identity
`CompositionIrId = SHA-256("PHOR/COMPOSITION-IR/v1\0" ‖ canonical_bytes)` and one
lazy `eval` over a site-aware `PortBackend`. Because the oracle side and the sealed
side evaluate the *same* graph, a composition has exactly one meaning and cannot
drift. `composition_registry.rs` is the one table binding each of the seven chains
to its IR and corpus; `composition_engine.rs` provides `ForeignBackend` (the cage),
`SealedBackend` (`NativeDispatcher`), the generic court, and the runtime path
`eval_composition_port`. The `composition_runner(id)` branch is removed; the runtime
resolves a composition port to its IR and recurses through the same dispatcher, so
every seal check and dispatch count is preserved — the committed session verdict
(`d219be2c…`, 13 calls / 68 dispatches / 6 objects) is byte-identical. A composed
artifact binds four v2 identities (`composition_ir_hash`,
`dependency_binding_hash`, `behavior_hash`, `composition_artifact_hash`); the
historical v1 `chain_hash` remains the seal the store publishes, and the v2 evidence
lives in `phost/evidence/composition_ir/` verified by
`verify_composition_ir_court.sh`. Equivalence is proven by an in-test cross-check
(`test_ir_court_agrees_with_the_legacy_court_for_every_composition`) and by the
unchanged v1 evidence under the legacy verifier. See `docs/COMPOSITION_IR.md`.

**Phase 3 (complete): the court-sensitivity (challenge) court.**
`phost/src/porting/challenge.rs` makes the measuring instrument falsifiable. A
bounded `MutationProfile` of intentionally wrong implementations is run against the
**same corpus and oracle the real court uses**: leaf mutants are wrong observable
implementations, composition mutants are wrong `CompositionIR`s evaluated over the
sealed store against the correct chain's committed oracle. For all six leaves and
all seven compositions **every declared family is detected** — e.g. `strlen`
`last_nul` on 30/308 cases, `treat_0x80` on 22/308, `off_by_one` on 308/308;
`strspn.omit_lane` on 297/578; `toupper_memchr.no_fold` on 40/560;
`slice_search.unfolded` on 149/688. Detection is recorded as *specific* (localized)
or *coarse*, and never collapsed into one score. An equivalent mutant (equal to the
correct implementation on every valid input under the declared preconditions) is
recorded as equivalent with its reason and is never counted as killed or missed —
the `strlen` "ignore the bound" mutant is equivalent because the precondition
guarantees a NUL within `n`. The evidence is committed at
`phost/evidence/challenge/<name>/challenge_verdict.json` and verified by
`verify_challenge_court.sh` (schema, counts, no blind spots, equivalence notes,
canonical residual, determinism). See `docs/CHALLENGE.md`.

**Phase 4 (core): the FRF-Fuzz counterexample engine.** `phorport/` is the one
host-only orchestration crate, outside the sealed runtime's dependency closure. It
provides the differential comparison harness (precondition gate → foreign oracle →
**uninstrumented compiled `.phor` object** → normalized comparison → structural
residual), the `PORT.*` residual bank, **court-verified minimization** (the
minimizer proposes, the same court decides), durable content-addressed
counterexample records, and an FRF-Fuzz campaign bridge that generates a
differential target, seeds it with the design corpus, runs a bounded campaign, and
shrinks the finding. Gate E is demonstrated: a `strspn` candidate whose
set-membership fold is XOR instead of OR is **missed by the whole design corpus
(578 cases, 0 divergences)**; a 90-second FRF-Fuzz campaign records 246 findings
and the first is minimized from 15 to 7 bytes and replayed on the uninstrumented
object with the same `PORT.LENGTH` lineage. The campaign also exposed a real ABI
violation in phorc's emitted objects (callee-saved registers clobbered without a
save); it is fixed in the executor's register-preserving call trampoline, so no
emitted object, seal, verdict or session hash changed. Verified by
`verify_phorport.sh`. See `docs/AUTONOMOUS_PORTING.md`.

**Phase 5 (core): Gemel longitudinal memory.** `phorport/src/memory.rs` publishes
durable boundaries into Gemel — discovered counterexamples and rejected
candidates — using Gemel's own object model (`Family::Residual`, `Field`, `Value`),
content-addressed store and opaque Gids (never merged with Phorensicos hashes or
FRF ids). Records are keyed by the candidate **source** identity, so an identical
source that already failed is retrieved rather than rediscovered: a second campaign
for the same source reports "already known" and does not build or run the target at
all. Gemel is optional (standalone mode when no repository is discoverable). Gate F
is demonstrated end-to-end (`findings: 463` and a memory Gid on the first run; the
prior rejection on the second; `phorport history strspn` lists both records).
Verified by a unit test and the committed campaign evidence. Intents, trajectories
and cross-compiler revision replay remain. See `docs/GEMEL_MEMORY.md`.

**Phase 6 (core): the candidate producer and the bounded CEGIS loop.**
`phorport/src/producer.rs` defines the untrusted `CandidateProducer` trait with a
scripted catalogue and an external-command (agent) adapter; every proposal is
checked against the `PortSpec`'s constraints (size, leaf policy — comments stripped)
before compilation. `SynthesisWorkspace` materializes only permitted material and
audits for qualification leaks (unit-tested with forbidden markers).
`phorport/src/cegis.rs` drives a monotonic campaign state machine over a bounded
budget: propose → constrain → compile (a `CandidateIdentity` of source+object hash)
→ design court → discovery court (Phase 4's FRF-Fuzz campaign is the production
hook) → revise on failure or freeze on survival. A falsified revision is minimized
through the same court and remembered as negative knowledge. Demonstrated:
`phorport campaign strspn --source <always-0> --source examples/jit_port_strspn.phor`
rejects revision 0 on 305/578 design cases and freezes revision 1. Disagreement
driven experiment selection and a shipped synthesizer remain; the workspace's
qualification isolation is structural (the full held-out universe is Phase 7). See
`docs/CEGIS.md`.

```sh
cargo test -p phost --lib porting::composition_ir
```

### Test Results (reproduced on `main`)
- phost: 265 passed, 0 failed, 4 ignored (269 total). The ignored tests read
  privileged control registers (CR0/CR2/CR3/CR4) and fault outside ring 0;
  run them under a kernel harness with `cargo test -- --ignored`.
- phorc: 44 unit + 6 integration tests passed (0 warnings). The integration tests
  pin the lowering fixes that unblocked execution: `<<` lowering to `Shl`, named
  constants resolving to literals (including integer constant folding of `0 - 1`),
  `&&` lowering to `And`, shift precedence, and `x = expr` storing instead of
  adding.
- `.phor` corpus: 369/369 files lower to non-empty ELF64 objects
  (`src/` 232, `examples/` 51, `tests/` 83 incl. 46 `compile-pass`, `fixtures/` 2,
  `README.phor`). These are bootstrap-stage results: the checker still reports diagnostics for constructs
  outside its current subset, but lowering and ELF emission complete for every file.
- `.ph` compile-pass fixtures: 46/46 files emit objects.

### Reproduce
```sh
cargo test                                       # phorc + phost
cargo run -p phorc -- examples/hello.phor /tmp/hello.o --emit-receipts --emit-seal
cargo run -p phorc -- --court-replay /tmp/hello.sealed_package.json
cargo run -p phost -- port promote toupper       # JIT-porting court (256)
cargo run -p phost -- port promote memcmp        # JIT-porting court (312)
cargo run -p phost -- port promote memchr        # JIT-porting court (482)
cargo run -p phost -- port promote strlen        # JIT-porting court (308)
cargo run -p phost -- port promote strrchr       # JIT-porting court (336)
cargo run -p phost -- port promote strspn        # JIT-porting court (578, historical posix: record)
cargo run -p phost -- port cross toupper          # cross-implementation court (256, musl)
cargo run -p phost -- port cross strspn           # cross-implementation court (578, musl)
cargo run -p phost -- port store                # load + verify the committed store
cargo run -p phost -- port store --write        # regenerate it from committed evidence
cargo run -p phost -- port session              # one load, thirteen ports, six objects
cargo run -p phost -- port session --no-capability   # store never read
cargo run -p phost -- port native toupper 61     # runtime dispatch (sealed object)
cargo run -p phost -- port native memchr 616263:62:0300000000000000
cargo run -p phost -- port native toupper 61 --no-capability   # foreign fallback
cargo run -p phost -- port compose               # composition court (560)
cargo run -p phost -- port compose --target toupper_strlen_memchr   # composition court (350)
cargo run -p phost -- port compose --target toupper_strlen_memchr_pair  # composition court (474)
cargo run -p phost -- port compose --target toupper_each            # map court (267)
cargo run -p phost -- port compose --target toupper_each_strlen_memchr  # nested court (350)
cargo run -p phost -- port compose --target toupper_memchr_suffix       # slice court (688)
cargo run -p phost -- port compose --target toupper_memchr_suffix 62617862:61:62:0400000000000000
cargo run -p phost -- port compose --target toupper_each_slice_search    # derived-buffer court (688)
cargo run -p phost -- port compose --target toupper_each_slice_search 62617862:61:62:0400000000000000
./verify_jit_porting_court.sh --target toupper                    # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memchr
./verify_jit_porting_court.sh --target strlen
./verify_jit_porting_court.sh --target strrchr
./verify_jit_porting_court.sh --target strspn                     # the historical posix: record
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
./verify_composition_court.sh                                    # composition #1 (560)
./verify_composition_court.sh --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr     # composition #2 (350)
./verify_composition_court.sh --target toupper_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr_pair     # composition #3 (474)
./verify_composition_court.sh --target toupper_strlen_memchr_pair --check-committed
./verify_composition_court.sh --target toupper_each                   # composition #4 (267)
./verify_composition_court.sh --target toupper_each --check-committed
./verify_composition_court.sh --target toupper_each_strlen_memchr     # composition #5 (350)
./verify_composition_court.sh --target toupper_each_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_memchr_suffix          # composition #6 (688)
./verify_composition_court.sh --target toupper_memchr_suffix --check-committed
./verify_composition_court.sh --target toupper_each_slice_search      # composition #7 (688)
./verify_composition_court.sh --target toupper_each_slice_search --check-committed
# Phase 2: the generic (IR) composition court — composition as data.
./verify_composition_ir_court.sh --check-committed                     # composition #1 (560)
./verify_composition_ir_court.sh --target toupper_each_slice_search --check-committed
# Phase 3: the court-sensitivity (challenge) court.
./verify_challenge_court.sh --target strlen --check-committed
./verify_challenge_court.sh --target toupper_memchr_suffix --check-committed
# The store-backed composition court: no compiler, no nested replay.
cargo run -p phost -- port compose --target toupper_memchr --store --phorc /nonexistent/phorc
cargo run -p phost -- port compose --target toupper_memchr --ir --store --phorc /nonexistent/phorc
cargo run -p phost -- port compose --target toupper_each_strlen_memchr --store --phorc /nonexistent/phorc
./verify_store.sh                                # persistent store verifier
./verify_session.sh                              # sealed native service verifier
./verify_cross_implementation.sh                 # cross-implementation court (all six leaves)
cd phost_kernel && ./build_kernel.sh             # Multiboot image
./boot_qemu.sh phorensic-kernel.elf evidence 8   # boot + capture
./verify_evidence.sh evidence                    # 13/13 boot-evidence checks
./evidence_manifest.sh evidence
```

Everything above also runs in containers:
```sh
docker compose run --rm host     # cargo test + store, session, court and composition verifiers
docker compose run --rm kernel   # build kernel + QEMU boot + evidence verify
```

**Gate L (clean-room reproduction) is verified.** From a clean checkout of the final
tree,
`docker compose build host && docker compose run --rm host` exits 0 (74 verifier
blocks — including the new `./verify_supersession.sh` — with 0 failures) and
`docker compose build kernel && docker compose run --rm kernel` exits 0 (13/13
boot-evidence checks, all five captured artifacts byte-reproducible against the
committed manifest). The Docker build context excludes host-local foundry state, so
the in-container workspace is exactly the committed tree.

Boot evidence (hashes, ABI address, commands, toolchain) is committed as
`phost_kernel/evidence_manifest.json`. The manifest's claim is two-tier:
`asserted_reproducible_evidence` holds the five boot-evidence artifacts
(`serial.log`, `debug.log`, `screen.ppm`, `fb-abi.bin`, `lfb.bin`), which the
kernel CI asserts byte-identical against the committed manifest, while the kernel
image hash sits under `observed_toolchain_bound_build` with
`asserted_reproducible: false` (image bytes depend on the
rustc/nasm/ld.lld/binutils versions). A top-level `manifest_claim` states this
explicitly, and the kernel CI verifies the flag is still `false` before checking
the evidence. The porting evidence set (oracle traces,
behavior/candidate signatures, replay verdict, execution verdict, dispatch
verdict, promotion receipt, sealed package) is committed as
`phost/evidence/porting/{toupper,memcmp,memchr,strlen,strrchr}/`, and the
composition verdicts as
`phost/evidence/composition/{toupper_memchr,toupper_strlen_memchr,toupper_strlen_memchr_pair,toupper_each,toupper_each_strlen_memchr}/composition_verdict.json`
(the large composition oracle traces are regenerable and stay gitignored). The
cross-implementation verdicts are committed as
`phost/evidence/cross/{toupper,memcmp,memchr,strlen,strrchr,strspn}/cross_implementation_verdict.json`,
with the environment-bound facts under `observed_not_asserted`.
