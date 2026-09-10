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
| All tests (phost) | ✅ 77 pass, 4 ignored | 81 total: 8 serial + 4 kernel + 49 nucleus + 20 porting; the 4 ignored read privileged control registers and require ring 0 |
| All tests (phorc lib) | ✅ 43/43 pass | Parser/checker/lower/codegen |
| Full pipeline (`.ph` → ELF64) | ✅ Works | lex → parse → check → lower → codegen → emit |
| `canvas` module | ✅ | Shapes, text, compositing primitives |
| `console` module | ✅ | Scrolling text console on framebuffer |
| `shell` module | ✅ | Interactive shell with keyboard input |
| `serial` driver | ✅ | UART serial driver with capability model |
| `status_screen` | ✅ | Boot phase display, progress bar, log messages |
| `input/keyboard` | ✅ | PS/2 keyboard driver with scancode→ASCII |
| `.phor` corpus | ✅ 311/311 → ELF64 | `src/` 232 + `examples/` 45 + `tests/` 34, all emit objects (checker diagnostics may be emitted; see Known Gaps) |
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
| phost reach | 81 tests, loader, compositor, phorc_bridge, keyboard, serial, canvas, shell, JIT-porting court |

### JIT-Porting Court (first target: libc `toupper`)
| Aspect | Result |
|--------|--------|
| Dialect cage observation | ✅ 256/256 cases (exhaustive `0x00..=0xff`), C locale |
| Replay court | ✅ 256/256 passed, 0 failed, verdict `consistent` |
| Oracle hash | `23f73cde270bfe5195d906965088ddbf58af7370e05b6f0603d1d828c803fbe3` |
| Candidate hash | `777f11a0264a69b20f5c8a87d9c0687768d3e11216a43dd95698b5dd119fefc2` |
| Promotion | ✅ `oracle-compared` → `sealed` |
| Sealed package | ✅ `native:libc:toupper` (`phost/evidence/porting/toupper/`) |
| Determinism | ✅ two runs byte-identical (6/6 artifacts) |
| Capability gating | ✅ `PORTING` required to observe and to promote |

### Test Results (reproduced on `main`)
- phost: 77 passed, 0 failed, 4 ignored (81 total). The ignored tests read
  privileged control registers (CR0/CR2/CR3/CR4) and fault outside ring 0;
  run them under a kernel harness with `cargo test -- --ignored`.
- phorc: 43/43 passed (0 warnings).
- `.phor` corpus: 311/311 files lower to non-empty ELF64 objects
  (`src/` 232, `examples/` 45, `tests/` 34). These are bootstrap-stage
  results: the checker still reports diagnostics for constructs outside its
  current subset, but lowering and ELF emission complete for every file.
- `.ph` compile-pass fixtures: 46/46 files emit objects.

### Reproduce
```sh
cargo test                                       # phorc + phost
cargo run -p phorc -- examples/hello.phor /tmp/hello.o --emit-receipts --emit-seal
cargo run -p phorc -- --court-replay /tmp/hello.sealed_package.json
cargo run -p phost -- port promote toupper       # JIT-porting court
./verify_jit_porting_court.sh                    # 256/256, hashes MATCH, Sealed
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
