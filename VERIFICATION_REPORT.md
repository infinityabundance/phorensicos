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
| All tests (phost) | ✅ 121 pass, 4 ignored | 125 total: 8 serial + 4 kernel + 49 nucleus + 56 porting (incl. exec + dispatch courts); the 4 ignored read privileged control registers and require ring 0 |
| All tests (phorc) | ✅ 48 pass | 44 unit (parser/checker/lower/codegen) + 4 integration lowering regressions |
| Full pipeline (`.ph` → ELF64) | ✅ Works | lex → parse → check → lower → codegen → emit |
| `canvas` module | ✅ | Shapes, text, compositing primitives |
| `console` module | ✅ | Scrolling text console on framebuffer |
| `shell` module | ✅ | Interactive shell with keyboard input |
| `serial` driver | ✅ | UART serial driver with capability model |
| `status_screen` | ✅ | Boot phase display, progress bar, log messages |
| `input/keyboard` | ✅ | PS/2 keyboard driver with scancode→ASCII |
| `.phor` corpus | ✅ 315/315 → ELF64 | `src/` 235 + `examples/` 46 + `tests/` 34, all emit objects (checker diagnostics may be emitted; see Known Gaps) |
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
| phost reach | 108 tests, loader, compositor, phorc_bridge, keyboard, serial, canvas, shell, JIT-porting court (toupper + memcmp) + sealed-object execution court |

### JIT-Porting Court
| Aspect | `toupper` | `memcmp` |
|--------|-----------|----------|
| Qualified target id | `libc:toupper:c-locale:u8:v1` | `libc:memcmp:c-locale:sign:v1` |
| Corpus | exhaustive `0x00..=0xff` (256) | bounded deterministic (312) |
| Dialect cage observation | ✅ 256/256 | ✅ 312/312 |
| Replay court | ✅ 256/256, 0 failed, `consistent` | ✅ 312/312, 0 failed, `consistent` |
| Contract | `u8` (C locale) | sign `(-1 \| 0 \| 1)`, unsigned, n-bounded |
| Oracle hash | `8daf25ed2482b1258bdef6b618ea2c2ef17c22ef45b47ba3a45ded2cfc05ac91` | `918c86d5302aaa0394a782b68e4aed7494549bcaaf06a82948c6d945ccb43210` |
| Candidate behavior hash | `777f11a0264a69b20f5c8a87d9c0687768d3e11216a43dd95698b5dd119fefc2` | `e7fcf296920ea7a179a218202a059665dab2ee6cd38ade094696b44b19553033` |
| Candidate source hash | `a23bea4da98b4526635d295b6f71f1dbfc14e1a219fb2849d94ee89ca80a37b8` | `adcb65fbd2e54a04c78ed019947d157d0935247b69be828ceb0702af75088a0c` |
| Candidate object hash | `05c175a89a25d339f22793193860045e94cdf5c4a2f85cfba3dea3677ab4e8d4` | `23ae1e515557ab9449838ad1f72666e70d7ff9f689acf306877a9f3a0a1bd854` |
| Candidate receipt hash | `614fd9a0860eeee816512a26320001a24217fcfaf2c37344d2a528ca24133485` | `83f0698ed7c1d684be3b0522cfa0e816e00a2708a9f67acb295e3d68c6446aa7` |
| Compiler | `phorc 0.1.0` | `phorc 0.1.0` |
| Replay residual | `c5a4a2b88a72e56c3a7ea6d1a248723a337a38cca9f001e50f868e37abaed349` | `98843827cbbf97866221127651840eaaee6dbb17dc1bf3e0053476ce30a8290d` |
| Executed ELF symbol | `_phor_phor_toupper` | `_phor_phor_memcmp_sign` |
| Execution court | ✅ 256/256, 0 failed, `consistent` | ✅ 312/312, 0 failed, `consistent` |
| Execution behavior hash | `777f11a0264a69b20f5c8a87d9c0687768d3e11216a43dd95698b5dd119fefc2` | `e7fcf296920ea7a179a218202a059665dab2ee6cd38ade094696b44b19553033` |
| Execution residual | `e73092e6e2d112cfd5334d9fa840e22b12056fb3f53b63dfc8b7d38898b970d7` | `9ac82dc7a3872ca8b4e493322f5164d54c9732f1dece88ca8e995d5b7015ab03` |
| Dispatch court | ✅ 256/256 native, 0 fallback, `consistent` | ✅ 312/312 native, 0 fallback, `consistent` |
| Dispatch hash | `a3f7c0606ae6b6da6353c1b532957dbe40022e206cb059c1512620ecaa0df90d` | `fb00385a5ecaf78c2031d25d7d6c98b7a040d44c53fd12579fa6931394387231` |
| Dispatch residual | `c2c360eb46bc24957b8a88852c7bf9b946b4908efcbebbe3cc6ca3950aa102bd` | `ce74b873d5ec2e652a6ff48cf2b5c7dcec9d45a6d768f649858e17a91f1d1d09` |
| Promotion | ✅ → `sealed` | ✅ → `sealed` |
| Sealed package | `native:libc:toupper:c-locale:u8:v1` | `native:libc:memcmp:c-locale:sign:v1` |

For both targets the **execution behavior hash equals the candidate behavior hash**: the
Rust mirror and the compiled object reproduce the identical output for every case,
computed through different code paths (slice-based vs packed-word).

Shared properties (both targets):
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
`_phor_phor_memcmp_sign`), rejects an entry whose byte range contains a relocation
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

Sealed native dispatch: the runtime call site (`phost::porting::dispatch`) looks
the target up in the capability-gated sealed store and, if a sealed entry is
present, verifies the object hash, maps the entry function once and calls it;
otherwise it reports a foreign fallback. A *broken seal* is counted separately
(`broken_seal_cases`) and is terminal — it never falls back. The dispatch court
replays the whole corpus through this path and requires 256/256 (`toupper`) and
312/312 (`memcmp`) cases served natively with zero fallbacks and zero broken
seals; `dispatch_hash` is bound into the promotion receipt and the sealed package.
Try it: `phost port native toupper 61` (native) and
`phost port native toupper 61 --no-capability` (foreign fallback).

### Test Results (reproduced on `main`)
- phost: 121 passed, 0 failed, 4 ignored (125 total). The ignored tests read
  privileged control registers (CR0/CR2/CR3/CR4) and fault outside ring 0;
  run them under a kernel harness with `cargo test -- --ignored`.
- phorc: 44 unit + 4 integration tests passed (0 warnings). The integration tests
  pin the lowering fixes that unblocked execution: `<<` lowering to `Shl`, named
  constants resolving to literals, `&&` lowering to `And`, and shift precedence.
- `.phor` corpus: 315/315 files lower to non-empty ELF64 objects
  (`src/` 235, `examples/` 46, `tests/` 34). These are bootstrap-stage
  results: the checker still reports diagnostics for constructs outside its
  current subset, but lowering and ELF emission complete for every file.
- `.ph` compile-pass fixtures: 46/46 files emit objects.

### Reproduce
```sh
cargo test                                       # phorc + phost
cargo run -p phorc -- examples/hello.phor /tmp/hello.o --emit-receipts --emit-seal
cargo run -p phorc -- --court-replay /tmp/hello.sealed_package.json
cargo run -p phost -- port promote toupper       # JIT-porting court (256)
cargo run -p phost -- port promote memcmp        # JIT-porting court (312)
cargo run -p phost -- port native toupper 61     # runtime dispatch (sealed object)
./verify_jit_porting_court.sh --target toupper                    # determinism
./verify_jit_porting_court.sh --target memcmp
./verify_jit_porting_court.sh --target memcmp --check-committed   # fresh == checked-in
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
`phost/evidence/porting/{toupper,memcmp}/`.
