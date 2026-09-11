# ABI v2 — bounded memory effects, court side (Phase 11)

**Status: Phase 11 court side implemented; the compiler half is not.** This
document is explicit about the boundary.

## 1. Why ABI v1 cannot see a memory effect

The leaf ABI carries only packed `u64` words: a candidate takes scalars and
returns a scalar. It cannot take a pointer, write memory, or overlap regions.
`memset`, `memcpy` and `memmove` are therefore *not* expressible, and adding more
packed-integer functions would not reach them.

## 2. The court-side model

`phost/src/porting/abi_v2.rs` implements the observable model a memory-effect port
is judged by. A case is a **guarded arena**:

```text
[ guard ][ input regions ][ output regions ][ guard ]
```

Regions are declared with a `RegionKind` (input / output / in-out) and a before
image. The court observes **both** what was returned and what memory changed:

* the before and after images of every region;
* the **actual write set** (every offset that changed, and how);
* whether the guard zones survived (a run past a region end is a defect);
* the candidate's return value, normalized to an arena offset.

`compare_effects` classifies a divergence into a `EffectResidualClass`:
`ReturnMismatch`, `WriteOutsideRange`, `GuardViolated`, `ByteMismatch` or
`WriteSetMismatch`, or `Consistent`. Every variant fails closed for an
autonomous seal.

## 3. Overlap is first-class

`memmove`'s contract against `memcpy`'s is exactly its behavior when the source
and destination ranges overlap, and the direction of the copy decides the
outcome:

```text
classify_overlap(src, dst, n) = Disjoint | DstAfterSrc | DstBeforeSrc
```

The court ships overlapping cases and the two reference models
(`forward_copy`, `backward_copy`) so it can be shown to separate a naive forward
`memcpy` from an overlap-correct `memmove`, in both directions. Adversarial
overlap and boundary cases are present as tests.

## 4. The honest boundary

`phorc` does **not** yet emit pointer/region arguments or memory operations, so
there is no native memory-effect candidate to execute and no memory-effect port
is sealed. The court-side comparison is exercised against a Rust model, which is
a **simulation**, clearly marked as such — never a sealed port. Completing ABI v2
requires the compiler/lowering half (pointer and region arguments, `load`/`store`,
bounds), which is a separate, versioned change: it would regenerate emitted
objects and therefore must not be done silently.

No claim of ABI v2 completeness is made. `memset`, `memcpy` and `memmove` remain
future work precisely because the spec warns against beginning this phase early.
