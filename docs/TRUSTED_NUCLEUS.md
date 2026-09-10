# Trusted Nucleus Specification

## Overview

The Trusted Nucleus is the smallest, most rigorously audited component of the Phorensic OS stack. It is the irreducible machine boundary between the hardware and all higher-level system components. It is line-audited, formally verified where feasible, and contains no unnecessary functionality.

## Design Principles

- **Minimal surface area.** The nucleus does only what hardware access requires.
- **Line-audited.** Every line is reviewed and stamped by multiple auditors.
- **No heap.** All data structures are statically allocated.
- **No recursion.** All operations are iterative or bounded.
- **No dynamic dispatch.** All code paths are statically determined.
- **No FFI.** The nucleus speaks only to hardware.
- **No ambient authority.** The nucleus grants capabilities explicitly.
- **Deterministic.** The same inputs produce the same state transitions.
- **Fail-closed.** Any unexpected condition results in a safe halt or reset.
- **Verified entry and exit.** Every mode transition is audited.

## Nucleus Components

```text
TrustedNucleus =
  CPU entry points
  trap handlers
  mode transition gates
  interrupt controller interface
  memory management unit (minimal)
  capability seed store
  generation counter
  hardware clock interface
  watchdog timer
  debug/serial output
  nucleus integrity checker
  machine configuration registers
```

### CPU Entry Points

The nucleus defines the exact CPU entry protocol:

```text
CPUEntry:
  - on power-on / reset: initialize machine state, load nucleus image, run integrity check
  - on interrupt: save minimal context, dispatch to registered handler, restore context, return
  - on trap: capture trap type and context, transition to kernel via capability gate
  - on syscall-like event (kernel entry): validate caller capability, transition mode
```

### Trap Leaves

Trap leaves transfer control from kernel/user space into the nucleus:

```text
TrapLeave:
  - verify caller identity
  - capture residual (reason, caller, timestamp)
  - transition to nucleus context
  - handle or dispatch
  - return results or fault
```

### Mode Transitions

All mode transitions are explicit and audited:

```text
ModeTransition:
  - user → kernel: through capability-gated trap only
  - kernel → nucleus: through defined call gate only
  - nucleus → kernel: through return gate only
  - kernel → user: through context restore only
  - idle → interrupt: through hardware interrupt only
```

## Signed Nucleus Images

Nucleus images are signed and verified before loading:

```text
NucleusImage =
  machine_image
  image_hash
  source_snapshot
  compilation_receipts
  audit_report
  signatures (n > 2 auditors)
```

The nucleus can verify its own integrity at boot and at runtime:

```text
NucleusIntegrityCheck:
  - measure(text_segment)
  - measure(data_segment)
  - hash(measured_regions)
  - compare_with_stored_hash
  - if mismatch: halt
```

## Memory Management (Minimal)

The nucleus provides only the minimum memory management needed to bootstrap the kernel:

```text
NucleusMemory:
  - fixed page table structures (statically allocated)
  - identity mapping for nucleus itself
  - capability-gated mapping for kernel
  - no demand paging
  - no virtual memory management
  - no TLB shootdown handling
```

The kernel extends memory management after receiving the appropriate capabilities from the nucleus.

## Capability Seed Store

The nucleus holds the root of the capability hierarchy:

```text
CapabilitySeedStore:
  - root capabilities (hardware-granted at boot)
  - capability generation counter
  - capability derivation logs (in fixed ring buffer)
```

Root capabilities include:

- memory access (nucleus-reserved regions)
- interrupt registration
- CPU mode control
- hardware clock access
- watchdog timer control
- debug output

## Generation Counter

The nucleus maintains the system generation counter:

```text
GenerationCounter:
  - monotonic increasing
  - persisted across reboots (in non-volatile storage)
  - used for capability generation tags
  - used for generation snapshot identification
```

## Interrupt Handling

The nucleus handles interrupts minimally:

```text
InterruptHandler:
  - save minimal context (register state)
  - identify interrupt source
  - dispatch to registered kernel handler (if capability-gated)
  - if no handler or unregistered interrupt: ignore or halt based on policy
  - restore context and return
```

All interrupt handlers in the kernel must be registered through capability gating:

```text
register_interrupt_handler(irq, handler_capability) -> Result[(), Error]
  where handler_capability includes right_to_receive_interrupt
```

## Watchdog Timer

The nucleus includes a watchdog timer that fires if the kernel does not check in within a configurable interval:

```text
WatchdogTimer:
  - set(milliseconds) -> Result[(), Error]
  - pet() -> Result[(), Error]
  - on_expiry: capture residual, attempt recovery, or halt
```

## Debug Output

The nucleus provides minimal debug output capability:

```text
DebugOutput:
  - write_byte(byte) -> Result[(), Error]
  - write_string(str) -> Result[(), Error]
  - set_output_port(port_type, port_address) -> Result[(), Error]
```

Supported output types: serial, framebuffer text mode, MMIO register.

## Integrity Checker

The nucleus can verify its own integrity at any time:

```text
NucleusIntegrityChecker:
  - measure_self() -> Measurement
  - verify(measurement) -> Result[(), IntegrityError]
  - log_integrity_residual() -> ResidualRecord
```

## Machine Configuration

The nucleus provides read access to machine configuration:

```text
MachineConfig:
  - cpu_info: vendor, family, model, features
  - memory_map: physical memory regions
  - device_tree: discovered hardware
  - clock_frequency: timer and CPU frequencies
```

Write access to machine configuration is capability-gated and rarely granted.

## Boot Process

```text
1. Power-on / Reset
2. CPU loads nucleus image from ROM/verified media
3. Nucleus performs integrity check
4. Nucleus initializes minimal machine state:
   - page tables (identity map)
   - interrupt descriptors
   - capability seed store
   - generation counter
   - watchdog timer
   - debug output
5. Nucleus loads kernel image (verified, signed)
6. Nucleus grants kernel initial capabilities:
   - memory management capability
   - interrupt registration capability
   - hardware access capabilities
   - store access capability
7. Nucleus transitions to kernel entry point
8. Kernel initializes higher-level services
```

## Residuals Produced by Nucleus

The nucleus produces deterministic residual records for:

- each mode transition
- each interrupt handled
- each capability grant
- each integrity check
- each watchdog event
- each error or fault

These residuals are stored in a fixed ring buffer and retrieved by the kernel.

## Auditing Requirements

Every change to the Trusted Nucleus requires:

1. Formal specification update
2. Line-by-line audit by at least 3 auditors
3. Compilation with full forensic receipts
4. Replay court verification
5. Integrity measurement update
6. Source snapshot update in the sealed package

## Non-Goals

- The nucleus does not manage processes, threads, or scheduling.
- The nucleus does not provide filesystem access.
- The nucleus does not handle network protocols.
- The nucleus does not provide dynamic memory allocation.
- The nucleus does not interpret or execute user code.
- The nucleus does not provide any form of persistence beyond the generation counter.
