# Phorensic Compiler Specification

## Overview

The Phorensic compiler is not merely a code translator. It is a deterministic, replay-aware, evidence-producing semantic pipeline that generates sealed executable containers with full provenance, receipts, and residual graphs.

## Compilation Pipeline

The compilation pipeline is segmented and resumable:

```text
Source
→ parse
→ expand
→ lower
→ optimize
→ codegen
→ object-write
→ link
→ seal
→ sealed container
```

Each stage produces:
- stable inputs
- deterministic outputs
- receipts
- hashes
- provenance records
- replay checkpoints
- residual deltas

## Stage Detail

### Parse

Input: source text
Output: AST with source location map
Receipt: source hash, parse tree hash, diagnostic log
Replay checkpoint: full AST state

### Expand

Input: AST
Output: expanded AST (macros resolved, templates instantiated, includes inlined)
Receipt: expansion receipts for each macro/template/include
Provenance: which expansion rule was selected, what context was active
Replay checkpoint: expanded AST before lowering

Expansion metadata per expansion event:

```text
input fragment
macro/template/include context
rule selected
dialect/profile flags
symbol table before and after
emitted bytes
oracle bytes (if available)
diff class
provenance
casefile ID
replay seed
deterministic hash
```

### Lower

Input: expanded AST
Output: IR (intermediate representation)
Receipt: IR hash, lowering rule selection provenance
Replay checkpoint: IR state

### Optimize

Input: IR
Output: optimized IR
Receipt: each optimization pass records its input hash, rule application, output hash, and residual delta
Replay checkpoint: IR after each optimization pass

Optimization receipts include:

```text
pass name
pass version
input IR hash
applied rule(s)
output IR hash
residual delta (what changed, why it is sound)
oracle comparison (if available)
```

### Codegen

Input: optimized IR
Output: machine code (object file)
Receipt: instruction selection records, register allocation decisions, relocation entries
Replay checkpoint: emitted machine code

### Object Write

Input: machine code + metadata
Output: object file in Phorensic Object Format (.phor-obj)
Receipt: object file hash, section table, symbol table
Replay checkpoint: object file

### Link

Input: one or more .phor-obj files
Output: linked executable image
Receipt: link graph, relocation resolution records, symbol resolution records
Replay checkpoint: linked image

### Seal

Input: linked image + all preceding receipts + source snapshot
Output: sealed .phor-spec container
Receipt: full evidence chain signature

Seal operation produces:

```text
final binary hash
source snapshot hash
source ↔ binary byte-comparison receipt
chain of all intermediate hashes
signature over the full chain
```

## Replay-Aware Compilation

The compiler supports pause, resume, re-run-dirty, and bisect modes.

### Pause/Resume

Compilation can be paused at any stage checkpoint and resumed from that exact point, provided the inputs match.

### Re-Run Only Dirty Segments

When inputs change, the compiler identifies which segments are affected and re-runs only those. A segment is dirty if:

- its source input changed
- its expansion input changed
- an optimization rule changed
- the target architecture changed
- the dialect profile changed
- the oracle configuration changed

### Bisect Miscompiles

Given a known-bad output, the compiler can bisect across stage checkpoints to identify which stage introduced the divergence. Each checkpoint is hashed and replayable, allowing precise fault isolation.

## Incremental and Replay-Aware Compilation

Incremental compilation asks: "what changed?"

Replay-aware compilation asks: "which deterministic semantic segment remains valid, and which residual boundary must be recomputed?"

This distinction is fundamental. Replay-aware compilation operates on residual boundaries — regions of the semantic graph that are sealed by deterministic receipts — rather than file modification timestamps.

## Pre-Compile Viability Inference

Before the compiler runs its full pipeline, a pre-compile inference engine analyzes the compilation context to predict viability:

Inputs for inference:

- recipe context
- dialect flags
- macro traces
- include topology
- dependency shape
- replay residuals
- prior Atlas patterns
- build graph structure
- generated-file freshness
- configure assumptions

Inferred outputs:

- likely missing context
- bad expansion path
- incompatible dialect/profile
- dependency shape mismatch
- stale generated files
- impossible configure assumptions
- high-risk semantic regions
- code that compiles but likely behaves wrong

This inference is advisory. The compiler proper makes the final determination.

## Deterministic Compilation

The compiler is deterministic across identical inputs:

```text
same source
+ same dialect profile
+ same compiler version
+ same target architecture
+ same effect profile
+ same oracle identities
+ same court versions
+ same build environment capsule
= identical sealed container (binary + evidence + trust state)
```

## ASM / Compiler Semantic Atlas

The system accumulates an Atlas corpus of:

- instruction semantics
- ABI semantics
- compiler optimization semantics
- linker semantics
- relocation behavior
- object format behavior
- calling conventions
- optimizer behavior
- target backend behavior
- diagnostics
- dialect signatures
- platform assumptions

The Atlas is used by the pre-compile inference engine, the porting engine, and the replay courts to reduce search space for future work.

## Deterministic CUDA Acceleration

GPU acceleration is permitted only for deterministic, bounded, canonicalized workloads:

Good acceleration targets:

- IR graph scans
- dataflow bitsets
- dominance/post-dominance analysis
- alias-candidate filtering
- pattern matching over instruction streams
- corpus/replay comparison
- residual hashing/fingerprinting
- pass-diff clustering
- diagnostics/casefile generation
- whole-corpus compile court sweeps

Rules:

- A CPU reference path must exist for every GPU-accelerated operation.
- GPU output must be court-checked against CPU/reference output.
- GPU scheduling must never decide semantics nondeterministically.
- Reductions must be canonicalized.
- GPU acceleration is a residual-processing accelerator, not semantic authority.

## Output Artifact Tiers

### Release Binary

```text
minimal hashes and signatures
executable image
source snapshot (required by package rule)
source hash
binary hash
source↔binary seal signature
```

### Audit Binary

```text
release binary contents
provenance records
compiler pass receipts
link receipts
```

### Forensic Binary

```text
audit binary contents
full IR snapshots
expansion receipts
replay checkpoints
residual graph
```

### Court Binary

```text
forensic binary contents
oracle traces
oracle comparison results
court verdicts
full replay evidence
```

## Compiler Diagnostics

All diagnostics have stable codes:

```text
PHOR-COMP-001: parse error
PHOR-COMP-002: expansion error
PHOR-COMP-003: type error
PHOR-COMP-004: capability error
PHOR-COMP-005: effect error
PHOR-COMP-006: bound error
PHOR-COMP-007: trusted block missing rationale
PHOR-COMP-008: stale generation usage
PHOR-COMP-009: use-after-move
PHOR-COMP-010: capability duplication
PHOR-COMP-011: undeclared effect in closure
PHOR-COMP-012: unbounded loop
PHOR-COMP-013: unbounded recursion
PHOR-COMP-014: link error
PHOR-COMP-015: seal error
PHOR-COMP-016: source↔binary hash mismatch
PHOR-COMP-017: missing source in package
PHOR-COMP-018: stale receipt
```

## Example: Compilation Receipt Chain

```text
Compilation of "hello.phor"
→ parse receipt: hash(p1) = 0xA1B2
→ expand receipt: hash(e1) = 0xC3D4
→ lower receipt: hash(l1) = 0xE5F6
→ optimize pass #1 (const_fold): hash(o1) = 0xG7H8
→ optimize pass #2 (dead_code): hash(o2) = 0xI9J0
→ codegen receipt: hash(c1) = 0xK1L2
→ object write receipt: hash(w1) = 0xM3N4
→ link receipt: hash(r1) = 0xO5P6
→ seal receipt: hash(s1) = 0xQ7R8
→ full chain hash: 0xS9T0
→ signature over chain
```

This chain allows anyone with the receipts to replay the compilation and verify the output.
