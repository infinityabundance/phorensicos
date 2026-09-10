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

> `phost_kernel` is a `no_std` staticlib cross-compiled for
> `x86_64-unknown-none`, so it is excluded from the default workspace build. Use
> `build_kernel.sh` (it sets the target via `phost_kernel/.cargo/config.toml`).

## JIT-Porting Court

Phorensic OS ports *behavior*, not binaries. The first court target is libc
`toupper`, ported at the API boundary:

```text
foreign behavior → dialect cage       (observe libc toupper as a black box)
                 → oracle traces      (256 sealed cases, 0x00..0xff)
                 → behavior signature (combined SHA-256 oracle hash)
                 → native candidate   (clean-room phor_toupper)
                 → replay court       (replay all 256 cases)
                 → comparison         (exact output / status / effects)
                 → promotion          (only on an exact match)
                 → sealed package     (native:libc:toupper)
```

```sh
cargo run -p phost -- port promote toupper   # observe → replay → promote
./verify_jit_porting_court.sh                # 256/256, hashes MATCH, Sealed
```

Verified: **256 observed, 256 replayed, 256 passed, 0 failed**, oracle and
candidate hashes MATCH, promotion `Sealed`. The evidence set is committed under
`phost/evidence/porting/toupper/`.

This is **API-surface** JIT-porting (byte-in/byte-out functions), not arbitrary
binary translation — eager JIT of arbitrary foreign binaries is a later phase.
See `docs/REPLAY_COURTS.md` and `docs/PHORENSIC_OS.md`.

## Reproducible runs with Docker

Two minimal, resource-capped containers (QEMU boots inside the kernel one):

```sh
docker compose run --rm host     # cargo test + the JIT-porting court verifier
docker compose run --rm kernel   # build kernel + QEMU boot + evidence verify
docker compose up --build        # both, one shot
```

`host` runs the compiler/runtime tests and the court verifier. `kernel` builds the
Multiboot kernel, boots it under QEMU, verifies the boot evidence (13 checks) and
checks the boot evidence is byte-reproducible against the committed
`phost_kernel/evidence_manifest.json`. Boot evidence is byte-identical across
hosts and containers; kernel *image* bytes additionally depend on the linker
toolchain (`nasm`/`ld.lld`/binutils), which the pinned `docker/Dockerfile.*`
images record.

## Current status

All numbers below were reproduced on a clean checkout.

| Check | Result |
|-------|--------|
| `cargo test` (phorc) | **43 / 43 pass** |
| `cargo test` (phost) | **77 pass, 0 fail, 4 ignored** (the 4 ignored read privileged CR0/CR2/CR3/CR4 and require ring 0) |
| `.phor` / `.ph` → ELF64 | all corpus sources emit objects: `src/` (232), `examples/` (45), `tests/` (34 `.phor`), `tests/compile-pass/` (46) |
| Compiler pipeline | `hello.phor` → 6960-byte ELF64 relocatable + receipts + sealed package |
| Seal verification | source hash **MATCH**, object hash **MATCH** |
| Court replay | 6 / 6 phases **PASS**, verdict `consistent` |
| Kernel | builds a valid Multiboot v1 image (magic `02 b0 ad 1b`) |
| Kernel boot (QEMU) | `Ph` on COM1 and `0xE9`; 1024×768 boot GUI rendered to the LFB |
| Boot evidence | `verify_evidence.sh`: **13 / 13 checks pass**; manifest committed; evidence byte-reproducible |
| JIT-porting court | `toupper`: **256 observed / 256 replayed / 256 passed / 0 failed**, hashes MATCH, promotion `Sealed` |
| Docker | `docker compose run --rm host` / `kernel` reproduce the tests, the court, and the QEMU boot |

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
- **Kernel image bytes.** Boot evidence is byte-reproducible, but kernel image
  bytes depend on the linker toolchain; cross-toolchain bit-reproducibility is
  not yet asserted.

## Documentation

- `docs/PHORENSIC_LANGUAGE.md`, `docs/PHORENSIC_GRAMMAR.md` — the language.
- `docs/PHORENSIC_COMPILER.md` — pipeline and artifact tiers.
- `docs/FORENSIC_STORE.md`, `docs/REPLAY_COURTS.md` — evidence store and courts.
- `docs/PHORENSIC_OS.md`, `docs/FORENSIC_OS_VISION.md` — the OS.
- `docker-compose.yml`, `docker/` — reproducible host + kernel containers.
- `docs/REVIEWER_PROTOCOL.md` — how to review claims in this repo.
- `VERIFICATION_REPORT.md` — detailed build/boot verification notes.

## License

Licensed under either of **MIT** or **Apache-2.0**, at your option
(`LICENSE-MIT`, `LICENSE-APACHE`).
