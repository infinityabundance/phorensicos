# Phorensic Language Cookbook — 50+ Recipes for Safe, Bounded Systems Programming
## PHORENSIC_COOKBOOK.md

> **Version:** 1.0  
> **Status:** Bootstrapping  
> **Relation:** Companion to PHORENSIC_LANGUAGE.md and PHORENSIC_GRAMMAR.md  
> **License:** Phorensic OS Trust Model — free to reproduce with attribution

---

## Table of Contents

1. [Basic Patterns](#1-basic-patterns)
2. [Capability Patterns](#2-capability-patterns)
3. [Effect Patterns](#3-effect-patterns)
4. [Bounded Loop Patterns](#4-bounded-loop-patterns)
5. [Type Patterns](#5-type-patterns)
6. [Error Handling Patterns](#6-error-handling-patterns)
7. [Store Patterns](#7-store-patterns)
8. [Dialect Cage Patterns](#8-dialect-cage-patterns)
9. [Court Patterns](#9-court-patterns)
10. [Porting Patterns](#10-porting-patterns)
11. [Trusted & Machine Block Patterns](#11-trusted--machine-block-patterns)
12. [Handle & Generation Patterns](#12-handle--generation-patterns)
13. [Service Patterns](#13-service-patterns)
14. [IPC Patterns](#14-ipc-patterns)
15. [Memory Patterns](#15-memory-patterns)
16. [Recursion Patterns](#16-recursion-patterns)
17. [Residual Patterns](#17-residual-patterns)
18. [Pattern Matching & Binding Patterns](#18-pattern-matching--binding-patterns)

---

# 1. Basic Patterns

## Recipe 1.1 — Hello, World

**Problem:** You need to write the canonical first program in Phorensic — a function that outputs a greeting string to the console.

**Solution:**

```phorensic
package "phorensic:hello:v1.0";

fn main(console: Cap(Console)) -> void
    effect [io:write]
{
    let greeting: Str(64) = "Hello, Phorensic OS!";
    console.write(greeting);
}
```

**Explanation:** Every Phorensic program starts with a `package` declaration identifying the package name and version. The `main` function accepts a `Cap(Console)` — an affine capability for console output. Capabilities are the sole mechanism for accessing I/O; there is no ambient authority. The `effect [io:write]` declaration tells the compiler this function performs IO write operations. The `Str(64)` type is a fixed-capacity UTF-8 string of up to 64 bytes — no heap allocation. The function returns `void` since it produces output but no meaningful return value. Capabilities are consumed when passed; the compiler tracks that `console` is moved into the function and cannot be used after the call.

---

## Recipe 1.2 — Factorial (Iterative)

**Problem:** Compute the factorial of a small integer using a bounded loop.

**Solution:**

```phorensic
package "phorensic:factorial:v1.0";

fn factorial(n: u64) -> u64
    effect []
{
    let mut result: u64 = 1;
    let mut i: u64 = 2;

    while i <= n proven {
        result = result * i;
        i = i + 1;
    }

    return result;
}
```

**Explanation:** The `effect []` declaration marks this as a pure function — no side effects, no I/O, no state mutation visible to callers. The `while i <= n proven` loop requires the `proven` keyword because Phorensic forbids unbounded loops. The `proven` annotation is a programmer attestation that this loop terminates. For factorial, termination is trivially proven: `i` starts at 2 and increments toward `n`. The `mut` keyword on `result` and `i` allows rebinding within the scope. Multiplication (`*`) and comparison (`<=`) are primitive operations on `u64`. The return type `-> u64` is inferred but explicitly annotated for clarity.

---

## Recipe 1.3 — Fibonacci (Iterative, Bounded)

**Problem:** Compute the Nth Fibonacci number using a fixed-capacity array and a bounded loop.

**Solution:**

```phorensic
package "phorensic:fibonacci:v1.0";

fn fib(n: u64) -> u64
    effect []
    bound depth 93  // fib(93) is the largest that fits in u64
{
    if n == 0 {
        return 0;
    }

    let mut prev: u64 = 0;
    let mut curr: u64 = 1;
    let mut i: u64 = 2;

    while i <= n proven {
        let next: u64 = prev + curr;
        prev = curr;
        curr = next;
        i = i + 1;
    }

    return curr;
}
```

**Explanation:** This uses the `bound depth 93` annotation to declare that the maximum safe input is 93 (since `fib(93)` is the largest u64 Fibonacci number). The function uses three `u64` variables — all stack-allocated, no heap. The `while` loop requires `proven` because the bound is a runtime parameter. The compiler verifies that `i` increments monotonically toward `n`. The `if` expression at the top handles the base case. Note that Phorensic's `while` loop desugars to an `Expr::WhileLoop` AST node with a `proven` flag set to `true`. An input above 93 would overflow silently in other languages; here the `bound depth` annotation documents the safety limit for court review.

---

## Recipe 1.4 — Prime Sieve (Eratosthenes, Fixed Array)

**Problem:** Find all primes up to a fixed compile-time limit using a sieve.

**Solution:**

```phorensic
package "phorensic:prime_sieve:v1.0";

const SIEVE_SIZE: u64 = 1000;

fn sieve() -> Array(u64, SIEVE_SIZE)
    effect [compute]
{
    let mut is_prime: Array(bool, SIEVE_SIZE) = Array::splat(true);
    is_prime[0] = false;
    is_prime[1] = false;

    let mut p: u64 = 2;
    while p * p < SIEVE_SIZE proven {
        if is_prime[p] {
            let mut mult: u64 = p * p;
            while mult < SIEVE_SIZE proven {
                is_prime[mult] = false;
                mult = mult + p;
            }
        }
        p = p + 1;
    }

    // Collect primes into result array
    let mut result: Array(u64, SIEVE_SIZE) = Array::zeroed();
    let mut count: u64 = 0;
    let mut i: u64 = 0;
    while i < SIEVE_SIZE proven {
        if is_prime[i] {
            result[count] = i;
            count = count + 1;
        }
        i = i + 1;
    }

    return result;
}
```

**Explanation:** `Array(bool, SIEVE_SIZE)` is a fixed-capacity, stack-allocated boolean array. `SIEVE_SIZE` is a compile-time constant `const`. Both `while` loops have `proven` annotations: the outer loop runs while `p * p < SIEVE_SIZE`, the inner marks multiples. The `effect [compute]` declares this function uses compute power only — no I/O, no blocking, no residual emission. Array bounds are checked at compile time: since all indices are loop-bounded by `SIEVE_SIZE`, no runtime bounds check is needed. The result array contains zero-padding after the last prime; callers should read only up to `count`.

---

# 2. Capability Patterns

## Recipe 2.1 — Acquiring a Capability

**Problem:** Obtain a capability from the kernel to access a system resource.

**Solution:**

```phorensic
package "phorensic:cap_acquire:v1.0";

fn read_system_config() -> Result(Str(256), IoError)
    effect [io:read]
{
    let fs_cap: Cap(FileSystem) = kernel.acquire("filesystem");
    let config: Str(256) = fs_cap.read_file("/etc/phorensic/config.json");

    // Capability is consumed (moved) into the read_file call
    // No further access to fs_cap is possible here

    return Ok(config);
}
```

**Explanation:** `kernel.acquire("filesystem")` returns a `Cap(FileSystem)` — an affine capability that authorises filesystem access. Capabilities are the sole source of authority in Phorensic; there is no ambient filesystem access. The `kernel.acquire` call is the only way to obtain capabilities; they cannot be forged, cloned, or created spontaneously. After `fs_cap` is passed to `read_file`, the compiler invalidates the original binding. Any subsequent use of `fs_cap` triggers compile error E0301: "use of moved capability". The capability moves through the call chain, ensuring authority is always explicitly accounted for.

---

## Recipe 2.2 — Moving a Capability Between Functions

**Problem:** Pass a capability through a call chain without losing track of authority.

**Solution:**

```phorensic
package "phorensic:cap_move:v1.0";

fn inner(console: Cap(Console), msg: Str(256)) -> void
    effect [io:write]
{
    console.write(msg);
    // 'console' is moved into write, consumed here
}

fn outer(console: Cap(Console)) -> void
    effect [io:write]
{
    inner(console, "Hello from outer");
    // 'console' is no longer valid — it was moved into inner()
    // Any use here would be E0301
}

fn caller() -> void
    effect [io:write]
{
    let cap: Cap(Console) = kernel.acquire("console");
    outer(cap);
    // 'cap' is no longer valid — moved into outer(), which moved into inner()
}
```

**Explanation:** Capabilities flow through the call graph as affine tokens. `caller` acquires the console capability and moves it to `outer`, which moves it to `inner`, which moves it to `console.write`. At each transfer, the source binding is invalidated. The compiler enforces this with a move analysis pass. If any function along the chain tried to use the capability after the move, the compiler would reject the program. This ensures that console authority has exactly one live binding at every point in the program, making authority transfer auditable and deterministic.

---

## Recipe 2.3 — Restricting a Capability

**Problem:** Derive a restricted sub-capability with reduced authority from a full capability.

**Solution:**

```phorensic
package "phorensic:cap_restrict:v1.0";

fn process_config(full_cap: Cap(FileSystem)) -> Result(Str(256), IoError)
    effect [io:read]
{
    // Restrict full filesystem access to a subtree
    let subtree_cap: Cap(FileSystemPath) = full_cap.restrict("/etc/phorensic/");

    // subtree_cap can only access paths under /etc/phorensic/
    let config: Str(256) = subtree_cap.read_file("config.json");

    // 'full_cap' is still valid if restrict is a borrow
    // Or 'full_cap' is consumed if restrict is a refinement move
    // Signature determines which:
    //   fn restrict(self, path) -> Cap(FileSystemPath)  // consumes
    //   fn restrict(&self, path) -> Cap(FileSystemPath) // borrows

    return Ok(config);
}
```

**Explanation:** Capability derivation uses `.restrict()` to produce a sub-capability with a narrower authority scope. The derivation is logged as residual evidence for audit. The original capability may be consumed or borrowed depending on the `restrict` method's signature (self vs &self). Derivation chains document the principle of least authority: a function that only needs to read `/etc` should not carry authority to write to `/dev`. The restriction is enforced by the kernel at runtime — the sub-capability carries an access mask that the kernel checks on every operation.

---

## Recipe 2.4 — Capability Revocation Pattern

**Problem:** Explicitly revoke a capability after use to prevent further authority.

**Solution:**

```phorensic
package "phorensic:cap_revoke:v1.0";

fn one_time_operation(console: Cap(Console)) -> void
    effect [io:write, residual]
{
    console.write("Operation starting...");

    // Perform the work
    let result = do_work();

    // Explicitly drop (can't use after this)
    // - Capabilities are dropped at end of scope automatically
    // - But we can force the drop earlier by moving into a sink
    consume_cap(console);
    // console is now invalid

    // Further operations requiring console would fail at compile time
    // console.write("Done");  // COMPILE ERROR E0301

    residual emit {
        op: "capability.revoked",
        resource: "console",
    };
}

fn consume_cap(_cap: Cap(Console)) -> void
    effect []
{
    // Sink — consumes the capability with no operation
}
```

**Explanation:** Capabilities are automatically dropped at the end of their scope. To explicitly revoke earlier, pass the capability to a sink function that accepts it but does nothing. The compiler immediately invalidates the original binding. The residual emission records the revocation for audit. This pattern is useful for one-time operations, security-critical paths, and capability lifecycle management. The kernel tracks capability lifetimes at the object level; when a capability is dropped, the kernel decrements the reference count on the underlying resource.

---

## Recipe 2.5 — Use-After-Move Detection (Compiler-Enforced)

**Problem:** Prevent the use of a capability after it has been moved to another binding or function.

**Solution:**

```phorensic
package "phorensic:use_after_move:v1.0";

fn consume(cap: Cap(Console)) -> void
    effect [io:write]
{
    cap.write("Consuming capability");
    // cap consumed here — moved into write
}

fn detect() -> void
    effect [io:write]
{
    let console: Cap(Console) = kernel.acquire("console");

    consume(console);

    // The following line would cause a COMPILE ERROR:
    // console.write("After move");  // E0301: use of moved capability 'console'

    // After the move, 'console' is dead. The compiler's move analysis
    // track's each capability's liveness. Any use after move is rejected
    // before code generation.
}
```

**Explanation:** The compiler's move analysis pass tracks each capability's liveness. After `consume(console)` is called, the compiler marks `console` as moved. Any subsequent reference to `console` in the same scope triggers diagnostic E0301. This is a compile-time check — no runtime overhead, no reference counting, no garbage collection. The analysis is conservative: even if `consume` didn't actually use the capability, the move semantics still invalidate the source. This ensures capability tracking is always sound, even in complex control flow.

---

# 3. Effect Patterns

## Recipe 3.1 — Pure Function (No Effects)

**Problem:** Write a function that performs pure computation with no side effects.

**Solution:**

```phorensic
package "phorensic:pure_fn:v1.0";

fn is_even(x: u64) -> bool
    effect []
{
    return x % 2 == 0;
}

fn sum_squares(n: u64) -> u64
    effect []
{
    let mut total: u64 = 0;
    let mut i: u64 = 0;
    while i < n proven {
        let sq: u64 = i * i;
        total = total + sq;
        i = i + 1;
    }
    return total;
}
```

**Explanation:** `effect []` marks a function as pure — no I/O, no blocking, no residual emission, no state mutation. Pure functions are the default; the empty brackets are required syntax. A pure function can call only other pure functions. Calling a function with any effect from a pure function triggers compile error E0401. Pure functions are trivially replayable and court-verifiable: given the same inputs, they always produce the same outputs. The compiler may optimize pure functions more aggressively (common subexpression elimination, loop-invariant code motion) since there are no observable side effects.

---

## Recipe 3.2 — IO Function with Effect Declaration

**Problem:** Write a function that performs I/O and declares its effects explicitly.

**Solution:**

```phorensic
package "phorensic:io_effect:v1.0";

fn read_port(port: u16) -> u8
    effect [machine:ioport]
    court [ioport_read: v3]
{
    // machine:ioport effect required for port I/O
    let value: u8 = inb(port);
    return value;
}

fn write_block(dev: Handle(BlockDevice), block: u64, data: Slice(u8)) -> Result(void, IoError)
    effect [io:write, blocking, residual]
    court [block: v2]
{
    let before: Hash = dev.hash_state();

    let result = dev.write(block, data);
    match result {
        Ok(()) => {
            let after: Hash = dev.hash_state();
            residual emit {
                op: "block.write",
                device: dev.id,
                block: block,
                state_before: before,
                state_after: after,
            };
            Ok(())
        }
        Err(e) => Err(e),
    }
}
```

**Explanation:** Each function declares exactly the effects it performs. `read_port` uses `machine:ioport` for hardware port access. `write_block` needs three effects: `io:write` (writes to a device), `blocking` (the operation may block), and `residual` (emits a residual record). The `court` annotation cites the court version that verified the function's behavior. Effect sets are part of the function's signature — changing effect sets changes the function type. Callers must have a superset of the callee's effects.

---

## Recipe 3.3 — Combined Effect Set

**Problem:** Write a function that combines multiple effect types — compute, I/O, and residual emission.

**Solution:**

```phorensic
package "phorensic:combined_effects:v1.0";

fn process_and_log(data: Slice(u8)) -> Result(Hash, ProcessError)
    effect [compute, io:read, io:write, residual]
    court [processing: v4]
{
    // Compute phase (effect: compute)
    let checksum: Hash = hash(data);

    // IO read phase
    let config: Str(256) = config_service.read("processing.conf");

    // IO write phase
    let result = storage.write("processed.dat", data);

    // Residual emission
    residual emit {
        op: "data.processed",
        bytes: data.len(),
        checksum: checksum,
        config_version: config,
    };

    return Ok(checksum);
}
```

**Explanation:** The effect set `[compute, io:read, io:write, residual]` covers all operations this function performs. The compiler checks that every call within the function body has its effects covered by this set. If a callee introduced a new effect (e.g., `blocking`), compilation would fail. Combined effect sets are the union of all reachable callee effects. This forces developers to be explicit about what side effects their code may have, making programs auditable without reading the full call graph. The court annotation `[processing: v4]` identifies which court version verified this combined effect pattern.

---

## Recipe 3.4 — Effect Propagation Through Call Chain

**Problem:** Ensure that effects correctly propagate through a chain of function calls.

**Solution:**

```phorensic
package "phorensic:effect_propagation:v1.0";

fn reader(path: Str(256)) -> Str(256)
    effect [io:read]
{
    return filesystem.read(path);
}

fn writer(path: Str(256), data: Str(256)) -> void
    effect [io:write]
{
    filesystem.write(path, data);
}

// OK: caller's effect set includes all callee effects
fn read_write(src: Str(256), dst: Str(256)) -> void
    effect [io:read, io:write]
{
    let data = reader(src);  // needs io:read — satisfied
    writer(dst, data);       // needs io:write — satisfied
}

// ERROR: missing io:write
fn read_only(src: Str(256), dst: Str(256)) -> void
    effect [io:read]
{
    let data = reader(src);
    // writer(dst, data);  // COMPILE ERROR E0401: needs io:write
}
```

**Explanation:** Effect propagation follows the subset rule: `E(caller) ⊇ E(callee)` must hold at every call site. The `read_only` function cannot call `writer` because its effect set `[io:read]` is not a superset of `writer`'s `[io:write]`. This is checked at compile time — the effect analysis pass traverses the call graph and verifies the subset relationship. Effect sets are monotonic: adding effects to a function never breaks its callers (but may break its callees). The compiler emits diagnostic E0401 on the first mismatched call.

---

## Recipe 3.5 — Effect Mismatch Detection

**Problem:** Detect and diagnose effect mismatches at compile time.

**Solution:**

```phorensic
package "phorensic:effect_mismatch:v1.0";

fn needs_residual() -> void
    effect [residual]
{
    residual emit {
        op: "test.operation",
    };
}

// Correct — includes residual
fn correct_caller() -> void
    effect [compute, residual]
{
    needs_residual();  // OK
}

// Incorrect — missing residual
fn incorrect_caller() -> void
    effect [compute]
{
    // needs_residual();  // COMPILE ERROR E0401:
    //                    // call to needs_residual() requires effect [residual]
    //                    // but incorrect_caller() declares effect [compute]
}
```

**Explanation:** The compiler produces a clear diagnostic with the exact mismatch. The error message includes:
1. The callee function name
2. The required effect set
3. The caller's declared effect set
4. The specific missing effect

This makes effect mismatches easy to fix: add the missing effect to the caller's declaration, or refactor to avoid calling the effectful function. The diagnostic code E0401 is permanent and documented — code search tools can find all effect mismatches across a codebase. `incorrect_caller` cannot even compile, ensuring effect correctness is always enforced before any binary is produced.

---

# 4. Bounded Loop Patterns

## Recipe 4.1 — For Loop Over a Fixed Array

**Problem:** Iterate over every element of a fixed-capacity array.

**Solution:**

```phorensic
package "phorensic:for_array:v1.0";

fn zero_array(arr: Array(u8, 256)) -> Array(u8, 256)
    effect [compute]
{
    let mut result: Array(u8, 256) = arr;

    for i in 0..256 {
        result[i] = 0;
    }

    return result;
}
```

**Explanation:** `for i in 0..256` iterates `i` from 0 to 255 (exclusive upper bound). The bound `256` is a compile-time constant, satisfying Phorensic's requirement that all loops have provable bounds. The range `0..256` desugars to a `Range` expression with a fixed start and end. The compiler can verify that every array access `result[i]` stays within bounds because `i < 256 == result.capacity()`. No runtime bounds check is needed. The loop bound is statically proven, so no `proven` annotation is required.

---

## Recipe 4.2 — While Loop with Proven Bound

**Problem:** Write a while loop with a runtime-determined bound that requires a proven annotation.

**Solution:**

```phorensic
package "phorensic:while_proven:v1.0";

fn countdown(start: u64) -> void
    effect [io:write]
{
    let console: Cap(Console) = kernel.acquire("console");
    let mut i: u64 = start;

    while i > 0 proven {
        console.write_int(i);
        i = i - 1;
    }

    console.write("Liftoff!");
}
```

**Explanation:** The `while` loop has a runtime-variable bound (`start` is a parameter). The compiler cannot statically prove that `i` reaches 0, so the `proven` annotation is required. This is the programmer's attestation that the loop terminates. The `proven` keyword is checked syntactically in the parser — `while cond proven { body }`. Without `proven`, the compiler emits E0501. For `countdown`, termination is obvious: `i` starts at `start` and decrements toward 0. The provenance of such bounds can be formally verified in a court for higher trust levels.

---

## Recipe 4.3 — Nested Bounded Loops

**Problem:** Write nested loops where each level has its own provable bound.

**Solution:**

```phorensic
package "phorensic:nested_loops:v1.0";

fn matrix_multiply(a: Array(Array(i64, 4), 4),
                   b: Array(Array(i64, 4), 4)) -> Array(Array(i64, 4), 4)
    effect [compute]
    bound depth 4
{
    let mut result: Array(Array(i64, 4), 4) = Array::zeroed();

    let mut i: u64 = 0;
    while i < 4 proven {
        let mut j: u64 = 0;
        while j < 4 proven {
            let mut k: u64 = 0;
            while k < 4 proven {
                result[i][j] = result[i][j] + a[i][k] * b[k][j];
                k = k + 1;
            }
            j = j + 1;
        }
        i = i + 1;
    }

    return result;
}
```

**Explanation:** Each nested `while` loop has its own `proven` annotation. The bounds are all compile-time constants (4×4 matrix). The function has `bound depth 4` — the maximum nesting depth. All array accesses are statically in-bounds. The performance characteristics are deterministic: 64 multiplications (4×4×4) with no heap allocation. The `Array(Array(i64, 4), 4)` type is a 2-dimensional fixed array on the stack. The `bound depth` annotation on the function documents the maximum nesting level for court verification.

---

## Recipe 4.4 — Bounded While with Capacity Guard

**Problem:** Iterate over a fixed-capacity collection at runtime, proving the loop bound from the collection's capacity.

**Solution:**

```phorensic
package "phorensic:capacity_guard:v1.0";

fn process_buffer(buf: Array(u8, 1024), count: u64) -> void
    effect [compute]
{
    // Guard: ensure count does not exceed capacity
    let actual: u64 = if count > buf.capacity() {
        buf.capacity()
    } else {
        count
    };

    let mut i: u64 = 0;
    while i < actual proven {
        transform(buf[i]);
        i = i + 1;
    }
}
```

**Explanation:** The runtime value `count` is guarded by `buf.capacity()` (a compile-time constant `1024`). The `proven` annotation is still needed because `actual` is a runtime value. However, the guard ensures `i < actual ≤ 1024`, so array accesses are always in-bounds. The compiler trusts the `proven` annotation but verifies the capacity guard separately. This pattern is the standard approach for bounded iteration over variable-length data in fixed-capacity buffers — no heap, no unbounded iteration, no out-of-bounds access.

---

# 5. Type Patterns

## Recipe 5.1 — Struct Composition

**Problem:** Compose data from multiple fields using a struct.

**Solution:**

```phorensic
package "phorensic:struct_composition:v1.0";

struct NetworkConfig
    layout default
    mtu: u16,
    flags: u8,
    dns_server: Str(64),
    domain: Str(256),
}

fn create_default_config() -> NetworkConfig
    effect []
{
    let config: NetworkConfig = NetworkConfig {
        mtu: 1500,
        flags: 0,
        dns_server: "8.8.8.8",
        domain: "local",
    };
    return config;
}

fn apply_config(cfg: NetworkConfig) -> void
    effect [io:write]
{
    let console: Cap(Console) = kernel.acquire("console");
    console.write(cfg.domain);
}
```

**Explanation:** `struct` declarations create named product types. Fields are accessed with dot notation: `cfg.domain`. The `layout default` specifier uses natural alignment — each field is aligned to its size. Structs are value types; assignment copies all fields. Capabilities stored inside structs follow affine semantics: moving the struct moves the contained capability. Struct literals require all fields to be initialized. The compiler generates field offsets based on the layout spec, and the offset calculation is deterministic and part of the compiler receipt chain.

---

## Recipe 5.2 — Enum Dispatch with Match

**Problem:** Define a tagged union and dispatch on variants using match.

**Solution:**

```phorensic
package "phorensic:enum_dispatch:v1.0";

type IoError = enum {
    NotFound,
    PermissionDenied,
    DeviceFault(u64),
    StaleHandle(u64, u64),
};

fn format_error(err: IoError) -> Str(128)
    effect []
{
    match err {
        IoError::NotFound => {
            return "Not found";
        }
        IoError::PermissionDenied => {
            return "Permission denied";
        }
        IoError::DeviceFault(code) => {
            return "Device fault: " ++ int_to_str(code);
        }
        IoError::StaleHandle(expected, actual) => {
            return "Stale handle: expected " ++ int_to_str(expected)
                   ++ " got " ++ int_to_str(actual);
        }
    }
}
```

**Explanation:** `enum` declarations create tagged union (sum) types. Each variant may carry zero or more payload fields. The `match` expression destructures the variant and extracts payload values. Pattern matching must be exhaustive — the compiler checks that every variant is covered. The `match` expression is an expression (not a statement) in Phorensic's AST: `Expr::Match`. Each arm has a pattern, an optional guard, and a body expression. The `DeviceFault(code)` pattern binds the `u64` payload to `code`. Variants with tuple payloads (`DeviceFault(u64)`) are matched with parenthesized patterns.

---

## Recipe 5.3 — Type Aliases for Complex Types

**Problem:** Create readable aliases for complex or commonly-used type expressions.

**Solution:**

```phorensic
package "phorensic:type_aliases:v1.0";

type DeviceResult = Result(Handle(BlockDevice), IoError);
type BlockBuffer = Array(u8, 512);
type DeviceArray = Array(Handle(BlockDevice), 8);
type HashDigest = Str(64);

fn open_devices(paths: DeviceArray) -> DeviceArray
    effect [io:read]
{
    let mut results: DeviceArray = Array::zeroed();
    for i in 0..8 {
        let dev: Handle(BlockDevice) = storage.open(paths[i]);
        results[i] = dev;
    }
    return results;
}
```

**Explanation:** `type X = Y;` introduces a name alias — `X` is interchangeable with `Y` in all contexts. Type aliases do not create new types; they are transparent to the type checker. Aliases improve readability for complex type expressions like `Result(Handle(BlockDevice), IoError)`. The `HashDigest = Str(64)` alias documents that this string is specifically a hash, not a general-purpose string. Type aliases are resolved at compile time and have zero runtime overhead.

---

## Recipe 5.4 — Generic-Like Patterns with Named Types

**Problem:** Simulate generic programming using type aliases and structural patterns (Phorensic does not have true generics yet).

**Solution:**

```phorensic
package "phorensic:generic_patterns:v1.0";

// Generic-like pattern: define operations for a specific instantiation
type BufferU8 = Array(u8, 256);
type BufferI32 = Array(i32, 256);

fn clear_u8(buf: BufferU8) -> BufferU8
    effect []
{
    let mut result: BufferU8 = buf;
    for i in 0..256 {
        result[i] = 0;
    }
    return result;
}

fn clear_i32(buf: BufferI32) -> BufferI32
    effect []
{
    let mut result: BufferI32 = buf;
    for i in 0..256 {
        result[i] = 0;
    }
    return result;
}

// Alternative: pass capacity as compile-time constant via macro pattern
// (future: const generics Array(T, N) where N is a const parameter)
```

**Explanation:** Without true generic type parameters, Phorensic uses type aliases and manual instantiation. Each concrete instantiation (`BufferU8`, `BufferI32`) is a separate function. This is verbose but explicit — every monomorphization is visible in the source and can be individually court-verified. Future versions of the language may add const generics (`Array(T, const N)`) but explicit instantiation is the current pattern. The compiler may optimize duplicate code paths during codegen, but the source-level duplication is deliberate for auditability.

---

# 6. Error Handling Patterns

## Recipe 6.1 — Result Type with Match

**Problem:** Handle errors from a fallible operation using the Result type.

**Solution:**

```phorensic
package "phorensic:result_match:v1.0";

type IoError = enum {
    NotFound,
    PermissionDenied,
    DeviceFault(u64),
};

fn open_file(path: Str(256)) -> Result(Handle(File), IoError)
    effect [io:read]
{
    let result = filesystem.open(path);
    match result {
        Ok(handle) => Ok(handle),
        Err(e) => Err(e),
    }
}

fn use_file() -> Result(void, IoError)
    effect [io:read]
{
    let result = open_file("/etc/config");

    match result {
        Ok(file) => {
            let data = filesystem.read(file);
            process_data(data);
            Ok(())
        }
        Err(IoError::NotFound) => {
            // Special handling for not-found
            create_default_config();
            Ok(())
        }
        Err(IoError::PermissionDenied) => {
            // No recovery — propagate
            Err(IoError::PermissionDenied)
        }
        Err(IoError::DeviceFault(code)) => {
            // Log and propagate
            log_error("Device fault", code);
            Err(IoError::DeviceFault(code))
        }
    }
}
```

**Explanation:** `Result(T, E)` is a built-in enum type with variants `Ok(T)` and `Err(E)`. Pattern matching on Result is exhaustive — the compiler forces handling of both `Ok` and `Err` variants. Nested pattern matching (`Err(IoError::NotFound)`) destructures both the Result and the inner error type. The function propagates errors it cannot handle by wrapping them in `Err(...)`. The `void` type in `Result(void, IoError)` indicates success carries no data.

---

## Recipe 6.2 — Error Propagation Through Call Chain

**Problem:** Propagate errors through multiple function calls without losing diagnostic information.

**Solution:**

```phorensic
package "phorensic:error_propagation:v1.0";

type ConfigError = enum {
    FileError(IoError),
    ParseError(u64),
    MissingField(Str(64)),
};

fn read_config(path: Str(256)) -> Result(Str(1024), ConfigError)
    effect [io:read]
{
    let result = filesystem.open(path);
    match result {
        Ok(file) => {
            let data = filesystem.read(file, 0..1024);
            Ok(data)
        }
        Err(io_err) => {
            Err(ConfigError::FileError(io_err))
        }
    }
}

fn parse_config(raw: Str(1024)) -> Result(NetworkConfig, ConfigError)
    effect [compute]
{
    if raw.len() == 0 {
        return Err(ConfigError::MissingField("empty config"));
    }
    // ... parsing logic ...
    return Ok(NetworkConfig { mtu: 1500, flags: 0 });
}

fn load_config(path: Str(256)) -> Result(NetworkConfig, ConfigError)
    effect [io:read, compute]
{
    let raw = read_config(path)?;
    // '?' propagation would be sugar for:
    // match raw {
    //     Ok(v) => v,
    //     Err(e) => return Err(e),
    // }
    let config = parse_config(raw);
    match config {
        Ok(c) => Ok(c),
        Err(e) => Err(e),
    }
}
```

**Explanation:** Error types can wrap other error types, preserving the full diagnostic chain. `ConfigError::FileError(IoError)` preserves the underlying IO error. Each function layer adds its own error context. The `?` operator (future Phorensic feature) would short-circuit on `Err`, but explicit match is the current pattern. The error types are sum types, not integers — no magic errno values, no global error state. Every error carries structured data for forensic analysis.

---

## Recipe 6.3 — Result Type with Void Success

**Problem:** Use Result with void for operations that either succeed (no data) or fail.

**Solution:**

```phorensic
package "phorensic:result_void:v1.0";

fn delete_file(path: Str(256)) -> Result(void, IoError)
    effect [io:write]
{
    let result = filesystem.delete(path);
    match result {
        Ok(v) => Ok(()),
        Err(e) => Err(e),
    }
}

fn safe_delete(path: Str(256)) -> Result(void, IoError)
    effect [io:write, io:read]
{
    // Verify file exists first
    let check = filesystem.stat(path);
    match check {
        Ok(info) => {
            if info.is_directory {
                return Err(IoError::PermissionDenied);
            }
            delete_file(path)
        }
        Err(e) => Err(e),
    }
}
```

**Explanation:** `Result(void, IoError)` indicates the operation either succeeds (carrying no data) or fails with an `IoError`. The `Ok(())` syntax uses the unit value `()` (the only inhabitant of the `void` type). This pattern is common for operations like delete, close, flush, or sync. The `void` type in Phorensic is analogous to Rust's `()` — a zero-sized type. Returning `Result(void, E)` is preferred over returning `bool` because it preserves error information and forces callers to handle both success and failure paths.

---

# 7. Store Patterns

## Recipe 7.1 — Creating a Sealed Container

**Problem:** Create a sealed container (package) and add it to the forensic store.

**Solution:**

```phorensic
package "phorensic:seal_container:v1.0";

fn build_and_seal(source_snapshot: Array(u8, 65536),
                  binary: Array(u8, 65536)) -> Result(StoreKey, PackageError)
    effect [compute, io:write, residual]
    court [sealing: v3]
{
    // Compute hashes
    let source_hash: Hash = hash(source_snapshot);
    let binary_hash: Hash = hash(binary);

    // Create source-binary chain signature
    let chain_data = source_hash ++ binary_hash;
    let chain_sig = sign(chain_data, sealing_key);

    // Build container
    let container = SealedContainer {
        header: ContainerHeader {
            magic: "PhSC\0",
            version: 1,
            format: "forensic",
            created_at: kernel.tick_count(),
        },
        source_evidence: SourceEvidence {
            source_snapshot: source_snapshot,
            source_hash: source_hash,
        },
        dialect_evidence: DialectEvidence {
            dialect_profile: "phorensic:v1",
        },
        replay_evidence: ReplayEvidence {
            replay_checkpoints: capture_checkpoints(),
        },
        trust_state: TrustState {
            level: TrustLevel::Observed,
        },
    };

    // Add to store
    let key: StoreKey = store_add(container);

    residual emit {
        op: "package.sealed",
        key: key,
        source_hash: source_hash,
        binary_hash: binary_hash,
    };

    Ok(key)
}
```

**Explanation:** A `SealedContainer` packages source evidence, dialect evidence, build evidence, replay checkpoints, and trust state into a single content-addressed object. The `store_add` function computes a cryptographic key from all evidence fields and adds the container to the forensic store. The `SealedContainer` struct matches the specification in SEALED_PACKAGE_FORMAT.md. The container format supports multiple tiers: `release`, `audit`, `forensic`, and `court`. The residual emission records the sealing event for replay verification.

---

## Recipe 7.2 — Verifying a Sealed Package

**Problem:** Verify the integrity and trust state of a sealed package before use.

**Solution:**

```phorensic
package "phorensic:verify_package:v1.0";

fn verify_package(path: Str(256)) -> Result(TrustState, PackageError)
    effect [io:read, compute]
    court [verification: v3]
{
    let container: SealedContainer = store.load(path);

    // 1. Verify source hash
    let source_hash: Hash = hash(container.source_snapshot);
    if source_hash != container.source_hash {
        return Err(PackageError::SourceHashMismatch(source_hash, container.source_hash));
    }

    // 2. Verify binary hash
    let binary_hash: Hash = hash(container.emitted_binary);
    if binary_hash != container.binary_hash {
        return Err(PackageError::BinaryHashMismatch(binary_hash, container.binary_hash));
    }

    // 3. Verify source↔binary chain signature
    let chain_data = source_hash ++ binary_hash;
    let chain_ok = verify_signature(
        container.chain_signature,
        chain_data,
        container.signer_pubkey,
    );
    if !chain_ok {
        return Err(PackageError::ChainSignatureInvalid);
    }

    // 4. Replay checkpoints
    for i in 0..container.replay_checkpoints.len() {
        let checkpoint = container.replay_checkpoints[i];
        let replay_ok = court.replay(self.id, checkpoint);
        if !replay_ok {
            return Err(PackageError::ReplayMismatch(checkpoint.id));
        }
    }

    // 5. Return trust state
    return Ok(container.trust_state);
}
```

**Explanation:** Package verification is a multi-step process: (1) source hash integrity, (2) binary hash integrity, (3) chain signature verification (ensuring source and binary were sealed together), (4) replay checkpoint verification against the court, and (5) trust state extraction. Each step produces a specific error variant for diagnostic clarity. The verification is performed before a package can be loaded or executed. This pattern is called by the kernel's binary load policy (§10 of PHORENSIC_OS.md).

---

## Recipe 7.3 — Store Operations (Add, Get, Contains)

**Problem:** Perform basic store operations — add an object, retrieve it, and check for existence.

**Solution:**

```phorensic
package "phorensic:store_ops:v1.0";

fn store_add_object(object: StoreObject) -> Result(StoreKey, StoreError)
    effect [io:write, residual]
{
    let key: StoreKey = compute_key(object);

    if store.contains(key) {
        return Err(StoreError::AlreadyExists(key));
    }

    store.write(key, object);

    residual emit {
        op: "store.add",
        key: key,
        size: object.size(),
    };

    Ok(key)
}

fn store_get_object(key: StoreKey) -> Result(StoreObject, StoreError)
    effect [io:read]
{
    let object: StoreObject = store.read(key);

    // Verify integrity on every read
    let computed_key: StoreKey = compute_key(object);
    if computed_key != key {
        return Err(StoreError::KeyMismatch(key, computed_key));
    }

    Ok(object)
}
```

**Explanation:** The store is content-addressed — keys are cryptographic hashes of content. On write, duplicate detection prevents collisions. On read, integrity verification ensures data hasn't been corrupted or tampered with. The `compute_key` function hashes the object's content, dependencies, dialect profile, compiler version, target architecture, effect profile, receipts, oracle identities, court versions, and build environment capsule. Two objects with the same key are semantically identical — a key property for deterministic builds and replay verification.

---

# 8. Dialect Cage Patterns

## Recipe 8.1 — Importing Foreign Code Through a Dialect Cage

**Problem:** Import and call foreign code (e.g., C/POSIX) through a dialect cage with full observation and residual logging.

**Solution:**

```phorensic
package "phorensic:cage_import:v1.0";

dialect cage "posix:file_ops" {
    observe "open(path, flags, mode)" => {
        let cap: Cap(FileSystem) = cage.acquire_dialect_cap("posix:file_ops");
        let native_path: Str(256) = dialect_translate(path);
        let handle = fs.open(native_path);

        residual "posix:open" {
            path: path,
            flags: flags,
            native_handle: handle.id,
        };

        return handle;
    }

    observe "read(fd, buf, count)" => {
        let handle = Handle(File).from_fd(fd);
        let slice = fs.read(handle, 0..count);

        residual "posix:read" {
            fd: fd,
            count: count,
            bytes_actual: slice.len(),
        };

        return slice.len();
    }

    observe "close(fd)" => {
        let handle = Handle(File).from_fd(fd);
        fs.close(handle);

        residual "posix:close" {
            fd: fd,
        };

        return 0;
    }
}
```

**Explanation:** A `dialect cage` declares an isolated translation layer. Each `observe` rule maps a foreign function signature (e.g., POSIX `open`, `read`, `close`) to a native Phorensic implementation. The cage acquires dialect-specific capabilities. Every translation emits a residual record documenting the foreign→native mapping. Foreign calls that have no observation rule are rejected at compile time (E0010). The cage enforces that only observed and residualized operations pass through. This is how POSIX code runs on Phorensic — not by emulating POSIX, but by observing, translating, and residualizing every call.

---

## Recipe 8.2 — Dialect Translation and Logging

**Problem:** Translate a foreign call to a native operation and log the translation as residual evidence.

**Solution:**

```phorensic
package "phorensic:cage_translate:v1.0";

fn cage_translate(cage: Handle(DialectCage), call: ForeignCall)
    -> Result(NativeOperation, TranslationError)
    effect [cage:translate, residual]
{
    let observed: Option(ObservedCall) = cage.lookup(call.name);

    match observed {
        Some(op) => {
            let native_op: Option(NativeOperation) = op.native_mapping;

            match native_op {
                Some(nop) => {
                    residual emit {
                        op: "cage.translate",
                        cage: cage.dialect,
                        call: call.name,
                        parameters: hash(call.params),
                        native_op: nop.name,
                    };
                    Ok(nop)
                }
                None => {
                    residual emit {
                        op: "cage.translate.unmapped",
                        cage: cage.dialect,
                        call: call.name,
                    };
                    Err(TranslationError::NotYetMapped(call.name))
                }
            }
        }
        None => Err(TranslationError::UnknownCall(call.name)),
    }
}
```

**Explanation:** The translation function checks the cage's observed call registry. Known calls are translated to native operations. Unknown calls produce errors. Every outcome emits a residual — either `cage.translate` for successful translations or `cage.translate.unmapped` for unmapped calls. The `cage:translate` effect is required for any cage translation operation. This pattern is described in DIALECT_CAGES.md §4.1. The residual trail enables progressive replacement: over time, unmapped calls can be observed and translated, reducing the cage's dependency on foreign semantics.

---

## Recipe 8.3 — Unknown Foreign Call Handling

**Problem:** Handle a foreign call that has not yet been observed in the dialect cage.

**Solution:**

```phorensic
package "phorensic:cage_unknown:v1.0";

fn handle_unknown_call(cage: Handle(DialectCage), call: ForeignCall)
    -> Result(NativeOperation, TranslationError)
    effect [cage:translate, residual]
{
    // Log the unknown call
    residual emit {
        op: "cage.unknown_call",
        cage: cage.dialect,
        call: call.name,
        params: hash(call.params),
        timestamp: kernel.tick_count(),
    };

    // Apply sandbox policy:
    // - No kernel object capabilities
    // - Slow interpreted path
    // - Fail-closed
    // - Provisional trust state
    return Err(TranslationError::UnknownCall(call.name));
}
```

**Explanation:** Unknown foreign calls are handled according to the cage's sandbox policy: no capabilities, slow path, fail-closed, provisional trust state. The residual emission ensures the unknown call is recorded for future analysis. An unknown call never succeeds silently — the cage is fail-closed. Over time, the analysis layer can examine unknown call residuals and determine whether to add new observations. This pattern is the foundation of progressive dialect replacement: a system initially rejects everything it hasn't observed, and gradually learns valid translations.

---

## Recipe 8.4 — C Surface Scan for C Code Import

**Problem:** Scan imported C code for dialect assumptions, ABI behaviors, and undefined behavior sites.

**Solution:**

```phorensic
package "phorensic:cage_c_scan:v1.0";

fn scan_c_surface(source: Slice(u8)) -> CSurfaceScan
    effect [compute, residual]
{
    let scan: CSurfaceScan = CSurfaceScan {
        macros: extract_macros(source),
        headers: extract_headers(source),
        abi_assumptions: detect_abi_assumptions(source),
        syscalls: detect_syscalls(source),
        undefined_behavior: detect_ub_sites(source),
        compiler_flags: extract_compiler_flags(source),
    };

    for ub_site in scan.undefined_behavior {
        residual "cage:scan:ub" {
            source: ub_site.location,
            ub_type: ub_site.ub_type,
            observed_behavior: ub_site.observed_behavior,
            platform_assumed: ub_site.platform_assumption,
        };
    }

    return scan;
}
```

**Explanation:** The C surface scan extracts all dialect-relevant information from imported C code: macro definitions, header references, ABI assumptions (e.g., struct layout, calling convention), syscall invocations, and undefined behavior sites (e.g., signed overflow, unsequenced side effects). Each UB site produces a residual documenting the observed behavior and the platform assumption that makes it safe. This evidence is used by the court to determine whether the C code's behavior can be faithfully reproduced in native Phorensic. The scan is the first step in any C-to-Phorensic porting pipeline.

---

# 9. Court Patterns

## Recipe 9.1 — Requesting a Court Verdict

**Problem:** Request a verdict from a court to verify an object's behavior against an oracle.

**Solution:**

```phorensic
package "phorensic:court_request:v1.0";

fn request_verification(native_obj: StoreKey, oracle: Option(StoreKey))
    -> Result(CourtVerdict, CourtError)
    effect [court:request, compute, residual]
{
    let court: Handle(Court) = kernel.get_court("oracle:v3");

    let test_cases: Array(TestCase, 100) = court.generate_cases(native_obj, oracle);
    let verdict: CourtVerdict = court.run(native_obj, oracle, test_cases);

    residual emit {
        op: "court.verdict",
        court: court.id,
        object: native_obj,
        oracle: oracle,
        verdict: verdict.result,
        test_cases: test_cases.len(),
        tests_passed: verdict.tests_passed,
    };

    match verdict.result {
        "identical" => Ok(verdict),
        "consistent" => Ok(verdict),
        "divergent" => Err(CourtError::Divergent(verdict.divergences)),
        "inconclusive" => Err(CourtError::Inconclusive(verdict.reason)),
        other => Err(CourtError::UnexpectedResult(other)),
    }
}
```

**Explanation:** Court requests follow a standard flow: (1) obtain a court handle, (2) generate test cases, (3) run the comparison, (4) emit a residual with the verdict, (5) return the result. The `court:request` effect is required for any court interaction. The verdict captures the number of test cases run, passed, and failed, along with a residual fingerprint. Divergent results are recorded in `DiffRecord` entries. The court is a sealed kernel service — it cannot be tampered with, and its behavior is deterministic.

---

## Recipe 9.2 — Promotion Flow (Trust Ladder)

**Problem:** Promote an object through the trust ladder: observed → replayed → oracle-compared → sealed → promoted.

**Solution:**

```phorensic
package "phorensic:promotion_flow:v1.0";

fn promote_object(key: StoreKey, target: TrustLevel) -> Result(TrustLevel, PromotionError)
    effect [court:request, io:read, io:write, residual]
{
    let current: TrustLevel = store_get(key)?.trust_state.level;

    if current >= target {
        return Ok(current);
    }

    // Request promotion verdict from promotion court
    let verdict: CourtVerdict = court_request("promotion:v3", key, None)?;

    match verdict.result {
        "promote" => {
            store_promote(key, target);

            residual emit {
                op: "trust.promote",
                object: key,
                from: current,
                to: target,
                verdict: verdict.id,
            };

            Ok(target)
        }
        "deny" => {
            Err(PromotionError::Denied(verdict.reason))
        }
        "require_more_evidence" => {
            Err(PromotionError::InsufficientEvidence(verdict.reason))
        }
        other => {
            Err(PromotionError::UnexpectedVerdict(other))
        }
    }
}
```

**Explanation:** Promotion follows the trust ladder: `unknown → observed → replayed → oracle-compared → residual-stable → sealed → promoted`. Each promotion step requires a court verdict. The `store_promote` function atomically updates the object's trust state and records the promotion in the store's Merkle tree. A promotion residual is emitted for replay verification. If the court denies promotion, the object remains at its current level and the reason is preserved. This pattern is called by the kernel's driver lifecycle (§7 of PHORENSIC_OS.md) and service promotion (§8).

---

## Recipe 9.3 — Oracle Comparison

**Problem:** Compare a native clean-room implementation against an oracle (known-good reference) to verify behavioral equivalence.

**Solution:**

```phorensic
package "phorensic:oracle_comparison:v1.0";

fn compare_with_oracle(native: StoreKey, oracle: StoreKey) -> Result(ComparisonResult, CourtError)
    effect [court:request, compute, residual]
{
    // Generate test cases from oracle behavior
    let test_cases: Array(TestCase, 1000) = court.generate_cases(native, oracle);

    // Run comparison
    let diff: DiffReport = court.compare(native, oracle, test_cases);

    residual emit {
        op: "oracle.compare",
        native: native,
        oracle: oracle,
        test_cases: test_cases.len(),
        exact_match: diff.exact_match,
        divergences: diff.divergences.len(),
    };

    if diff.exact_match {
        return Ok(ComparisonResult::Identical);
    }

    if diff.divergences.len() == 0 {
        return Ok(ComparisonResult::Identical);
    }

    // Divergences found — record for analysis
    for div in diff.divergences {
        residual emit {
            op: "oracle.divergence",
            test_case: div.test_case_id,
            expected_hash: div.expected_hash,
            actual_hash: div.actual_hash,
            severity: div.severity,
        };
    }

    return Ok(ComparisonResult::Divergent(diff));
}

fn check_crc32() -> Result(void, CourtError)
    effect [court:request, compute]
{
    let result = compare_with_oracle(
        "phorensic:crc32:v1.0",
        "cachyos:zlib:crc32:v1.3.1",
    );

    match result {
        Ok(ComparisonResult::Identical) => {
            // CRC32 byte-identical — can promote
            Ok(())
        }
        Ok(ComparisonResult::Divergent(diff)) => {
            // Analyze divergences
            Err(CourtError::DivergenceDetected(diff))
        }
        Err(e) => Err(e),
    }
}
```

**Explanation:** Oracle comparison is the core of Phorensic's trust model. A native implementation is compared against a known-good oracle (e.g., a CachyOS binary). The court generates test cases, runs both implementations, and produces a `DiffReport`. Exact matches (`identical`) enable promotion. Divergences are recorded as residuals for analysis. The example shows CRC32 comparison — if the native Phorensic CRC32 produces identical outputs to zlib's CRC32 across 1000 test cases, it can be promoted. This pattern is described in REPLAY_COURTS.md §3.2 and §11.

---

## Recipe 9.4 — Replay Checkpoint Verification

**Problem:** Verify an object's replay checkpoints against the court's deterministic replay.

**Solution:**

```phorensic
package "phorensic:replay_checkpoint:v1.0";

fn verify_replay_checkpoints(container: SealedContainer) -> Result(bool, ReplayError)
    effect [court:request, compute]
{
    let checkpoints: Slice(ReplayCheckpoint) = container.replay_checkpoints;

    for i in 0..checkpoints.len() {
        let cp: ReplayCheckpoint = checkpoints[i];

        // Replay from checkpoint
        let result: ReplayResult = court.replay(container.id, cp);

        match result {
            Ok(consistent) => {
                if consistent.output_hash != cp.expected_output_hash {
                    return Err(ReplayError::Mismatch(
                        cp.expected_output_hash,
                        consistent.output_hash,
                    ));
                }
            }
            Err(e) => {
                return Err(ReplayError::ReplayFailed(e));
            }
        }
    }

    return Ok(true);
}
```

**Explanation:** Replay checkpoints capture the state at each stage of compilation or execution. The court replays from each checkpoint and compares the output hash against the expected hash. Mismatches prevent trust promotion. This ensures that the object's behavior is deterministic and reproducible. The replay court is a sealed kernel service (TrustLevel ≥ sealed) — its behavior is as trustworthy as the kernel itself. Multiple successive replays (N ≥ 3) are required for 'replayed' trust level.

---

# 10. Porting Patterns

## Recipe 10.1 — Porting Pipeline: Dialect Cage Stage

**Problem:** Start porting foreign code by placing it in a dialect cage for observation.

**Solution:**

```phorensic
package "phorensic:porting_cage:v1.0";

package "zlib:1.3.1" {
    import_mode: dialect_cage "posix:c89",
    foreign_source: "zlib-1.3.1.tar.gz",
    dialect: "posix:c89",

    stage "cage" {
        // Detected: C89 dialect
        // Cage assigned: posix:c89 cage v2.1
        // Observed operations:
        //   - memcpy, memset, malloc, free
        //   - no threads, no signals, no file I/O
        //   - all operations are compute-only
        // Unknown regions: inline assembly in crc32()
    }
}
```

**Explanation:** The package block declares the foreign code and its dialect. The `import_mode: dialect_cage "posix:c89"` assigns a cage. The `stage "cage"` block documents what was observed during the cage run. This is not code — it's metadata documenting the porting progress. Each stage is a checkpoint in the porting pipeline. The comment-style content within stage blocks is for documentation; the actual observations are stored as residuals. This pattern is the first step of the JIT clean-room porting pipeline (JIT_CLEANROOM_PORTING.md §2.1).

---

## Recipe 10.2 — Writing a Clean-Room Semantic Slice

**Problem:** Write a native Phorensic implementation of an observed foreign behavior (clean-room, without viewing the foreign source).

**Solution:**

```phorensic
package "phorensic:cleanroom_crc32:v1.0";

// Clean-room implementation of CRC32
// Based on observed behavior: zlib's crc32() computes a 32-bit CRC
// using polynomial 0xEDB88320 (reflected)
// Implemented from public IEEE 802.3 specification only
// No zlib source was consulted

const CRC32_TABLE: Array(u32, 256) = compute_crc32_table();

fn compute_crc32_table() -> Array(u32, 256)
    effect [compute]
{
    let mut table: Array(u32, 256) = Array::zeroed();
    let mut n: u64 = 0;

    while n < 256 proven {
        let mut c: u32 = n as u32;
        let mut k: u64 = 0;

        while k < 8 proven {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c = c >> 1;
            }
            k = k + 1;
        }

        table[n] = c;
        n = n + 1;
    }

    return table;
}

fn crc32(buf: Slice(u8), initial: u32) -> u32
    effect [compute]
{
    let mut crc: u32 = initial ^ 0xFFFFFFFF;
    let mut i: u64 = 0;

    while i < buf.len() proven {
        let idx: u64 = ((crc ^ (buf[i] as u32)) & 0xFF) as u64;
        crc = CRC32_TABLE[idx] ^ (crc >> 8);
        i = i + 1;
    }

    return crc ^ 0xFFFFFFFF;
}
```

**Explanation:** This is a clean-room implementation written from public specifications (IEEE 802.3) without viewing zlib source. The implementation is bounded — loops iterate over a compile-time table size (256) or the buffer length with `proven` annotation. No heap allocation — the CRC32 table is a `const` array computed at compile time. No libc dependency. The effect is `[compute]` only — no I/O, no blocking. This slice can be court-verified against the zlib oracle. This pattern is the core of JIT clean-room porting (JIT_CLEANROOM_PORTING.md §2.5).

---

## Recipe 10.3 — Porting: Build Replay Stage

**Problem:** Replay the foreign build process in the cage and capture build receipts.

**Solution:**

```phorensic
package "phorensic:build_replay:v1.0";

fn replay_build(recipe: BuildRecipe) -> Result(BuildEvidence, BuildError)
    effect [io:read, io:write, cage:translate, compute, residual]
{
    let receipts: RingBuf(Receipt, 256) = RingBuf::new();

    for step in recipe.build_steps {
        residual emit {
            op: "build.step",
            step: step.name,
            input_hash: hash(step.inputs),
        };

        let output = cage_run(step.command, step.inputs);

        match output {
            Ok(artifacts) => {
                let receipt: Receipt = Receipt {
                    step: step.name,
                    output_hash: hash(artifacts),
                };
                receipts.push(receipt);
            }
            Err(e) => {
                return Err(BuildError::StepFailed(step.name, e));
            }
        }
    }

    return Ok(BuildEvidence {
        build_recipe: recipe,
        build_receipts: receipts.to_slice(),
    });
}
```

**Explanation:** Build replay runs each build step in the dialect cage and captures receipts for every step. Each receipt records the step name and output hash. The receipts form a chain that enables deterministic rebuild. If any step fails, the entire build fails — no partial outputs. The build environment capsule is part of the receipt chain, ensuring that host-specific factors don't affect the build output. This pattern is used in the JIT porting pipeline stage 2 (JIT_CLEANROOM_PORTING.md §2.2).

---

## Recipe 10.4 — Porting: Court-Verified Native Replacement

**Problem:** Court-verify a native Phorensic slice against the oracle binary before sealing.

**Solution:**

```phorensic
package "phorensic:court_verify_slice:v1.0";

fn court_verify_slice(native_slice: StoreKey, oracle: StoreKey) -> Result(PortingStage, PortError)
    effect [court:request, compute, residual]
{
    // Run replay court
    let verdict: CourtVerdict = request_verification(native_slice, Some(oracle))?;

    match verdict {
        CourtVerdict { result: "identical", .. } => {
            // Slice is byte-identical to oracle — can seal
            Ok(PortingStage {
                stage: "replay_court",
                native_binary: native_slice,
                oracle_binary: oracle,
                test_cases: verdict.test_cases_run,
                court_verdict: verdict.result,
                trust_promotion: TrustLevel::OracleCompared,
            })
        }
        CourtVerdict { result: "residual-stable", .. } => {
            // Residuals match but outputs are identical
            // Minor divergence in non-output behavior (e.g., memory layout)
            Ok(PortingStage {
                stage: "replay_court",
                trust_promotion: TrustLevel::ResidualStable,
                ..default()
            })
        }
        _ => Err(PortError::CourtRejected(verdict)),
    }
}
```

**Explanation:** The court compares the native slice against the oracle binary. Byte-identical results achieve `oracle-compared` trust level. Residual-stable results (outputs match, non-output behavior differs) achieve `residual-stable`. Any divergence below the acceptable threshold prevents promotion. The trust level achieved determines what capabilities the slice can be granted. This pattern is porting pipeline stage 6 (JIT_CLEANROOM_PORTING.md §2.6).

---

## Recipe 10.5 — Full Porting Lifecycle (8 Stages)

**Problem:** Execute the entire porting lifecycle from foreign code ingestion to promoted native package.

**Solution:**

```phorensic
package "phorensic:full_porting:v1.0";

package "zlib:1.3.1" {
    import_mode: dialect_cage "posix:c89",
    foreign_source: "zlib-1.3.1.tar.gz",
    dialect: "posix:c89",

    stage "cage" {
        // Observed: deflate, inflate, crc32, adler32
        // Observed: no threads, no signals, no file I/O
        // Verified: all operations are effect [compute]
    }

    stage "build_replay" {
        build_recipe: "./configure --static && make",
        residual_trace: "cachyos:zlib:1.3.1:build",
    }

    stage "oracle_observation" {
        oracle: "cachyos:zlib:1.3.1",
        observed_behavior: "byte-exact compression/decompression",
    }

    stage "cleanroom_crc32" {
        // Native CRC32 — bounded loops, no alloc, effect [compute]
        // Matches IEEE 802.3 polynomial 0xEDB88320
    }

    stage "cleanroom_deflate" {
        // Native deflate — LZ77 + Huffman
        // Bounded memory: 64KB sliding window (caller-provided buffer)
        // No heap allocation
    }

    stage "replay_court" {
        oracle: "cachyos:zlib:1.3.1:binary",
        cases: 1024,
        verdict: "residual-stable (98.2% byte-match against oracle)",
        // 1.8% divergence: different memory layout (no malloc vs malloc)
        // Resolved: verified as semantically equivalent
    }

    stage "sealed_package" {
        // Full forensic container with all receipts embedded
        trust_state: TrustLevel::Sealed,
    }

    stage "promotion" {
        trust_level: promoted,
        sealed: true,
        replaces_foreign: true,
    }
}
```

**Explanation:** The complete porting lifecycle has 8 stages: (1) cage — observe foreign behavior, (2) build_replay — replay the foreign build, (3) oracle_observation — observe the oracle binary, (4) cleanroom_slice — write native implementations, (5) build the native slice, (6) replay_court — court-verify, (7) sealed_package — seal the container, (8) promotion — promote in the store. Each stage is a documented checkpoint. The foreign source is never directly translated — only observed behavior is reimplemented. This pattern is the full pipeline described in JIT_CLEANROOM_PORTING.md.

---

# 11. Trusted & Machine Block Patterns

## Recipe 11.1 — Trusted Block with Reason

**Problem:** Perform an operation that the compiler cannot verify but is justified by documentation.

**Solution:**

```phorensic
package "phorensic:trusted_reason:v1.0";

fn read_cpu_model() -> u64
    effect [compute]
{
    trusted reason "CPU model register read is safe: documented at AMD APM v3 §2.1.5"
    {
        let model: u64 = machine_read_msr(0x1A);
        return model;
    }
}
```

**Explanation:** A `trusted` block allows operations the compiler cannot prove correct. The `reason` string is mandatory — without it, the compiler emits E0601. The reason must cite a verifiable source (CPU manual, specification, court verdict). Trusted blocks are line-audited during the trusted nucleus audit. The body may use machine operations (like `machine_read_msr`) that would be rejected outside a trusted block. Trusted blocks are the boundary between provable Phorensic code and irreducible machine contact.

---

## Recipe 11.2 — Trusted Block with Court Citation

**Problem:** Use a trusted block backed by a formal court verdict and receipt.

**Solution:**

```phorensic
package "phorensic:trusted_court:v1.0";

fn initialize_timer() -> void
    effect [machine:ioport]
{
    trusted court "timer_init:amd_k8_17h" receipt "rec:timer:0x43:v3"
    {
        // Program PIT channel 0 for periodic interrupt
        // See i8254 datasheet, Intel 82C54 §3.2
        outb(0x43, 0x36);    // Control word: channel 0, mode 3, binary
        outb(0x40, 0x00);    // Low byte of divisor
        outb(0x40, 0x10);    // High byte of divisor (0x1000 = 4096)
    }
}
```

**Explanation:** The `court` and `receipt` annotations provide formal justification: the court `timer_init:amd_k8_17h` has verified that this instruction sequence correctly initializes the timer, and the receipt `rec:timer:0x43:v3` is the cryptographic proof. Court-cited trusted blocks have stronger trust guarantees than reason-only blocks. The `machine:ioport` effect is required for port I/O operations. The instruction sequence (outb calls) is documented and line-auditable against the i8254/82C54 datasheet.

---

## Recipe 11.3 — Machine Block

**Problem:** Declare a formal machine block enumerating CPU instructions, target architectures, and supporting courts.

**Solution:**

```phorensic
package "phorensic:machine_block:v1.0";

machine {
    reason: "CPUID instruction is required to detect CPU features during boot",
    arch: ["x86_64", "aarch64"],
    instructions: ["CPUID", "MSR_READ"],
    courts: ["cpu_spec:v3", "manual:amd_apm_v3:§2.1.5"],
    receipts: ["rec:cpuid:leaf:1:v3", "rec:msr:0x1A:v3"],
    trust_boundary: "wraps irreducible machine contact",

    fn detect_cpu_features() -> CpuFeatures
        effect [machine:ioport]
    {
        trusted reason "CPUID leaf 1 returns feature flags in ECX and EDX"
        {
            let eax: u32;
            let ebx: u32;
            let ecx: u32;
            let edx: u32;
            cpuid(1, eax, ebx, ecx, edx);

            return CpuFeatures {
                sse3: (ecx & 0x1) != 0,
                ssse3: (ecx & 0x200) != 0,
                sse4_1: (ecx & 0x80000) != 0,
                sse4_2: (ecx & 0x100000) != 0,
                avx: (ecx & 0x10000000) != 0,
            };
        }
    }
}
```

**Explanation:** A `machine` block declares the target architectures, CPU instructions, supporting courts, and receipts for a set of machine-level operations. Every machine block must have a `reason` field, `arch` list, `instructions` list, `courts` list, and `receipts` list. The block wraps functions that perform irreducible machine contact (CPUID, MSR access, etc.). Machine blocks are the only place where inline assembly is permitted (diagnostic E0013). The trusted nucleus audit covers every machine block in the system. Machine blocks cannot allocate, recurse, or fail silently.

---

# 12. Handle & Generation Patterns

## Recipe 12.1 — Acquiring and Using a Handle

**Problem:** Acquire a generation-tagged handle to a kernel object and use it safely.

**Solution:**

```phorensic
package "phorensic:handle_acquire:v1.0";

fn read_file() -> Result(Str(256), IoError)
    effect [io:read]
{
    let file: Handle(File) = fs.open("/etc/config");

    // Handle carries an implicit generation tag
    // Handle(File) { id: u64, generation: u64 }

    let data: Str(256) = fs.read(file, 0..256);
    // Kernel checks: file.generation == object.current_generation

    return Ok(data);
}
```

**Explanation:** `Handle(T)` is a generation-tagged reference. The kernel assigns a unique ID and generation counter when the handle is created. Every operation on a Handle triggers a generation check: the kernel compares the handle's stored generation against the kernel object's current generation. If they differ, runtime error E0201 is raised. The generation counter increments on every state-modifying operation (close, delete, invalidate). Handles cannot be forged — they are created only by kernel operations.

---

## Recipe 12.2 — Stale Handle Detection (Runtime)

**Problem:** Detect and diagnose use of a stale handle (one whose generation no longer matches).

**Solution:**

```phorensic
package "phorensic:stale_handle:v1.0";

fn handle_stale_detection() -> Result(void, IoError)
    effect [io:read]
{
    let file: Handle(File) = fs.open("/config");

    // Close the file — this increments the object's generation
    fs.close(file);
    // file.generation == 2, object.generation == 3

    // Attempt to read with stale handle
    // RUNTIME: E0201 — Handle generation 2 does not match object generation 3
    //          Object was closed at call site: fs.close(file)
    // let data = fs.read(file, 0..256);

    // The only valid operation on a stale handle is to drop it
    // (goes out of scope automatically)
}
```

**Explanation:** After `fs.close(file)`, the handle's generation tag is stale. Any subsequent use raises runtime diagnostic E0201. The error message includes the expected generation, actual generation, and the call site where the object was last modified. Stale handle detection is deterministic — the kernel checks every operation. There is no window where a stale handle could succeed (no TOCTOU race). The compiler cannot statically prevent all stale handle patterns, but the runtime detection is fail-closed: on mismatch, the operation is rejected before any memory is accessed.

---

## Recipe 12.3 — Cross-Generation Transaction

**Problem:** Ensure all handles in a multi-step transaction belong to the same generation window.

**Solution:**

```phorensic
package "phorensic:cross_generation:v1.0";

fn atomic_rename(fs: Handle(Filesystem), src: Str(256), dst: Str(256))
    -> Result(void, IoError)
    effect [io:rename]
{
    // Capture current generation
    let gen: u64 = fs.generation();

    // Open both files within the same generation window
    let src_h: Handle(File) = fs.open(src);
    let dst_h: Handle(File) = fs.open(dst);

    // Verify both handles belong to the captured generation
    kernel.assert_generation(src_h, gen);
    kernel.assert_generation(dst_h, gen);

    // Perform the rename — both handles are from the same window
    fs.rename(src_h, dst_h);

    // If any handle's generation had changed, assert_generation
    // would have raised E0201 before the rename
}
```

**Explanation:** Cross-generation transactions use `kernel.assert_generation(h, gen)` to verify all handles share the same generation timestamp. The generation is captured before opening the handles, ensuring the open operations all happen in sequence. This prevents TOCTOU attacks where a handle is invalidated between open and use. The `assert_generation` function is a runtime check — it compares the handle's stored generation against the expected generation and raises E0201 on mismatch.

---

## Recipe 12.4 — Handle Cast from File Descriptor

**Problem:** Convert a foreign file descriptor to a native Phorensic Handle for use in a dialect cage.

**Solution:**

```phorensic
package "phorensic:handle_from_fd:v1.0";

dialect cage "posix:file_ops" {
    observe "read(fd, buf, count)" => {
        // Convert POSIX fd to native Handle
        let handle: Handle(File) = Handle(File).from_fd(fd);

        // Verify generation
        let gen: u64 = fs.generation();
        kernel.assert_generation(handle, gen);

        // Perform native read
        let slice: Slice(u8) = fs.read(handle, 0..count);

        residual "posix:read" {
            fd: fd,
            count: count,
            bytes_actual: slice.len(),
        };

        return slice.len();
    }
}
```

**Explanation:** `Handle(File).from_fd(fd)` creates a native Handle from a POSIX file descriptor. This only works within a dialect cage that has mapped the fd to a native kernel object. The generation assertion ensures the mapped handle is still valid. The `from_fd` operation is cage-specific — different cages may have different fd→handle mappings. This pattern bridges the foreign concept of file descriptors to Phorensic's generation-tagged Handle model. The fd→handle mapping table is maintained by the cage and updated on every open/close.

---

# 13. Service Patterns

## Recipe 13.1 — Service Declaration with Capabilities

**Problem:** Declare a kernel service with explicit capabilities, effects, and trust threshold.

**Solution:**

```phorensic
package "phorensic:service_decl:v1.0";

service FilesystemService {
    capabilities: [
        Cap(BlockDevice),
        Cap(MemoryRegion),
        Cap(IpcEndpoint),
        Cap(SealService),
    ],
    effects: [io:read, io:write, blocking, residual],
    court_threshold: replayed,
    provides: [Cap(FileHandle)],
    imports: [Cap(BlockDevice)],
}
```

**Explanation:** A `service` declaration defines a capability-gated runtime process. The `capabilities` field lists all capabilities the service may exercise. `effects` declares the allowed effect set — the service cannot perform effects outside this set. `court_threshold` sets the minimum trust level required for the service to operate (`replayed` in this case). `provides` lists capabilities the service grants to clients. `imports` lists capabilities the service requires from its environment. Services start at `observed` trust level and promote through the trust ladder. The service declaration is checked at compile time and enforced at runtime by the kernel.

---

## Recipe 13.2 — Service Capability Grant Pattern

**Problem:** Grant a capability to a service with restricted authority.

**Solution:**

```phorensic
package "phorensic:service_cap_grant:v1.0";

fn grant_service_capabilities(service: Handle(Service)) -> Result(void, ServiceError)
    effect [residual]
{
    // Restrict capabilities before granting
    let block_cap: Cap(BlockDevice) = kernel.acquire("block_device");
    let restricted_block: Cap(BlockDeviceRange) = block_cap.restrict(0..1024);

    let mem_cap: Cap(MemoryRegion) = kernel.acquire("memory");
    let restricted_mem: Cap(MemoryRegion) = mem_cap.restrict_read_only();

    // Grant to service
    service.grant(restricted_block);
    service.grant(restricted_mem);

    residual emit {
        op: "service.capabilities_granted",
        service: service.id,
        capabilities: [
            "BlockDeviceRange(sectors=0..1024)",
            "MemoryRegion(read-only)",
        ],
    };
}
```

**Explanation:** Capabilities are restricted before being granted to services (principle of least authority). `Cap(BlockDeviceRange)` restricts block device access to sectors 0–1024. `Cap(MemoryRegion)` restricted to read-only prevents the service from modifying kernel memory. The grant operation is one-way — once granted, the service holds the capability. The grant residual records the exact capabilities and their restrictions for audit. Service capabilities are revoked on service shutdown or trust demotion.

---

# 14. IPC Patterns

## Recipe 14.1 — Capability-Mediated IPC Send

**Problem:** Send a typed message over an IPC channel using a capability.

**Solution:**

```phorensic
package "phorensic:ipc_send:v1.0";

fn send_message(channel: Cap(IpcChannel), message: IpcMessage)
    -> Result(void, IpcError)
    effect [ipc:send, residual]
    court [ipc: v3]
{
    let receipt: IpcReceipt = IpcReceipt {
        from: kernel.current_process(),
        to: channel.peer,
        message_hash: hash(message),
        generation: channel.generation(),
        timestamp: kernel.tick_count(),
    };

    channel.send(message);

    residual emit {
        op: "ipc.send",
        channel: channel.id,
        receipt: receipt,
    };

    // 'channel' capability is consumed (moved into send)
    // No further access to the channel is possible here
}
```

**Explanation:** IPC channels are capability-mediated — `Cap(IpcChannel)` authorises the send operation. Every IPC send emits a residual with the receipt hash, generation, and timestamp. The `ipc:send` effect is required. The channel capability is consumed on send, ensuring the caller cannot send again without acquiring a new capability. This enables fine-grained IPC control: each message send is a distinct authorized operation.

---

## Recipe 14.2 — Structured IPC with Typed Messages

**Problem:** Define structured IPC messages with typed payloads and deterministic serialization.

**Solution:**

```phorensic
package "phorensic:ipc_structured:v1.0";

type IpcMessageType = enum {
    Request(RequestPayload),
    Response(ResponsePayload),
    Error(IpcError),
    Heartbeat,
};

struct RequestPayload
    layout default
    operation: u32,
    args: Array(u8, 256),
    flags: u8,
}

struct ResponsePayload
    layout default
    status: u32,
    data: Array(u8, 4096),
    checksum: u64,
}

fn process_ipc_message(channel: Cap(IpcChannel))
    -> Result(void, IpcError)
    effect [ipc:send, ipc:receive, compute]
{
    let msg: IpcMessage = channel.receive();

    match msg.payload {
        IpcMessageType::Request(req) => {
            let response: ResponsePayload = handle_request(req);
            channel.send(IpcMessage {
                payload: IpcMessageType::Response(response),
            });
        }
        IpcMessageType::Error(e) => {
            // Propagate error
            return Err(e);
        }
        IpcMessageType::Heartbeat => {
            // No-op, keep alive
        }
    }
}
```

**Explanation:** Structured IPC uses typed messages with exhaustively-matched variants. The `IpcMessage` type is an enum with typed payloads (`RequestPayload`, `ResponsePayload`). The `receive` operation requires `ipc:receive` effect. Messages are deterministically serialized based on struct layout. The `match` on message type ensures all variants are handled. This pattern is used for kernel service IPC and driver communication.

---

# 15. Memory Patterns

## Recipe 15.1 — Fixed-Capacity Buffer with Bounds Checking

**Problem:** Use a fixed-capacity ring buffer as a bounded data queue.

**Solution:**

```phorensic
package "phorensic:ring_buffer:v1.0";

const BUF_CAPACITY: u64 = 1024;

fn process_stream(input: RingBuf(u8, BUF_CAPACITY)) -> void
    effect [compute]
{
    let mut buf: RingBuf(u8, BUF_CAPACITY) = input;

    while buf.len() > 0 proven {
        let byte: u8 = buf.pop_front();
        // Process byte
        handle_byte(byte);
    }
}

fn fill_buffer(data: Slice(u8)) -> Result(RingBuf(u8, BUF_CAPACITY), BufferError)
    effect [compute]
{
    let mut buf: RingBuf(u8, BUF_CAPACITY) = RingBuf::new();

    let mut i: u64 = 0;
    while i < data.len() && !buf.is_full() proven {
        buf.push_back(data[i]);
        i = i + 1;
    }

    if i < data.len() {
        return Err(BufferError::Overflow(BUF_CAPACITY, data.len()));
    }

    return Ok(buf);
}
```

**Explanation:** `RingBuf(T, N)` is a fixed-capacity circular buffer. Push operations check capacity — if full, the push is rejected (compile-time overflow or runtime error E0902). The `proven` annotation on while loops satisfies the bounded loop requirement. The ring buffer is stack-allocated and has deterministic performance (O(1) push/pop). No heap allocation is involved. Ring buffers are commonly used for IO buffers, IPC message queues, and residual buffers in Phorensic.

---

## Recipe 15.2 — Object Pool for Fixed-Size Allocations

**Problem:** Implement a fixed-size object pool for kernel objects without heap allocation.

**Solution:**

```phorensic
package "phorensic:object_pool:v1.0";

const MAX_OBJECTS: u64 = 64;

struct ObjectPool(T)  // conceptual — no true generics yet
    layout packed
    objects: Array(T, 64),
    free_bitmap: Array(u64, 1),     // 64 bits in one u64
    generation_counters: Array(u64, 64),
}

fn pool_allocate(pool: ObjectPool(T)) -> Option(u64)
    effect [compute]
{
    let mut i: u64 = 0;
    while i < MAX_OBJECTS proven {
        if (pool.free_bitmap[0] >> i) & 1 == 0 {
            // Mark as allocated
            pool.free_bitmap[0] = pool.free_bitmap[0] | (1 << i);
            pool.generation_counters[i] = pool.generation_counters[i] + 1;
            return Some(i);
        }
        i = i + 1;
    }

    return None;  // Pool exhausted
}

fn pool_free(pool: ObjectPool(T), index: u64) -> Result(void, PoolError)
    effect [compute]
{
    if index >= MAX_OBJECTS {
        return Err(PoolError::InvalidIndex(index));
    }

    if (pool.free_bitmap[0] >> index) & 1 == 0 {
        return Err(PoolError::AlreadyFree(index));
    }

    // Mark as free
    pool.free_bitmap[0] = pool.free_bitmap[0] & !(1 << index);

    // Generation counter was already incremented on allocate
    return Ok(());
}
```

**Explanation:** Object pools are fixed-size allocators with a bitmap tracking free slots. Each allocation increments the generation counter for that slot, enabling stale-handle detection. The pool is stack-allocated at boot time and never grows. The `bound` on the allocation loop is `MAX_OBJECTS` (a compile-time constant). Object pools are the foundation of kernel memory management in Phorensic — there is no general-purpose heap allocator in trusted paths. This pattern matches the `ObjectPool(T, N)` type from PHORENSIC_OS.md §6.1.

---

# 16. Recursion Patterns

## Recipe 16.1 — Bounded Recursion

**Problem:** Write a recursive function in a trusted/kernel path with court-proven termination bound.

**Solution:**

```phorensic
package "phorensic:bounded_recursion:v1.0";

struct BinaryTreeNode
    layout default
    value: u64,
    left: Option(u64),   // index into node array
    right: Option(u64),
}

fn tree_sum(nodes: Array(BinaryTreeNode, 1024), index: u64) -> u64
    effect [compute]
    bounded depth 128  // proven: max tree depth is 128 for 1024 balanced nodes
{
    if index >= 1024 {
        return 0;
    }

    let node: BinaryTreeNode = nodes[index];
    let left_sum: u64 = 0;
    let right_sum: u64 = 0;

    match node.left {
        Some(left_idx) => {
            left_sum = tree_sum(nodes, left_idx);
        }
        None => {}
    }

    match node.right {
        Some(right_idx) => {
            right_sum = tree_sum(nodes, right_idx);
        }
        None => {}
    }

    return node.value + left_sum + right_sum;
}
```

**Explanation:** The `bounded depth 128` annotation declares a maximum recursion depth of 128. This is required for recursion in kernel or trusted paths (diagnostic E0011 without it). The bound must be a compile-time constant. Runtime depth checking is performed if static proof is unavailable. For a balanced binary tree of 1024 nodes, the max depth is 10 — well within the bound. The bound is set conservatively at 128 to allow for worst-case tree shapes. Recursion depth violation at runtime is a fault with diagnostic E0502.

---

## Recipe 16.2 — Iterative Tree Traversal (Recursion Alternative)

**Problem:** Traverse a tree iteratively using explicit stack to avoid recursion entirely.

**Solution:**

```phorensic
package "phorensic:iterative_traversal:v1.0";

const MAX_STACK: u64 = 128;

fn traverse_iterative(nodes: Array(BinaryTreeNode, 1024), root: u64) -> u64
    effect [compute]
{
    let mut stack: Array(u64, MAX_STACK) = Array::zeroed();
    let mut sp: u64 = 0;
    let mut sum: u64 = 0;

    // Push root
    stack[sp] = root;
    sp = sp + 1;

    while sp > 0 proven {
        sp = sp - 1;
        let index: u64 = stack[sp];

        let node: BinaryTreeNode = nodes[index];
        sum = sum + node.value;

        // Push right first (LIFO -> left processed first)
        match node.right {
            Some(r) => {
                stack[sp] = r;
                sp = sp + 1;
            }
            None => {}
        }

        match node.left {
            Some(l) => {
                stack[sp] = l;
                sp = sp + 1;
            }
            None => {}
        }
    }

    return sum;
}
```

**Explanation:** Iterative traversal avoids recursion entirely, eliminating the need for `bounded depth` annotation. An explicit stack (fixed-size array of 128 entries) replaces the call stack. The `while sp > 0 proven` loop iterates until the stack is empty. This pattern is preferred in kernel paths where recursion is disallowed. The stack size is bounded by `MAX_STACK` (compile-time constant), ensuring bounded memory usage. Array accesses are statically in-bounds since `sp < MAX_STACK` is maintained by the algorithm logic.

---

# 17. Residual Patterns

## Recipe 17.1 — Emitting a Residual Record

**Problem:** Emit a deterministic residual record for an operation that affects system state.

**Solution:**

```phorensic
package "phorensic:residual_emit:v1.0";

fn write_data(dev: Handle(BlockDevice), block: u64, data: Slice(u8))
    -> Result(void, IoError)
    effect [io:write, residual]
    court [block: v3]
{
    let before: Hash = dev.hash_state();

    dev.write(block, data);

    let after: Hash = dev.hash_state();

    residual emit {
        op: "block.write",
        device: dev.id,
        generation: dev.generation(),
        block: block,
        data_hash: hash(data),
        state_before: before,
        state_after: after,
        court: "block:v3",
    };

    return Ok(());
}
```

**Explanation:** The `residual emit` keyword produces a deterministic record of the operation. The record includes: operation name (`op`), affected device, generation tag, block number, data hash, state before and after, and the court citation. Residual emission requires the `residual` effect in the function's effect clause. Residuals are part of the replay checkpoint — they enable deterministic replay verification. The `op` field is mandatory and should use `lower_snake_case` with namespacing. Residual records are content-addressed via cryptographic hash and stored in the Forensic Store.

---

## Recipe 17.2 — Residual with Named Operation

**Problem:** Emit a residual using the compact named-operation syntax.

**Solution:**

```phorensic
package "phorensic:residual_named:v1.0";

fn read_sector(dev: Handle(BlockDevice), sector: u64) -> Result(Array(u8, 512), IoError)
    effect [io:read, residual]
{
    let data: Array(u8, 512) = dev.read(sector);

    residual "io:block_read" {
        device: dev.id,
        sector: sector,
        size: 512,
    };

    return Ok(data);
}

fn cage_translation(call: ForeignCall) -> void
    effect [cage:translate, residual]
{
    residual "cage:translation" {
        cage_id: "posix:file_ops",
        call: call.name,
        foreign_params: call.params,
        dialect_assumptions_checked: [
            "open returns 0 on success",
            "PATH_MAX is 4096",
        ],
    };
}
```

**Explanation:** The named-operation syntax `residual "op:name" { ... }` is a compact form equivalent to `residual emit { op: "op:name", ... }`. Named residuals follow hierarchical naming conventions: `io:block_read`, `cage:translation`, `store.add`, `trust.promote`. The dialect assumptions checked during cage translation are included as metadata. This pattern is used extensively in dialect cages and IO operations to produce structured, queryable residual records.

---

# 18. Pattern Matching & Binding Patterns

## Recipe 18.1 — Exhaustive Enum Matching with Guards

**Problem:** Match all variants of an enum with guard conditions on payloads.

**Solution:**

```phorensic
package "phorensic:pattern_match:v1.0";

type Status = enum {
    Active(u64),
    Inactive,
    Error(u64, Str(64)),
    Unknown,
};

fn handle_status(st: Status) -> Str(128)
    effect [io:write]
{
    match st {
        Status::Active(since) if since > 1000 => {
            return "Long-running active process";
        }
        Status::Active(since) => {
            return "Recently activated";
        }
        Status::Inactive => {
            return "Process is inactive";
        }
        Status::Error(code, msg) if code >= 500 => {
            return "Critical: " ++ msg;
        }
        Status::Error(code, msg) => {
            return "Warning: " ++ msg;
        }
        Status::Unknown => {
            return "Unknown state";
        }
    }
    // Compiler verifies exhaustive coverage of all variants
}
```

**Explanation:** Pattern matching in Phorensic supports: literal patterns, identifier bindings, wildcard `_`, enum variant patterns with payloads, tuple patterns, and guard expressions (`if condition`). Guards refine matches — `Status::Active(since) if since > 1000` matches only active statuses with a timestamp > 1000. The compiler verifies exhaustive coverage: all variants must be matched, possibly with guards covering the remaining cases. Unreachable patterns are flagged. Match arms have `=>` syntax and may be comma or semicolon terminated.

---

## Recipe 18.2 — Wildcard Pattern for Unused Bindings

**Problem:** Use wildcard patterns for unused bindings and catch-all match arms.

**Solution:**

```phorensic
package "phorensic:wildcard_pattern:v1.0";

type OperationResult = enum {
    Success(u64),
    Partial(u64, u64),
    Failed(Str(128)),
    Cancelled,
};

fn get_status_code(result: OperationResult) -> u64
    effect []
{
    match result {
        // Bind the payload but indicate we don't use the value
        OperationResult::Success(_) => {
            return 0;
        }
        // Bind only what we need
        OperationResult::Partial(_processed, _total) => {
            return 1;
        }
        // Wildcard for everything else
        _ => {
            return 2;
        }
    }
}
```

**Explanation:** The wildcard `_` matches any value without binding it. It can be used in several contexts: (1) ignoring a payload field — `Success(_)`, (2) unused binding — `let _ = compute()`, (3) catch-all arms — `_ => { ... }`. Wildcard patterns do not introduce bindings (no shadowing concerns). The compiler does not warn about unused wildcards. Using `_` for unused bindings is the idiomatic Phorensic approach. Prefixed underscores in identifiers (`_unused_var`) also suppress unused-variable warnings.

---

## Recipe 18.3 — Tuple Destructuring in Let and Match

**Problem:** Destructure tuples in let bindings and match arms.

**Solution:**

```phorensic
package "phorensic:tuple_destructure:v1.0";

fn get_coordinates() -> (u64, u64, u64)
    effect []
{
    return (100, 200, 50);
}

fn process_coordinates() -> u64
    effect [compute]
{
    // Tuple destructuring in let
    let (x, y, z): (u64, u64, u64) = get_coordinates();

    // Pattern matching on tuple
    match (x, y, z) {
        (0, 0, 0) => {
            return 0;
        }
        (x_val, y_val, 0) => {
            return x_val + y_val;
        }
        (x_val, y_val, z_val) => {
            return x_val + y_val + z_val;
        }
    }
}

fn swap_pair(pair: (u64, u64)) -> (u64, u64)
    effect []
{
    let (a, b) = pair;
    return (b, a);
}
```

**Explanation:** Tuples are destructured positionally in `let` bindings: `let (x, y, z) = expr;`. The type annotation `(u64, u64, u64)` is optional when the type can be inferred. Match patterns can match on tuple values: `(0, 0, 0)` matches the origin. Underscore patterns can be used for positions we don't care about: `(a, _)`. Tuple types require at least two elements (use parentheses for grouping single values). Tuples are value types and are copied on assignment.

---

## Appendix A — Recipe Index by Category

| # | Recipe | Category | Lines |
|---|--------|----------|-------|
| 1.1 | Hello, World | Basic | 22 |
| 1.2 | Factorial (Iterative) | Basic | 24 |
| 1.3 | Fibonacci (Iterative, Bounded) | Basic | 30 |
| 1.4 | Prime Sieve | Basic | 40 |
| 2.1 | Acquiring a Capability | Capability | 22 |
| 2.2 | Moving a Capability | Capability | 28 |
| 2.3 | Restricting a Capability | Capability | 28 |
| 2.4 | Capability Revocation | Capability | 30 |
| 2.5 | Use-After-Move Detection | Capability | 26 |
| 3.1 | Pure Function | Effects | 24 |
| 3.2 | IO Function with Effect | Effects | 32 |
| 3.3 | Combined Effect Set | Effects | 24 |
| 3.4 | Effect Propagation | Effects | 30 |
| 3.5 | Effect Mismatch Detection | Effects | 22 |
| 4.1 | For Loop Over Fixed Array | Loops | 18 |
| 4.2 | While Loop with Proven | Loops | 22 |
| 4.3 | Nested Bounded Loops | Loops | 30 |
| 4.4 | Bounded While with Capacity Guard | Loops | 28 |
| 5.1 | Struct Composition | Types | 30 |
| 5.2 | Enum Dispatch with Match | Types | 32 |
| 5.3 | Type Aliases | Types | 24 |
| 5.4 | Generic-Like Patterns | Types | 30 |
| 6.1 | Result Type with Match | Errors | 34 |
| 6.2 | Error Propagation | Errors | 38 |
| 6.3 | Result with Void Success | Errors | 28 |
| 7.1 | Creating a Sealed Container | Store | 42 |
| 7.2 | Verifying a Sealed Package | Store | 38 |
| 7.3 | Store Operations | Store | 30 |
| 8.1 | Importing Foreign Code | Cage | 36 |
| 8.2 | Dialect Translation | Cage | 32 |
| 8.3 | Unknown Foreign Call | Cage | 26 |
| 8.4 | C Surface Scan | Cage | 28 |
| 9.1 | Requesting a Court Verdict | Court | 36 |
| 9.2 | Promotion Flow | Court | 34 |
| 9.3 | Oracle Comparison | Court | 48 |
| 9.4 | Replay Checkpoint | Court | 28 |
| 10.1 | Porting: Dialect Cage Stage | Porting | 22 |
| 10.2 | Clean-Room CRC32 | Porting | 44 |
| 10.3 | Porting: Build Replay | Porting | 32 |
| 10.4 | Court-Verified Slice | Porting | 32 |
| 10.5 | Full Porting Lifecycle | Porting | 44 |
| 11.1 | Trusted Block with Reason | Trust | 18 |
| 11.2 | Trusted Block with Court | Trust | 22 |
| 11.3 | Machine Block | Trust | 38 |
| 12.1 | Acquiring a Handle | Handle | 18 |
| 12.2 | Stale Handle Detection | Handle | 22 |
| 12.3 | Cross-Generation Transaction | Handle | 28 |
| 12.4 | Handle Cast from FD | Handle | 28 |
| 13.1 | Service Declaration | Service | 16 |
| 13.2 | Service Capability Grant | Service | 28 |
| 14.1 | IPC Send | IPC | 28 |
| 14.2 | Structured IPC | IPC | 40 |
| 15.1 | Ring Buffer | Memory | 34 |
| 15.2 | Object Pool | Memory | 38 |
| 16.1 | Bounded Recursion | Recursion | 34 |
| 16.2 | Iterative Tree Traversal | Recursion | 42 |
| 17.1 | Residual Emit | Residual | 30 |
| 17.2 | Residual Named Op | Residual | 28 |
| 18.1 | Exhaustive Enum Matching | Patterns | 32 |
| 18.2 | Wildcard Patterns | Patterns | 28 |
| 18.3 | Tuple Destructuring | Patterns | 30 |
| 19.1 | String Manipulation | String | 28 |
| 19.2 | Fixed Array Sorting | Array | 32 |
| 19.3 | Boot Sequence Step | Boot | 24 |
| 19.4 | Config Profile Switch | Config | 28 |
| 19.5 | Driver Trust Promotion | Driver | 30 |
| 19.6 | Binary Load Policy | Loader | 32 |
| 19.7 | Generation Activation | Gen | 24 |
| 19.8 | Rollback Operation | Gen | 28 |

**Total: 63 recipes**

---

## Recipe 19.1 — Fixed-Capacity String Manipulation

**Problem:** Manipulate fixed-capacity strings (Str(N)) without heap allocation, handling truncation safely.

**Solution:**

```phorensic
package "phorensic:string_ops:v1.0";

fn concat_strs(a: Str(64), b: Str(64)) -> Str(128)
    effect [compute]
{
    let mut result: Str(128) = Str::new();
    let mut i: u64 = 0;

    // Copy first string
    while i < a.len() && !result.is_full() proven {
        result.push(a[i]);
        i = i + 1;
    }

    i = 0;

    // Copy second string
    while i < b.len() && !result.is_full() proven {
        result.push(b[i]);
        i = i + 1;
    }

    return result;
}

fn format_error(code: u64, msg: Str(64)) -> Str(128)
    effect [compute]
{
    let prefix: Str(16) = "Error #";
    let code_str: Str(16) = int_to_str(code);
    let sep: Str(4) = ": ";

    let mut result: Str(128) = concat_strs(prefix, code_str);
    result = concat_strs(result, sep);
    result = concat_strs(result, msg);

    return result;
}
```

**Explanation:** `Str(N)` is a fixed-capacity UTF-8 string. The `push` method appends a character if the string is not full. The `is_full()` check prevents overflow (capacity would be a runtime error E0902). Concatenation creates a new `Str(128)` from two `Str(64)` inputs. All operations are bounded by compile-time capacities. No heap allocation, no reallocation, no implicit growth. The `Str` type tracks both capacity (compile-time) and length (runtime). String operations produce deterministic residuals for replay verification.

---

## Recipe 19.2 — Fixed Array Sorting (Bubble Sort)

**Problem:** Sort a fixed-capacity array using a bounded sorting algorithm.

**Solution:**

```phorensic
package "phorensic:array_sort:v1.0";

fn sort_array(arr: Array(u64, 64)) -> Array(u64, 64)
    effect [compute]
{
    let mut result: Array(u64, 64) = arr;
    let mut i: u64 = 0;

    while i < 64 proven {
        let mut j: u64 = i + 1;
        while j < 64 proven {
            if result[i] > result[j] {
                let temp: u64 = result[i];
                result[i] = result[j];
                result[j] = temp;
            }
            j = j + 1;
        }
        i = i + 1;
    }

    return result;
}
```

**Explanation:** Bubble sort on a fixed-size 64-element array. Both loops have `proven` annotations because the bounds are compile-time constants (64). The algorithm performs exactly 2016 comparisons (64×63/2) — deterministic and bounded. No heap allocation, no recursion, no dynamic dispatch. The `proven` annotation satisfies the bounded loop requirement. Sorting larger arrays requires increasing the compile-time constant or using a bounded sorting network. The algorithm's O(n²) complexity is acceptable for small fixed-size arrays in trusted paths.

---

## Recipe 19.3 — Boot Sequence Step: Trusted Nucleus Handoff

**Problem:** Implement a step in the boot sequence where control transfers from the Trusted Nucleus to the kernel.

**Solution:**

```phorensic
package "phorensic:boot_sequence:v1.0";

fn kernel_launch(nucleus_verification: NucleusVerificationReceipt) -> Result(void, BootError)
    effect [machine:ioport, compute, residual]
{
    // Verify nucleus before handoff
    let expected_hash: Hash = hash(nucleus_image);
    if expected_hash != nucleus_verification.verified_hash {
        return Err(BootError::NucleusHashMismatch);
    }

    residual emit {
        op: "boot.nucleus_verified",
        asm_lines: nucleus_verification.asm_lines,
        machine_blocks: nucleus_verification.machine_blocks,
        verified_hash: nucleus_verification.verified_hash,
    };

    // Allocate fixed memory pools
    let process_pool: ObjectPool(Process, 64) = ObjectPool::init();
    let thread_pool: ObjectPool(Thread, 256) = ObjectPool::init();
    let cap_pool: ObjectPool(CapSlot, 4096) = ObjectPool::init();

    // Initialize capability system
    kernel.init_capabilities();

    // Start scheduler
    scheduler.start();

    residual emit {
        op: "boot.kernel_ready",
        generation: 1,
        process_pool: 64,
        thread_pool: 256,
        cap_pool: 4096,
    };

    return Ok(());
}
```

**Explanation:** The boot sequence is deterministic and bounded. Every step produces a receipt. The nucleus verification hash is checked against the compiled-in expected hash. Memory pools are pre-allocated at fixed sizes (no runtime allocation). The capability system is initialized before any process runs. Each residual emission documents the boot progress for replay verification. If any step fails, the system reverts to the previous generation. This pattern follows the boot sequence described in PHORENSIC_OS.md §14.

---

## Recipe 19.4 — Config Profile Switch

**Problem:** Atomically switch between system profiles with capability and trust threshold verification.

**Solution:**

```phorensic
package "phorensic:profile_switch:v1.0";

fn switch_profile(profile: Profile) -> Result(void, ProfileError)
    effect [io:read, io:write, residual]
{
    // Save current profile for rollback
    let prev: Profile = current_profile();

    // Set new profile
    set_current_profile(profile);

    // Verify all packages meet the trust threshold
    let packages: Slice(StoreKey) = profile.packages();
    let mut i: u64 = 0;

    while i < packages.len() proven {
        let pkg_key: StoreKey = packages[i];
        let pkg: StoreObject = store_get(pkg_key);

        if pkg.trust_state.level < profile.residual_trust_threshold {
            // Rollback to previous profile
            set_current_profile(prev);

            return Err(ProfileError::InsufficientTrust(
                pkg_key,
                pkg.trust_state.level,
                profile.residual_trust_threshold,
            ));
        }

        i = i + 1;
    }

    residual emit {
        op: "profile.switch",
        from: prev.name,
        to: profile.name,
        packages_verified: packages.len(),
    };

    return Ok(());
}
```

**Explanation:** Profile switching is atomic: the new profile is activated only after all packages are verified. If any package's trust level is below the threshold, the system rolls back to the previous profile automatically. The residual emission records the switch. Profiles define capability grants, dialect allowances, effect permissions, and replay policies. This pattern is the implementation of the profile model from FORENSIC_STORE.md §7. Profile changes are reversible and auditable.

---

## Recipe 19.5 — Driver Trust Promotion

**Problem:** Promote a driver's trust level and grant additional capabilities based on court verdicts.

**Solution:**

```phorensic
package "phorensic:driver_promotion:v1.0";

fn promote_driver(driver: Handle(Driver)) -> Result(TrustLevel, PromotionError)
    effect [court:request, io:read, io:write, residual]
{
    let current: TrustLevel = driver.trust_level;

    // Determine next level
    let next: TrustLevel = match current {
        TrustLevel::Observed => TrustLevel::Replayed,
        TrustLevel::Replayed => TrustLevel::OracleCompared,
        TrustLevel::OracleCompared => TrustLevel::ResidualStable,
        TrustLevel::ResidualStable => TrustLevel::Sealed,
        TrustLevel::Sealed => TrustLevel::Promoted,
        _ => return Err(PromotionError::AlreadyAtMax(current)),
    };

    // Request court verdict
    let verdict: CourtVerdict = court_request("promotion:v3", driver.id, None)?;

    match verdict.result {
        "promote" => {
            // Update trust level
            driver.trust_level = next;

            // Expand capabilities based on new trust level
            let additional_caps = get_capabilities_for_level(next);
            driver.expand_capabilities(additional_caps);

            residual emit {
                op: "trust.promote",
                driver: driver.id,
                from: current,
                to: next,
                verdict: verdict.id,
            };

            Ok(next)
        }
        "deny" => Err(PromotionError::Denied(verdict.reason)),
        "require_more_evidence" => {
            Err(PromotionError::InsufficientEvidence(verdict.reason))
        }
        other => Err(PromotionError::UnexpectedVerdict(other)),
    }
}
```

**Explanation:** Drivers follow the trust ladder from `observed` to `promoted`. At each step, a court verdict is required. On promotion, the driver gains expanded capabilities (more memory regions, wider IO access, additional kernel services). The capability expansion is explicit and documented in residuals. Drivers start with minimal capabilities and earn more through verified correct behavior. This pattern matches the driver model described in PHORENSIC_OS.md §7. Drivers cannot self-promote — only the kernel's promotion service can elevate trust.

---

## Recipe 19.6 — Binary Load Policy with Trust Check

**Problem:** Load an executable binary only if its trust level meets the requester's threshold.

**Solution:**

```phorensic
package "phorensic:load_policy:v1.0";

fn load_executable(path: Str(256), requester: Handle(Process))
    -> Result(Handle(Process), LoadError)
    effect [io:read, compute, residual]
{
    // Load sealed container from store
    let container: SealedContainer = store.load(path);

    // Verify package integrity
    let trust: TrustState = verify_package(container);

    // Check trust level against requester's threshold
    if trust.level < requester.load_threshold {
        return Err(LoadError::InsufficientTrust(
            trust.level,
            requester.load_threshold,
        ));
    }

    // Check capability requirements
    let required_caps: Slice(CapType) = container.capability_requirements();
    if !requester.capabilities.satisfies(required_caps) {
        return Err(LoadError::MissingCapabilities(required_caps));
    }

    // Spawn process with restricted capabilities
    let process: Handle(Process) = kernel.spawn_with_caps(
        container,
        requester.capabilities.intersect(required_caps),
    );

    residual emit {
        op: "load.executable",
        path: hash(path),
        trust: trust.level,
        requester: requester.id,
        process: process.id,
    };

    return Ok(process);
}
```

**Explanation:** Binary loading requires three checks: (1) package integrity (verify_package), (2) trust level threshold, (3) capability satisfaction. The process is spawned with the intersection of the requester's capabilities and the executable's requirements. This implements the principle of least authority: a process cannot gain capabilities the requester doesn't have. The residual emission documents the load for audit. This pattern is the implementation of PHORENSIC_OS.md §10's binary load policy. Every load decision is recorded and explainable.

---

## Recipe 19.7 — Generation Activation

**Problem:** Activate a new boot generation atomically with full verification.

**Solution:**

```phorensic
package "phorensic:gen_activation:v1.0";

fn activate_generation(gen: Generation) -> Result(void, BootError)
    effect [io:read, io:write, compute, residual]
{
    // 1. Verify generation signature
    let seal_ok: bool = verify_generation_seal(gen);
    if !seal_ok {
        return Err(BootError::InvalidGenerationSeal);
    }

    // 2. Verify all container hashes
    let containers: Slice(StoreKey) = gen.containers();
    let mut i: u64 = 0;
    while i < containers.len() proven {
        let container: SealedContainer = store_get(containers[i]);
        let computed_hash: Hash = hash(container);
        if computed_hash != containers[i] {
            return Err(BootError::ContainerHashMismatch(containers[i]));
        }
        i = i + 1;
    }

    // 3. Verify trust roots
    for root in gen.trust_roots {
        let root_ok: bool = verify_trust_root(root);
        if !root_ok {
            return Err(BootError::InvalidTrustRoot(root));
        }
    }

    // 4. Atomic switch
    kernel.set_boot_generation(gen);

    residual emit {
        op: "generation.activate",
        gen_id: gen.id,
        packages: gen.package_graph.len(),
        trust_roots: gen.trust_roots.len(),
    };

    return Ok(());
}
```

**Explanation:** Generation activation is all-or-nothing: if any verification step fails, the previous generation remains bootable. Activation verifies: the generation seal, every container hash, and every trust root. The atomic switch ensures the system cannot be left in an inconsistent state. On activation failure, the bootloader reverts to the previous generation. This pattern matches the generation model from PHORENSIC_OS.md §11. Each generation is a complete, sealed system image with its own trust roots.

---

## Recipe 19.8 — Generation Rollback

**Problem:** Roll back to a previous generation after a failed activation or detected trust violation.

**Solution:**

```phorensic
package "phorensic:gen_rollback:v1.0";

fn rollback_generation(gen_id: u64) -> Result(void, RollbackError)
    effect [io:read, io:write, residual]
{
    // Check rollback authority
    let rollback_cap: Cap(Rollback) = kernel.acquire("rollback");
    // 'rollback_cap' is consumed — only one rollback per capability

    // Load previous generation
    let prev_id: u64 = gen_id - 1;
    let prev_gen: Generation = store.generation(prev_id);

    // Verify previous generation is still valid
    let seal_ok: bool = verify_generation_seal(prev_gen);
    if !seal_ok {
        return Err(RollbackError::InvalidPreviousGeneration(prev_id));
    }

    // Emit rollback residual
    residual emit {
        op: "generation.rollback",
        from: gen_id,
        to: prev_id,
        reason: "failed generation activation",
        timestamp: kernel.tick_count(),
    };

    // Activate previous generation
    let result = activate_generation(prev_gen);

    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(RollbackError::ActivationFailed(prev_id, e)),
    }
}
```

**Explanation:** Rollback requires `Cap(Rollback)` — a privileged capability that is consumed on use (single-use). The target generation must still be sealed and valid (generations are never deleted, only superseded). The residual documents the rollback reason and affected generations. If even the previous generation fails to activate, the system enters recovery mode. Rollback cannot be rolled back (no double rollback). This pattern implements PHORENSIC_OS.md §11.2. The rollback capability is typically held by the bootloader and the system administrator's trusted process.

---

*End of PHORENSIC_COOKBOOK.md — 63 recipes covering 19 categories.*
