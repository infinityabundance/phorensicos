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
| All tests (phost) | ✅ 97 pass, 4 ignored | 101 total: 8 serial + 4 kernel + 49 nucleus + 40 porting; the 4 ignored read privileged control registers and require ring 0 |
| All tests (phorc lib) | ✅ 43/43 pass | Parser/checker/lower/codegen |
| Full pipeline (`.ph` → ELF64) | ✅ Works | lex → parse → check → lower → codegen → emit |
| `canvas` module | ✅ | Shapes, text, compositing primitives |
| `console` module | ✅ | Scrolling text console on framebuffer |
| `shell` module | ✅ | Interactive shell with keyboard input |
| `serial` driver | ✅ | UART serial driver with capability model |
| `status_screen` | ✅ | Boot phase display, progress bar, log messages |
| `input/keyboard` | ✅ | PS/2 keyboard driver with scancode→ASCII |
| `.phor` corpus | ✅ 312/312 → ELF64 | `src/` 232 + `examples/` 46 + `tests/` 34, all emit objects (checker diagnostics may be emitted; see Known Gaps) |
| `compositor` module | ✅ | Window manager, surface blitting to canvas |
| `phorc_bridge` | ✅ | Compiler invocation from shell with result parsing |
| `pub` visibility tracking | ✅ | FnDecl/StructDecl/EnumDecl/ImplBlock/ConstDecl |
| Register allocator | ✅ | x86-64 register allocator in codegen |

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
1. **Backend maturity**: register allocation, memory operands, call ABI still minimal
2. **GUI compositor chain**: window manager renders to the real QEMU LFB at boot; interactive shell/presentation loop still pending
3. **Documentation**: CORPUS.md reference files do not exist
4. **`.phor`→Rust convergence**: serial, canvas, compositor, input handler, porting engine exist in `.phor` (examples/) but not yet integrated as system `.phor` modules
5. **Input handling**: keyboard scancode→ASCII works in Rust and .phor, but mouse/touch not yet supported
6. **Native execution**: compiled `.phor` ELF objects loadable via ELF parser with compositor bridge — real JIT/mapping pending

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
| `.phor` examples | 44/44 files lower to ELF64 objects |
| Keyboard→compositor routing | Focus-aware input dispatch, Tab focus cycling |
| Self-consuming impl methods | `ReturnType::SelfConsuming` pattern for builder-style methods |
| `residual emit` checker | Type-checking for residual emit field expressions |
| phost reach | 95 tests, loader, compositor, phorc_bridge, keyboard, serial, canvas, shell, JIT-porting court (toupper + memcmp) |

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
| Candidate source hash | `1d3828469426e3db3d6ae1796db42477552ce922b6570c74883c9c258ecf6d17` | `63f0d5b40a7ecef3b3e78e274a0dbd15a94e905c9a4210a070f689ed6bf128e2` |
| Candidate object hash | `3e64eeb126d38140b1a80a2f6bc71e250155bb771a08ec12dd3ce4970e7e0f46` | `47ed32d69955162fbb66f0d23099260b65e70017d3606aa65b65d6eb226f9a7f` |
| Candidate receipt hash | `861f8e782575c0d7d4df9f5dbd6a37324145d98b0bff2dcd6a865a12fe96a404` | `06fa179e5d4b4b977bf0f3e4bc1bf94b5eab6f83dadd5800480bd2f71cfec0bd` |
| Compiler | `phorc 0.1.0` | `phorc 0.1.0` |
| Replay residual | `c5a4a2b88a72e56c3a7ea6d1a248723a337a38cca9f001e50f868e37abaed349` | `98843827cbbf97866221127651840eaaee6dbb17dc1bf3e0053476ce30a8290d` |
| Promotion | ✅ → `sealed` | ✅ → `sealed` |
| Sealed package | `native:libc:toupper:c-locale:u8:v1` | `native:libc:memcmp:c-locale:sign:v1` |

Shared properties (both targets):
| Aspect | Result |
|--------|--------|
| Compiled candidate authority | ✅ candidate.o is the promoted implementation; object + receipt hashes bound |
| Independent recompilation | ✅ verifier recompiles the `.phor` source; object/receipt hashes MATCH the seal |
| Determinism court | ✅ two fresh runs byte-identical (6/6 artifacts) |
| Committed-evidence court | ✅ `--check-committed`: fresh run == checked-in evidence (6/6) |
| Cross-environment | ✅ committed evidence matches a fresh container run (6/6), incl. object hashes |
| Tamper detection | ✅ editing the `.phor` candidate invalidates the seal; locale is hash-covered |
| Capability gating | ✅ `PORTING` required to observe and to promote |
| Fail-closed candidate | ✅ unknown target id → `UnsupportedTarget`; malformed args → `MalformedArgs`; missing object/receipt hash blocks promotion |

Compiled candidate: the court invokes `phorc` on the target's `.phor` source (from
the workspace root, with the repo-relative path so the ELF `FILE` symbol is
environment-independent), then binds SHA-256 of the emitted ELF64 object and of
its receipt file. To make this possible, `phorc` register allocation was made
deterministic (it iterated a `HashMap`; now a `BTreeMap`), so object bytes are
reproducible across runs and environments.

`memcmp` corpus axes: lengths `0..=8`; patterns zero/ones/ascending/descending/
alternating; every first-mismatch position with both orderings; the `n`-boundary
around a mismatch (`n = j` excludes it, `n = j+1` includes it); and the unsigned
edge bytes `00/01/7f/80/fe/ff`. Every case satisfies `n <= min(len(a), len(b))`.

### Test Results (reproduced on `main`)
- phost: 97 passed, 0 failed, 4 ignored (101 total). The ignored tests read
  privileged control registers (CR0/CR2/CR3/CR4) and fault outside ring 0;
  run them under a kernel harness with `cargo test -- --ignored`.
- phorc: 43/43 passed (0 warnings).
- `.phor` corpus: 312/312 files lower to non-empty ELF64 objects
  (`src/` 232, `examples/` 46, `tests/` 34). These are bootstrap-stage
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
`phost_kernel/evidence_manifest.json`. The porting evidence set (oracle traces,
behavior/candidate signatures, replay verdict, promotion receipt, sealed
package) is committed as `phost/evidence/porting/toupper/`.
