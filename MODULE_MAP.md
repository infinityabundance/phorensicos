# Phorensic OS — Complete Module Map

**Version:** 1.1.0  
**Date:** 2026-07-04  
**Status:** Bootstrapping — Observed Trust  

---

## 1. Source Module Map

### 1.1 Core Language (`src/core/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/core/types.phor` | Primitive type system (u8–u64, i8–i64, bool, char) | None | Sealed | `core_tests.phor` |
| `src/core/collections.phor` | Fixed-capacity collections (Array, RingBuf, Slice, Str) | `types.phor` | Sealed | `core_tests.phor` |
| `src/core/enums.phor` | Enum discriminant layout, match exhaustiveness | `types.phor` | Sealed | `core_tests.phor` |
| `src/core/structs.phor` | Struct layout, packed layout | `types.phor` | Sealed | `core_tests.phor` |
| `src/core/effects.phor` | Effect system definition and propagation | `types.phor` | Sealed | `core_tests.phor`, `effects_demo.phor` |
| `src/core/capabilities.phor` | Capability type, affine moves, restrict | `types.phor`, `effects.phor` | Sealed | `capability_move.phor` |
| `src/core/errors.phor` | Typed error types, diagnostic codes | `types.phor` | Sealed | `core_tests.phor` |
| `src/core/trust.phor` | Trust ladder states and transitions | `types.phor` | Sealed | `court_tests.phor` |
| `src/core/generations.phor` | Generation tag system for handles | `types.phor` | Sealed | `kernel_tests.phor`, `edge_cases.phor` |
| `src/core/residuals.phor` | Residual emission and recording | `effects.phor` | Sealed | `effects_demo.phor` |
| `src/core/bounds.phor` | Loop bound checking and proven annotation | `types.phor` | Sealed | `bounded_loop.phor` |

### 1.2 Kernel (`src/kernel/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/kernel/capability_manager.phor` | Capability creation, transfer, revocation | `core/capabilities.phor`, `core/generations.phor` | Sealed | `kernel_tests.phor` |
| `src/kernel/session_manager.phor` | Session lifecycle, capability gating | `capability_manager.phor` | Sealed | `kernel_tests.phor` |
| `src/kernel/port_manager.phor` | Typed port send/receive | `session_manager.phor` | Sealed | `kernel_tests.phor` |
| `src/kernel/scheduler.phor` | Service dispatch, round-robin, priority | `capability_manager.phor` | Sealed | `kernel_tests.phor` |
| `src/kernel/ipc.phor` | Typed IPC channels, capability verification | `port_manager.phor`, `capability_manager.phor` | Sealed | `kernel_tests.phor` |
| `src/kernel/process.phor` | Service creation and destruction | `scheduler.phor` | Replayed | `kernel_tests.phor` |
| `src/kernel/memory.phor` | Memory region management | `capability_manager.phor` | Replayed | `kernel_tests.phor` |

### 1.3 Forensic Store (`src/forensic_store/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/forensic_store/store_object.phor` | Object storage and content addressing | `core/types.phor`, `core/hashes.phor` | Sealed | `store_tests.phor` |
| `src/forensic_store/resolve.phor` | Object resolution by key | `store_object.phor` | Sealed | `store_tests.phor` |
| `src/forensic_store/verify.phor` | Object integrity and signature verification | `store_object.phor` | Sealed | `store_tests.phor` |
| `src/forensic_store/gc.phor` | Garbage collection, reachability, audit retention | `store_object.phor` | Replayed | `store_tests.phor` |
| `src/forensic_store/generations.phor` | Generation activation, atomic transitions | `store_object.phor` | Sealed | `store_tests.phor` |
| `src/forensic_store/profiles.phor` | Capability/effect/trust policy profiles | `store_object.phor`, `core/effects.phor` | Replayed | `store_tests.phor` |
| `src/forensic_store/key_index.phor` | Hash key indexing and collision resolution | `store_object.phor` | Replayed | `edge_cases.phor` |

### 1.4 Replay Courts (`src/courts/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/courts/session.phor` | Court session lifecycle management | `core/trust.phor`, `core/residuals.phor` | Sealed | `court_tests.phor` |
| `src/courts/oracle.phor` | Oracle connection and reference execution | `session.phor` | Replayed | `court_tests.phor` |
| `src/courts/replay.phor` | Replay engine for deterministic re-execution | `session.phor` | Replayed | `court_tests.phor` |
| `src/courts/comparison.phor` | Byte-identical, structural, behavioral, divergence | `oracle.phor`, `replay.phor` | Replayed | `court_tests.phor` |
| `src/courts/verdict.phor` | Verdict issuance (Accept/Refine/Reject/MoreEvidence) | `comparison.phor` | Replayed | `court_tests.phor` |
| `src/courts/promotion.phor` | Trust ladder progression from verdicts | `verdict.phor`, `core/trust.phor` | Replayed | `court_tests.phor` |
| `src/courts/divergence.phor` | Divergence classification and analysis | `comparison.phor` | Observed | `court_tests.phor` |

### 1.5 Dialect Cages (`src/dialect_cages/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/dialect_cages/profile.phor` | Dialect profile definition and versioning | `core/types.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/cage.phor` | Cage creation, code loading, capability tracking | `profile.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/translation.phor` | Operation translation maps (observed→native) | `cage.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/expansion.phor` | Macro/template/include expansion receipts | `translation.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/c_import.phor` | C dialect import cage | `cage.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/posix.phor` | POSIX dialect translation to native store/IPC | `translation.phor` | Observed | `cage_tests.phor` |
| `src/dialect_cages/nesting.phor` | Cage nesting depth control and isolation | `cage.phor` | Observed | `edge_cases.phor` |

### 1.6 GUI / Compositor (`src/gui/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/gui/compositor.phor` | Compositor creation, surface management | `core/collections.phor` | Observed | `gui_tests.phor` |
| `src/gui/surface.phor` | Surface state machine (initial→configured→mapped→unmapped→destroyed) | `compositor.phor` | Observed | `gui_tests.phor` |
| `src/gui/input.phor` | Input event routing, capability-gated channels | `compositor.phor` | Observed | `gui_tests.phor` |
| `src/gui/window_manager.phor` | Window position, size, focus, z-order | `surface.phor` | Observed | `gui_tests.phor` |
| `src/gui/widgets.phor` | Button, label, text input, containers | `input.phor`, `compositor.phor` | Observed | `gui_tests.phor` |
| `src/gui/presentation.phor` | Present scheduling, present receipts | `compositor.phor` | Observed | `gui_tests.phor` |
| `src/gui/events.phor` | Event propagation (capture, bubble, stop) | `widgets.phor` | Observed | `gui_tests.phor`, `edge_cases.phor` |

### 1.7 Drivers (`src/drivers/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/drivers/serial.phor` | UART serial driver (COM1–COM4) | `kernel/capability_manager.phor` | Observed | `driver_tests.phor` |
| `src/drivers/framebuffer.phor` | Linear framebuffer driver | `kernel/memory.phor` | Observed | `driver_tests.phor` |
| `src/drivers/hid.phor` | HID report descriptor parser | `core/collections.phor` | Observed | `driver_tests.phor` |
| `src/drivers/block.phor` | NVMe/AHCI block I/O driver | `kernel/memory.phor`, `kernel/capability_manager.phor` | Observed | `driver_tests.phor` |
| `src/drivers/timer.phor` | HPET/APIC timer driver | `kernel/scheduler.phor` | Observed | `driver_tests.phor` |
| `src/drivers/hotplug.phor` | Driver hotplug arrival/removal management | `kernel/session_manager.phor` | Observed | `edge_cases.phor` |

### 1.8 Package System (`src/package/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/package/container.phor` | Package container structure (.phor-spec) | `core/types.phor` | Sealed | `package_tests.phor` |
| `src/package/source_evidence.phor` | Source evidence capture and verification | `container.phor`, `forensic_store/store_object.phor` | Sealed | `package_tests.phor` |
| `src/package/build_evidence.phor` | Build evidence and recipe validation | `container.phor` | Sealed | `package_tests.phor` |
| `src/package/binary_evidence.phor` | Binary evidence and source-seal enforcement | `container.phor`, `forensic_store/verify.phor` | Sealed | `package_tests.phor` |
| `src/package/signatures.phor` | Developer/auditor signature verification | `container.phor` | Sealed | `package_tests.phor` |
| `src/package/receipt_chain.phor` | Receipt chain verification | `source_evidence.phor`, `binary_evidence.phor` | Replayed | `package_tests.phor` |
| `src/package/activation.phor` | Package activation prerequisites | `signatures.phor`, `core/trust.phor` | Replayed | `package_tests.phor` |

### 1.9 Porting Engine (`src/porting/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/porting/pipeline.phor` | Porting pipeline stages | `dialect_cages/cage.phor`, `courts/session.phor` | Observed | `porting_tests.phor` |
| `src/porting/cleanroom.phor` | Clean-room slice stage | `pipeline.phor` | Observed | `porting_tests.phor` |
| `src/porting/oracle.phor` | Porting oracle comparison | `pipeline.phor`, `courts/oracle.phor` | Observed | `porting_tests.phor` |
| `src/porting/promotion.phor` | Ported component trust promotion | `pipeline.phor`, `courts/promotion.phor` | Observed | `porting_tests.phor` |

### 1.10 Compiler (`src/compiler/`)

| File | Role | Dependencies | Trust State | Covered By |
|---|---|---|---|---|
| `src/compiler/lexer.phor` | Lexical analysis and tokenization | `core/types.phor` | Replayed | `compiler_tests.phor` |
| `src/compiler/parser.phor` | Syntactic analysis (recursive descent) | `lexer.phor` | Replayed | `compiler_tests.phor` |
| `src/compiler/expander.phor` | Macro and import expansion | `parser.phor` | Observed | `compiler_tests.phor` |
| `src/compiler/lowerer.phor` | AST lowering to IR | `parser.phor` | Observed | `compiler_tests.phor` |
| `src/compiler/optimizer.phor` | Bounded optimization passes | `lowerer.phor` | Observed | `compiler_tests.phor` |
| `src/compiler/codegen.phor` | Code generation to machine code | `lowerer.phor` | Observed | `compiler_tests.phor` |
| `src/compiler/effects.phor` | Effect checking and propagation | `parser.phor`, `core/effects.phor` | Replayed | `compiler_tests.phor` |
| `src/compiler/capabilities.phor` | Capability tracking and affinity enforcement | `parser.phor`, `core/capabilities.phor` | Replayed | `compiler_tests.phor` |
| `src/compiler/bounds.phor` | Loop bound verification | `parser.phor`, `core/bounds.phor` | Replayed | `compiler_tests.phor` |
| `src/compiler/diagnostics.phor` | Stable diagnostic code emission | `parser.phor` | Replayed | `regression_tests.phor` |

---

## 2. Test File Module Map

### 2.1 Unit Tests

| File | Dependencies | Role | Trust State |
|---|---|---|---|
| `tests/core_tests.phor` | `src/core/*` | Core language feature tests | Observed |
| `tests/kernel_tests.phor` | `src/kernel/*` | Kernel service tests | Observed |
| `tests/court_tests.phor` | `src/courts/*` | Court session and oracle tests | Observed |
| `tests/store_tests.phor` | `src/forensic_store/*` | Forensic store operation tests | Observed |
| `tests/driver_tests.phor` | `src/drivers/*` | Driver initialization and I/O tests | Observed |
| `tests/cage_tests.phor` | `src/dialect_cages/*` | Dialect cage creation and translation tests | Observed |
| `tests/gui_tests.phor` | `src/gui/*` | Compositor, surface, input, widget tests | Observed |
| `tests/package_tests.phor` | `src/package/*` | Package container and verification tests | Observed |
| `tests/porting_tests.phor` | `src/porting/*` | Porting pipeline tests | Observed |
| `tests/compiler_tests.phor` | `src/compiler/*` | Compiler pass tests | Observed |

### 2.2 Integration Tests

| File | Dependencies | Role | Trust State |
|---|---|---|---|
| `tests/integration_tests.phor` | All subsystem tests | End-to-end pipeline tests | Replayed |

### 2.3 Framework Tests

| File | Dependencies | Role | Trust State |
|---|---|---|---|
| `tests/test_runner.phor` | All test files | Test discovery, execution, reporting, court verification | Observed |
| `tests/edge_cases.phor` | All subsystem boundary conditions | Edge case validation | Observed |
| `tests/regression_tests.phor` | All previous bug fixes | Regression prevention | Replayed |

### 2.4 Example Files

| File | Dependencies | Role | Trust State |
|---|---|---|---|
| `examples/hello.phor` | `core/*` | Hello World with full language demo | Observed |
| `examples/bounded_loop.phor` | `core/bounds.phor` | Bounded loop verification demo | Observed |
| `examples/effects_demo.phor` | `core/effects.phor` | Effect system demo | Observed |
| `examples/capability_move.phor` | `core/capabilities.phor` | Affine capability move demo | Observed |
| `examples/trusted_block.phor` | `core/trust.phor` | Trusted block rationale demo | Observed |
| `examples/court_example.phor` | `src/courts/*` | Court session lifecycle demo | Promoted |
| `examples/package_example.phor` | `src/package/*` | Package manifest demo | Sealed |
| `examples/gui_example.phor` | `src/gui/*` | GUI compositor demo | Observed |
| `examples/porting_example.phor` | `src/porting/*` | Porting engine demo | Observed |

### 2.5 Compile Test Fixtures

| File | Role | Expected Outcome |
|---|---|---|
| `tests/compile-pass/*.phor` | Programs that must compile successfully | Compilation accepted |
| `tests/compile-fail/*.phor` | Programs that must produce specific diagnostics | Compilation rejected with expected E-code |
| `tests/test_minimal.ph` | Minimal test grammar | Legacy compatibility check |
| `tests/test_minimal2.ph` | Second minimal test | Legacy compatibility check |
| `tests/test_arr.ph` | Array operation test | Legacy compatibility check |

---

## 3. Infrastructure Module Map

### 3.1 Compiler (`phorc/`)

| Path | Role | Language |
|---|---|---|
| `phorc/src/` | Phorensic compiler implementation | Rust |
| `phorc/src/lexer.rs` | Lexical analysis (Rust) | Rust |
| `phorc/src/parser.rs` | Syntactic analysis (Rust) | Rust |
| `phorc/src/effects.rs` | Effect checking (Rust) | Rust |
| `phorc/src/capabilities.rs` | Capability tracking (Rust) | Rust |
| `phorc/src/bounds.rs` | Loop bound verification (Rust) | Rust |
| `phorc/src/codegen.rs` | Code generation (Rust) | Rust |
| `phorc/check_test.exe` | Pre-built test binary | Rust (compiled) |
| `phorc/liblib.rlib` | Pre-built library archive | Rust (compiled) |
| `phorc/Cargo.toml` | Cargo build manifest | TOML |

### 3.2 Host Runtime (`phost/`)

| Path | Role | Language |
|---|---|---|
| `phost/src/` | Host runtime implementation | Rust |
| `phost/src/boot.rs` | Boot path: ASM stub → UEFI GOP → StatusScreen → Console → Shell | Rust |
| `phost/src/canvas.rs` | Canvas / shape / text / compositing primitives | Rust |
| `phost/src/console.rs` | Scrolling text console on framebuffer | Rust |
| `phost/src/shell.rs` | Minimal shell with command support (help, clear, echo, ver) | Rust |
| `phost/src/status_screen.rs` | Boot phase display, progress bar, log messages | Rust |
| `phost/src/kernel/` | Kernel service stubs and capability routing | Rust |
| `phost/nucleus/` | Trusted machine nucleus | Assembly + Rust |
| `phost/Cargo.toml` | Cargo build manifest | TOML |

### 3.3 Test Fixtures (`fixtures/`)

| Path | Role |
|---|---|
| `fixtures/courts/` | Court session fixture data |
| `fixtures/golden/` | Golden reference outputs |
| `fixtures/oracles/` | Oracle reference implementations |
| `fixtures/packages/` | Sample sealed packages |

### 3.4 Documentation (`docs/`)

| File | Role |
|---|---|
| `docs/PHORENSIC_GRAMMAR.md` | Formal BNF grammar specification |
| `docs/PHORENSIC_LANGUAGE.md` | Language reference manual |
| `docs/FORENSIC_OS_VISION.md` | OS architecture and vision |
| `docs/REPLAY_COURTS.md` | Court and oracle architecture |
| `docs/TRUSTED_NUCLEUS.md` | Trusted nucleus specification |
| `docs/SEALED_PACKAGE_FORMAT.md` | Package container format |
| `docs/DIALECT_CAGES.md` | Dialect cage architecture |
| `docs/GUI_DIALECT_PLAN.md` | GUI system plan |
| `docs/DRIVER_DIALECT_PLAN.md` | Driver model plan |
| `docs/FORENSIC_STORE.md` | Forensic store architecture |
| `docs/JIT_CLEANROOM_PORTING.md` | Porting engine architecture |
| `docs/PHORENSIC_COMPILER.md` | Compiler architecture |
| `docs/PHORENSIC_COOKBOOK.md` | Language cookbook / patterns |
| `docs/PHORENSIC_OS.md` | OS overview |
| `docs/REVIEWER_PROTOCOL.md` | Code review protocol |
| `docs/CACHYOS_MEZZANINE.md` | CachyOS mezzanine integration |

---

## 4. Dependency Graph

```
                     ┌─────────────────────────────┐
                     │       Core Language          │
                     │  (types, collections, enums, │
                     │   structs, effects, caps,    │
                     │   trust, generations,        │
                     │   residuals, bounds)         │
                     └──────────────┬──────────────┘
                                    │
            ┌───────────────────────┼───────────────────────┐
            │                       │                       │
            ▼                       ▼                       ▼
   ┌──────────────┐      ┌──────────────────┐      ┌──────────────┐
   │   Kernel     │      │  Forensic Store  │      │   Compiler   │
   │  (cap mgr,   │◄─────│  (object, verify,│◄─────│  (lexer,     │
   │   session,   │      │   gc, gen,       │      │   parser,    │
   │   scheduler, │      │   profiles)      │      │   effects,   │
   │   ipc)       │      └────────┬─────────┘      │   bounds)    │
   └──────┬───────┘               │                 └──────┬───────┘
          │                       │                        │
          ▼                       ▼                        ▼
   ┌──────────────┐      ┌──────────────────┐      ┌──────────────┐
   │   Drivers    │      │   Replay Courts  │      │   Package    │
   │  (serial,    │      │  (session,       │      │  (container, │
   │   fb, hid,   │      │   oracle, replay,│      │   evidence,  │
   │   block,     │      │   comparison,    │      │   signatures) │
   │   timer)     │      │   verdict)       │      └──────┬───────┘
   └──────────────┘      └────────┬─────────┘             │
                                  │                        │
          ┌───────────────────────┼───────────────────────┐│
          │                       │                        ││
          ▼                       ▼                        ▼▼
   ┌──────────────┐      ┌──────────────────┐      ┌──────────────┐
   │  Dialect     │      │   GUI / Compos.│      │   Porting    │
   │  Cages       │      │  (surface,      │      │  (pipeline,  │
   │  (profile,   │      │   input, wm,    │      │   cleanroom, │
   │   translation)│     │   widgets,       │      │   promotion) │
   └──────────────┘      │   events)       │      └──────────────┘
                         └────────────────┘

### Boot Path (Rust host runtime — operational)
```
ASM stub
   │
   ▼
UEFI GOP framebuffer init  ──►  BootParams to Rust main
   │
   ▼
StatusScreen (boot phases, progress bar, log messages)
   │
   ▼
Canvas module (shapes, text, compositing primitives)
   │
   ▼
Scrolling Console (framebuffer text console)
   │
   ▼
Minimal Shell (help, clear, echo, ver)
   │
   ▼
   Kernel service dispatch (capability routing, driver init)
```

### Dependency Rules

1. **Core is foundation** — All subsystems depend on Core types and effects. No circular dependencies with Core.
2. **Kernel provides capabilities** — Drivers, GUI, and Porting depend on Kernel for capability management.
3. **Store is data layer** — Package system and Courts depend on Store for object persistence.
4. **Compiler is standalone** — Compiler depends only on Core; it does not depend on Kernel or Store.
5. **Courts bridge Store and Trust** — Courts depend on Store for evidence and Core/Trust for promotion.
6. **Porting depends on both Dialect Cages and Courts** — Porting translates foreign code via cages and verifies via courts.
7. **No cyclic dependencies** — All dependency edges go from higher-level subsystems to lower-level ones. Core is acyclic at the bottom.

---

## 5. Trust State Summary

| Trust State | Modules | Criteria |
|---|---|---|
| **Promoted (6)** | `examples/court_example.phor` | Full court verification, multiple oracles, all evidence sealed |
| **Sealed (5)** | Core types, collections, effects, capabilities, bounds, Store objects, Package containers, Compiler passes | Full evidence chain, all signatures, source-seal rule enforced, court replay passes |
| **Replayed (4)** | Kernel memory, Store GC, profiles, Store key index, Compiler diagnostics, Court comparison/verdict/promotion, Receipt chain, Package activation | Replay matches oracle, residuals stable across 3+ runs |
| **Observed (3)** | Store resolve + verify, Court divergence, Dialect cage translation, all Driver tests, all GUI tests, Porting pipeline, Compiler optimizer + codegen | At least one observed execution, no errors |
| **Unknown (0)** | Not-yet-tested code paths | No evidence collected |

---

*End of Module Map*
