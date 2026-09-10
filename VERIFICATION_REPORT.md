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
| All tests (phost) | ✅ 168 pass, 4 ignored | 172 total: 111 porting + 49 nucleus + 8 drivers + 4 kernel (the 4 ignored are the ring-0 control-register reads) |
| All tests (phorc) | ✅ 50 pass | 44 unit (parser/checker/lower/codegen) + 6 integration lowering regressions |
| Full pipeline (`.ph` → ELF64) | ✅ Works | lex → parse → check → lower → codegen → emit |
| `canvas` module | ✅ | Shapes, text, compositing primitives |
| `console` module | ✅ | Scrolling text console on framebuffer |
| `shell` module | ✅ | Interactive shell with keyboard input |
| `serial` driver | ✅ | UART serial driver with capability model |
| `status_screen` | ✅ | Boot phase display, progress bar, log messages |
| `input/keyboard` | ✅ | PS/2 keyboard driver with scancode→ASCII |
| `.phor` corpus | ✅ 316/316 → ELF64 | `src/` 232 + `examples/` 47 + `tests/` 34 + `fixtures/` 2 + `README.phor`, all emit objects (checker diagnostics may be emitted; see Known Gaps) |
| `compositor` module | ✅ | Window manager, surface blitting to canvas |
| `phorc_bridge` | ✅ | Compiler invocation from shell with result parsing |
| `pub` visibility tracking | ✅ | FnDecl/StructDecl/EnumDecl/ImplBlock/ConstDecl |
| Register allocator | ✅ | x86-64 register allocator (deterministic, spill-correct) in codegen |
| Sealed-object execution | ✅ | `exec.rs` loads the sealed ELF64 object, verifies its hash, maps it and calls the ABI entry |
| Sealed native dispatch | ✅ | `dispatch.rs`: the runtime prefers the sealed object at a call site; broken seals fail closed, no capability → foreign fallback |

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
| `.phor` corpus | 315/315 files lower to non-empty ELF64 objects |
| Keyboard→compositor routing | Focus-aware input dispatch, Tab focus cycling |
| Self-consuming impl methods | `ReturnType::SelfConsuming` pattern for builder-style methods |
| `residual emit` checker | Type-checking for residual emit field expressions |
| phost reach | 168 tests, loader, compositor, phorc_bridge, keyboard, serial, canvas, shell, JIT-porting court (toupper + memcmp + memchr + strlen + strrchr) + sealed-object execution + sealed native dispatch + three sealed composition courts |

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

### Test Results (reproduced on `main`)
- phost: 168 passed, 0 failed, 4 ignored (172 total). The ignored tests read
  privileged control registers (CR0/CR2/CR3/CR4) and fault outside ring 0;
  run them under a kernel harness with `cargo test -- --ignored`.
- phorc: 44 unit + 6 integration tests passed (0 warnings). The integration tests
  pin the lowering fixes that unblocked execution: `<<` lowering to `Shl`, named
  constants resolving to literals (including integer constant folding of `0 - 1`),
  `&&` lowering to `And`, shift precedence, and `x = expr` storing instead of
  adding.
- `.phor` corpus: 316/316 files lower to non-empty ELF64 objects
  (`src/` 232, `examples/` 47, `tests/` 34, `fixtures/` 2, `README.phor`). These are
  bootstrap-stage results: the checker still reports diagnostics for constructs
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
cargo run -p phost -- port native toupper 61     # runtime dispatch (sealed object)
cargo run -p phost -- port compose               # composition court (560)
cargo run -p phost -- port compose --target toupper_strlen_memchr   # composition court (350)
cargo run -p phost -- port compose --target toupper_strlen_memchr_pair  # composition court (474)
./verify_jit_porting_court.sh --target toupper                    # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memchr
./verify_jit_porting_court.sh --target strlen
./verify_jit_porting_court.sh --target strrchr
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
./verify_composition_court.sh                                    # composition #1 (560)
./verify_composition_court.sh --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr     # composition #2 (350)
./verify_composition_court.sh --target toupper_strlen_memchr --check-committed
./verify_composition_court.sh --target toupper_strlen_memchr_pair     # composition #3 (474)
./verify_composition_court.sh --target toupper_strlen_memchr_pair --check-committed
cd phost_kernel && ./build_kernel.sh             # Multiboot image
./boot_qemu.sh phorensic-kernel.elf evidence 8   # boot + capture
./verify_evidence.sh evidence                    # 13/13 boot-evidence checks
./evidence_manifest.sh evidence
```

Everything above also runs in containers:
```sh
docker compose run --rm host     # cargo test + JIT-porting court verifier
docker compose run --rm kernel   # build kernel + QEMU boot + evidence verify
```

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
`phost/evidence/composition/{toupper_memchr,toupper_strlen_memchr,toupper_strlen_memchr_pair}/composition_verdict.json`
(the large composition oracle traces are regenerable and stay gitignored).
