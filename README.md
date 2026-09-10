# Phorensicos — Phorensic OS

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
| `examples/` | 44 runnable `.phor` examples (shell, compositor, drivers, courts, porting). |
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
cat evidence/serial.log evidence/debug.log
```

On a successful boot the kernel writes its proof bytes (`Ph`) to both COM1
(`0x3F8`) and the QEMU debug port (`0xE9`), and the 1024×768 boot GUI is rendered
to the linear framebuffer (`evidence/screen.ppm`).

> `phost_kernel` is a `no_std` staticlib cross-compiled for
> `x86_64-unknown-none`, so it is excluded from the default workspace build. Use
> `build_kernel.sh` (it sets the target via `phost_kernel/.cargo/config.toml`).

## Current status

All numbers below were reproduced on a clean checkout.

| Check | Result |
|-------|--------|
| `cargo test` (phorc) | **43 / 43 pass** |
| `cargo test` (phost) | **57 pass, 0 fail, 4 ignored** (ignored: privileged CR0/CR2/CR3/CR4 reads that require ring 0) |
| `.phor` / `.ph` → ELF64 | all corpus sources emit objects: `src/` (232), `examples/` (44), `tests/` (34 `.phor`), `tests/compile-pass/` (46) |
| Compiler pipeline | `hello.phor` → 6960-byte ELF64 relocatable + receipts + sealed package |
| Seal verification | source hash **MATCH**, object hash **MATCH** |
| Court replay | 6 / 6 phases **PASS**, verdict `consistent` |
| Kernel | builds a valid Multiboot v1 image (magic `02 b0 ad 1b`) |
| Kernel boot (QEMU) | `Ph` on COM1 and `0xE9`; 1024×768 boot GUI rendered to the LFB |

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

## Documentation

- `docs/PHORENSIC_LANGUAGE.md`, `docs/PHORENSIC_GRAMMAR.md` — the language.
- `docs/PHORENSIC_COMPILER.md` — pipeline and artifact tiers.
- `docs/FORENSIC_STORE.md`, `docs/REPLAY_COURTS.md` — evidence store and courts.
- `docs/PHORENSIC_OS.md`, `docs/FORENSIC_OS_VISION.md` — the OS.
- `docs/REVIEWER_PROTOCOL.md` — how to review claims in this repo.
- `VERIFICATION_REPORT.md` — detailed build/boot verification notes.

## License

Licensed under either of **MIT** or **Apache-2.0**, at your option
(`LICENSE-MIT`, `LICENSE-APACHE`).
