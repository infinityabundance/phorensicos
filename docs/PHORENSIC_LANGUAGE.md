# Phorensic Language Specification

## Overview

Phorensic is a safe, constrained systems language designed specifically for a forensic residual-primacy OS. It is the OS contract — not merely an implementation language.

## Core Design Constraints

- `no_std` core — no standard library dependency in the kernel or trusted paths
- `no_alloc` core — no heap allocation in trusted code paths
- `no_unsafe` core — unsafe is an explicit, audited, exceptional construct
- No libc as native law — libc is a dialect surface, not a native primitive
- No POSIX as native law — POSIX is an oracle dialect for compatibility cages
- No ambient authority — every capability must be explicitly granted
- No heap in trusted core paths — fixed-capacity collections only
- No FFI in trusted core — foreign dialect interfaces go through dialect cages
- No hidden dynamic dispatch — all dispatch is explicit and traceable
- No unrestricted pointer arithmetic — only bounded, capability-checked access
- No unbounded loops — every loop must have a statically verifiable bound
- No recursion in trusted or kernel core paths unless formally bounded and court-proven

## Memory Model

### Fixed-Capacity Collections

All collections have statically determined maximum capacities. There is no dynamic resizing in trusted paths.

```phor
// Fixed-capacity array
let buf: Array[Byte, 256] = Array[Byte, 256]::zeroed()

// Fixed-capacity map
let table: Map[Key, Value, 64] = Map[Key, Value, 64]::new()
```

### Generation-Tagged Handles

Every handle to a kernel object carries a generation tag. Operations on stale handles fail deterministically.

```phor
let cap: Capability[File, gen(3)] = session.open("/store/object")
// generation tag gen(3) is embedded in the handle type
// if the object is replaced or revoked, gen(3) becomes invalid
// access with stale generation produces a deterministic error
```

### Affine Capabilities

Capabilities follow affine typing — they can be moved but not duplicated. Duplication attempts are rejected at compile time.

```phor
let cap1: Capability[Port] = session.create_port()
let cap2: Capability[Port] = cap1       // move: cap1 is now consumed
// cap1.use()  // COMPILE ERROR: use of moved capability
```

### No Hidden Dynamic Dispatch

All virtual dispatch is explicit through capability tables:

```phor
trait Storage {
    fn read(self, offset: Offset, buf: &mut Array[Byte, 256]) -> Result[ByteCount, StorageError]
    fn write(self, offset: Offset, buf: &Array[Byte, 256]) -> Result[ByteCount, StorageError]
}

// Explicit vtable capability
let driver: Capability[impl Storage] = load_driver("framebuffer.phor-spec")
// dispatch is explicit: driver.read(...)
```

## Control Flow

### Bounded Loops

Every loop must declare its bound. The bound must be statically verifiable.

```phor
// Accepted: bounded loop
for i in 0..256 {
    buf[i] = compute(i)
}

// Accepted: bound from fixed-capacity collection
for i in 0..buf.capacity() {
    process(buf[i])
}

// REJECTED: no bound
for i in 0.. {  // COMPILE ERROR: unbounded loop
    process(buf[i])
}
```

### No Unbounded Recursion

Recursion is prohibited in trusted and kernel core paths unless formally bounded and court-proven. For bounded recursion, the bound must be explicit:

```phor
// Accepted: bounded recursion with court-proven bound
@court_proven(depth = 64, casefile = "tree_walk_court.phor-evidence")
fn walk_tree(node: Node, depth: u64) -> Result[Value, Error]
    where depth <= 64
{
    if depth == 0 { return Err(Error::MaxDepth) }
    match node {
        Node::Leaf(v) => Ok(v),
        Node::Branch(left, right) => {
            let l = walk_tree(left, depth - 1)?;
            let r = walk_tree(right, depth - 1)?;
            Ok(combine(l, r))
        }
    }
}
```

## Effects System

### Explicit Effects

Every function must declare its effects. Undeclared effects are rejected.

```phor
fn read_sensor(port: Port) -> Result[SensorReading, DriverError]
    effects: [io, deterministic]
    // io effect declared; deterministic effect declared

fn log_message(msg: &str) -> Result[(), LogError]
    effects: [io, non_deterministic]
    // non_deterministic must be declared explicitly
```

### Effect Closure

A function may close over effects only if the closure captures them explicitly:

```phor
// Accepted: effect closure with explicit signature
let handler: Closure[fn(Event) -> Result[(), HandlerError], effects: [io, deterministic]]
    = |evt| { process_event(evt) }

// REJECTED: undeclared effect
let handler: Closure[fn(Event) -> Result[(), HandlerError], effects: []]
    = |evt| { log_message("event") }  // COMPILE ERROR: undeclared io effect
```

## Trusted Blocks

Trusted blocks allow limited escape from the safe subset, but only with explicit rationale and court-verifiable justification.

```phor
// Accepted: trusted block with rationale
let ptr = trusted "CPU mode register is at fixed known address 0x1234, verified by hardware manual page 42, court-casefile CPU_MODE_001" {
    asm!("mov {0}, cr0", out(reg) mode_reg)
}

// REJECTED: trusted block without rationale
let ptr = trusted {} {  // COMPILE ERROR: trusted block requires rationale string
    asm!("mov {0}, cr0", out(reg) mode_reg)
}
```

## Typed Errors

All errors are typed. There is no untyped exception mechanism.

```phor
type DriverError = enum {
    NotFound = 0x01,
    Busy = 0x02,
    Timeout = 0x03,
    StaleGeneration = 0x10,
    CapabilityRevoked = 0x11,
}

fn open_driver(name: &str) -> Result[Capability[Driver], DriverError]
```

### Stable Diagnostic Codes

Every error and diagnostic has a stable, documented code. Codes do not change between compiler versions without explicit migration receipts.

## Residual Emission

The compiler emits residuals at every stage. Residuals are first-class outputs:

```phor
// The compiler generates residual records automatically for:
// - each compilation pass
// - each expansion decision
// - each optimization applied
// - each codegen decision
// - each link step
// - each seal operation

// User code can also emit explicit residuals:
@emit_residual("sensor_calibration", "calibration_values_used", cal_values)
```

## Examples

### Capability Move Accepted

```phor
let cap: Capability[Store] = store.open("package.phor-spec")
let moved: Capability[Store] = cap  // accepted: cap is moved
```

### Use-After-Move Rejected

```phor
let cap: Capability[Store] = store.open("package.phor-spec")
let moved: Capability[Store] = cap  // cap consumed
// cap.read(...)  // COMPILE ERROR: use of moved capability
```

### Capability Duplication Rejected

```phor
let cap: Capability[Store] = store.open("package.phor-spec")
// let cap2: Capability[Store] = cap.clone()  // COMPILE ERROR: Capability does not implement Clone
```

### Bounded Loop Accepted

```phor
for i in 0..buf.capacity() {
    buf[i] = transform(buf[i])
}
```

### Missing Bound Rejected

```phor
// for i in 0.. {  // COMPILE ERROR: unbounded loop
//     process(buf[i])
// }
```

### Effect Closure Accepted

```phor
let handler: Closure[fn(Event) -> Result[(), HandlerError], effects: [io]] =
    |evt| { device.write(evt.to_command()) }
```

### Undeclared Effect Rejected

```phor
// let handler: Closure[fn(Event) -> Result[(), HandlerError], effects: []] =
//     |evt| { device.write(evt.to_command()) }  // COMPILE ERROR: io effect undeclared
```

### Trusted Block Accepted with Rationale

```phor
let status = trusted "CPU feature register at 0x4321, verified by reference manual, casefile CPU_FEAT_002" {
    asm!("mov {0}, cr4", out(reg) features)
}
```

### Trusted Block Rejected without Rationale

```phor
// let status = trusted {} {  // COMPILE ERROR: trusted block requires rationale
//     asm!("mov {0}, cr4", out(reg) features)
// }
```

### Stale Generation Handle Failure

```phor
let cap: Capability[Session, gen(5)] = kernel.create_session()
kernel.revoke_generation(5)
// cap.send(...)  // runtime error: StaleGeneration — handle gen(5) is no longer valid
```

### Residual Record Emission

```phor
@emit_residual("compile_pass", "optimization_applied", opt_record)
@emit_residual("parse", "expansion_decision", expansion_log)
```
