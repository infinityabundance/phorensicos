# Forensic Residual-Primacy OS Vision

## Core Thesis

Phorensic OS is a forensic residual-primacy computing substrate where every system component — language, compiler, kernel, trusted machine nucleus, executable container format, package/store system, clean-room black-box porting system, replay courts, oracles, deterministic residual analysis, security policy, GUI, drivers, package management, binary verification, decompilation, rollback, and just-in-time porting — are designed as one coherent stack.

This is not an edge OS. This is not a Unix clone. This is not POSIX-first. This is not a normal compatibility layer. This is not a quick-rundown project.

## Foundational Principle: Evidence-First

The OS is not file-first or process-first. It is evidence-first.

Trust is established through replayable evidence, not only identity, users, groups, package signatures, or permission bits. The kernel stores, indexes, streams, and verifies residual objects. Analysis engines consume residuals, but the kernel substrate remains deterministic and bounded.

Every executable, dialect, build, package, driver, GUI component, kernel object, runtime object, and ported component is a signed, replayable semantic evidence container.

Porting, compilation, verification, decompilation, package management, rollback, and security are kernel-level evidence workflows, not external tools.

## Residual Primacy as Kernel Metadata

Residuals are first-class kernel metadata — like permissions, timestamps, signatures, and capabilities are today — but elevated to primary status.

Every kernel object carries:

```text
Object =
  data
  identity
  capabilities
  provenance
  residual history
  replay state
  trust state
  residual fingerprints
```

The kernel must answer:

- Why does this object exist?
- What source produced it?
- What generated this byte?
- Which compiler pass emitted it?
- Which dialect assumption caused it?
- Which residual changed?
- Can this object be replayed?
- Is this object promoted or provisional?

## Architectural Layers

```text
Machine Root:
  irreducible ASM / CPU entry / trap leaves / mode transitions

Trusted Nucleus:
  tiny line-audited machine boundary

Phorensic Language:
  safe constrained systems language

Phorensic OS:
  runtime kernel, capabilities, scheduler, IPC, GUI, drivers,
  residual metadata, semantic loader, package store, trust courts

Forensic Store:
  immutable sealed semantic packages and generations

Porting Engine:
  just-in-time clean-room black-box dialect import and native promotion

Residual Courts:
  oracle comparison, replay verification, trust promotion

Analysis Layer:
  deterministic residual inference over stored evidence
```

## Separation of Substrate and Analysis

The kernel substrate stores, transports, and verifies evidence. The analysis layer interprets residuals. This separation ensures the kernel remains deterministic and bounded while analysis can be rich, evolving, and computationally intensive.

## Trust Ladder

Every object, service, driver, and package progresses through a formal trust ladder:

```text
unknown
→ observed
→ replayed
→ oracle-compared
→ residual-stable
→ sealed
→ promoted
```

Kernel load policy, driver privilege, service privilege, package activation, and native replacement promotion depend on this ladder.

## Security Implications

Security decisions depend on behavioral evidence, not just identity:

- A driver starts constrained and gains privilege only through courts.
- Service promotion requires replay/court evidence.
- Drift outside a sealed residual envelope becomes a security signal.
- Supply-chain verification includes source/binary/receipt/signature verification.
- The kernel can explain why access or promotion was refused.
- Binary trust is behavioral, not only signer-based.

## Persistent Semantic Memory

The compiler and OS do not throw away reasoning. Every compile, port, package activation, run, failure, and residual enriches semantic memory. Future ports start from accumulated receipts, residuals, and Atlas patterns.

Current compilers are mostly amnesic. This OS remembers why code became a particular executable.

## Non-Goals and Anti-Claims

- **Not UNIX.** Phorensic OS does not inherit the Unix filesystem hierarchy, process model, or security model.
- **Not POSIX-first.** POSIX is an oracle dialect for compatibility observation, not native law.
- **Not a Linux clone.** The kernel architecture is capability-based, evidence-first, and residual-primacy.
- **Not an edge-only OS.** Phorensic OS targets general-purpose computing with forensic rigor.
- **Not a normal compatibility layer.** Foreign code is caged, residualized, and progressively replaced with native semantics.
- **Not automatic blind translation.** Every translated semantic element must be observed, replayed, and court-verified.
- **Not trusting generated code without courts.** Candidate generation is the start; courts verify the end.
- **Not trusting machine-suggested semantics directly.** Suggestions are candidate witnesses only, subject to replay and oracle comparison.
- **Not claiming universal support before residual evidence exists.** Every supported surface must have a residual trace.
- **Not sacrificing forensic precision for quick adoption.** The evidence chain is never truncated for convenience.
- **Not a promise that every foreign program can be natively sealed immediately.** Some programs require extensive dialect mining and court work before promotion is possible.

---

## Implementation Milestone: Framebuffer Boot Path (2026-07-04)

The framebuffer boot screen — a foundational prerequisite for the GUI and compositor vision — is now operational in the Rust host runtime (`phost/`):

| Component | Role |
|---|---|
| `phost/src/boot.rs` | ASM stub → UEFI GOP framebuffer initialization |
| `phost/src/status_screen.rs` | Boot phases, progress bar, log message display |
| `phost/src/canvas.rs` | Shapes, text, and compositing primitives |
| `phost/src/console.rs` | Scrolling text console on framebuffer |
| `phost/src/shell.rs` | Minimal shell with command support |

This completes the visual substrate layer. The next step is connecting this to the `.phor` GUI compositor (`src/gui/`) for full dialect-native window management.
