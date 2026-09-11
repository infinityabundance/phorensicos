// porting/abi_v2.rs — bounded memory effects, court side (§31, Phase 11)
//
// The current leaf ABI carries only packed `u64` words: a candidate cannot take
// a pointer, write memory, or overlap regions. ABI v2 is the next frontier, and
// this module implements its **court side**: the observable model a memory-effect
// port is judged by.
//
// A case is a *guarded arena*:
//
//   [ guard ][ input regions ][ output regions ][ guard ]
//
// The court observes both **what was returned** and **what memory changed**:
//
//   * the before image and the after image of every region;
//   * the actual write set (which offsets changed, and how);
//   * whether the guard zones survived (a run past a region end is a defect).
//
// Overlap is first-class: `memmove`'s contract against `memcpy`'s is exactly its
// behavior when the source and destination ranges overlap, so the court provides
// overlapping cases and distinguishes forward from backward copy.
//
// IMPORTANT — scope honesty. `phorc` does not yet emit pointer/region arguments,
// so there is no native memory-effect candidate to seal. This module is the
// observational and comparison contract, exercised in tests against a Rust model
// (clearly marked as a simulation, never a sealed port). A compiled memory-effect
// port requires the compiler/lowering half, which is not implemented.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The ABI v2 schema this build defines.
pub const ABI_V2_SCHEMA: u32 = 1;

/// The guard byte pattern written around every arena.
pub const GUARD_BYTE: u8 = 0xA5;

/// How a region may be used by the candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    /// Read-only input.
    Input,
    /// Writable output.
    Output,
    /// Readable and writable.
    InOut,
}

/// A declared region within the arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionSpec {
    pub kind: RegionKind,
    /// The initial bytes (the before image).
    pub bytes: Vec<u8>,
}

/// A guarded arena: left guard, the declared regions back to back, right guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedArena {
    pub guard: usize,
    pub regions: Vec<RegionSpec>,
    /// The full byte image the candidate would see (guards included).
    pub image: Vec<u8>,
    /// The offset of each region within `image`.
    pub offsets: Vec<usize>,
}

impl GuardedArena {
    /// Materialize an arena with guards.
    pub fn materialize(regions: Vec<RegionSpec>, guard: usize) -> Self {
        let mut image = Vec::new();
        image.extend(core::iter::repeat_n(GUARD_BYTE, guard));
        let mut offsets = Vec::with_capacity(regions.len());
        for r in &regions {
            offsets.push(image.len());
            image.extend_from_slice(&r.bytes);
        }
        image.extend(core::iter::repeat_n(GUARD_BYTE, guard));
        GuardedArena {
            guard,
            regions,
            image,
            offsets,
        }
    }

    /// The before image.
    pub fn before(&self) -> Vec<u8> {
        self.image.clone()
    }

    /// The total length of the declared regions (excluding guards).
    pub fn region_span(&self) -> usize {
        self.regions.iter().map(|r| r.bytes.len()).sum()
    }

    /// Observe the effects of mutating `image` in place.
    pub fn observe(&self, before: &[u8], after: &[u8]) -> ArenaObservation {
        let left_guard_after: &[u8] = after.get(..self.guard).unwrap_or(&[]);
        let right_start = after.len().saturating_sub(self.guard);
        let right_guard_after: &[u8] = after.get(right_start..).unwrap_or(&[]);
        let guard_intact = left_guard_after.iter().all(|b| *b == GUARD_BYTE)
            && right_guard_after.iter().all(|b| *b == GUARD_BYTE);

        let mut writes: Vec<WriteRecord> = Vec::new();
        let n = before.len().min(after.len());
        for i in 0..n {
            if before[i] != after[i] {
                writes.push(WriteRecord {
                    offset: i,
                    before: before[i],
                    after: after[i],
                });
            }
        }

        // Per-region after images.
        let mut regions_after: Vec<Vec<u8>> = Vec::with_capacity(self.regions.len());
        for (i, r) in self.regions.iter().enumerate() {
            let start = self.offsets[i];
            let end = start + r.bytes.len();
            regions_after.push(after.get(start..end).unwrap_or(&[]).to_vec());
        }

        ArenaObservation {
            before: before.to_vec(),
            after: after.to_vec(),
            writes,
            guard_intact,
            regions_after,
        }
    }
}

/// One byte that changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteRecord {
    pub offset: usize,
    pub before: u8,
    pub after: u8,
}

/// What the court observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaObservation {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub writes: Vec<WriteRecord>,
    pub guard_intact: bool,
    pub regions_after: Vec<Vec<u8>>,
}

/// The expected effects of a case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectExpectation {
    /// The candidate's return value (e.g. the destination pointer, normalized to
    /// its arena offset).
    pub return_value: i64,
    /// The offsets (relative to the arena) a correct implementation may write.
    pub allowed_writes: Vec<(usize, usize)>,
    /// The expected after image of the declared regions, back to back.
    pub expected_regions: Vec<u8>,
}

/// How an observation diverged from the expectation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectResidualClass {
    /// The observation matched the expectation.
    Consistent,
    /// The return value differed.
    ReturnMismatch,
    /// The candidate wrote outside every allowed range.
    WriteOutsideRange,
    /// The candidate clobbered a guard zone.
    GuardViolated,
    /// The observed bytes differ from the expected after image.
    ByteMismatch,
    /// The write set itself differs (a write a correct implementation must make
    /// is missing, or an extra write occurred inside the allowed range).
    WriteSetMismatch,
}

/// A memory-effect residual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectResidual {
    pub class: EffectResidualClass,
    pub matches: bool,
    pub detail: String,
}

/// Compare an observation against the expectation.
pub fn compare_effects(
    arena: &GuardedArena,
    expectation: &EffectExpectation,
    observed_return: i64,
    observation: &ArenaObservation,
) -> EffectResidual {
    if observed_return != expectation.return_value {
        return EffectResidual {
            class: EffectResidualClass::ReturnMismatch,
            matches: false,
            detail: format!(
                "return {} != expected {}",
                observed_return, expectation.return_value
            ),
        };
    }
    if !observation.guard_intact {
        return EffectResidual {
            class: EffectResidualClass::GuardViolated,
            matches: false,
            detail: String::from("a guard zone was modified"),
        };
    }
    for w in &observation.writes {
        let inside = expectation
            .allowed_writes
            .iter()
            .any(|(a, b)| w.offset >= *a && w.offset < *b);
        if !inside {
            return EffectResidual {
                class: EffectResidualClass::WriteOutsideRange,
                matches: false,
                detail: format!("write at offset {} is outside the allowed ranges", w.offset),
            };
        }
    }
    let observed_regions: Vec<u8> = observation.regions_after.concat();
    if observed_regions != expectation.expected_regions {
        return EffectResidual {
            class: EffectResidualClass::ByteMismatch,
            matches: false,
            detail: format!(
                "after image differs ({} vs {} bytes)",
                observed_regions.len(),
                expectation.expected_regions.len()
            ),
        };
    }
    // The write set must be exactly the changed offsets (implied by the after
    // image match) — record it as consistent.
    let _ = arena;
    EffectResidual {
        class: EffectResidualClass::Consistent,
        matches: true,
        detail: String::from("observed effects match the expectation"),
    }
}

// ---------------------------------------------------------------------------
// Overlap: memcpy vs memmove
// ---------------------------------------------------------------------------

/// The overlap relationship of a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// `dst` and `src` do not overlap.
    Disjoint,
    /// `dst` starts after `src` (a forward byte loop still works).
    DstAfterSrc,
    /// `dst` starts before `src` (a forward byte loop clobbers the source).
    DstBeforeSrc,
}

/// Classify the overlap of `[src, src+n)` and `[dst, dst+n)`.
pub fn classify_overlap(src: usize, dst: usize, n: usize) -> Overlap {
    if n == 0 || dst >= src + n || src >= dst + n {
        Overlap::Disjoint
    } else if dst > src {
        Overlap::DstAfterSrc
    } else {
        Overlap::DstBeforeSrc
    }
}

/// A forward-copy model (`memcpy` semantics): byte by byte from low to high.
/// This is deliberately the *wrong* model under overlap, so the court can be
/// shown to distinguish it from `memmove`.
pub fn forward_copy(buf: &mut [u8], src: usize, dst: usize, n: usize) {
    for i in 0..n {
        if src + i < buf.len() && dst + i < buf.len() {
            buf[dst + i] = buf[src + i];
        }
    }
}

/// A backward-copy model (`memmove` semantics for `dst > src`).
pub fn backward_copy(buf: &mut [u8], src: usize, dst: usize, n: usize) {
    for i in (0..n).rev() {
        if src + i < buf.len() && dst + i < buf.len() {
            buf[dst + i] = buf[src + i];
        }
    }
}

/// The overlap-correct copy: forward when `dst < src`, backward when `dst > src`.
/// This is `memmove`.
pub fn memmove_copy(buf: &mut [u8], src: usize, dst: usize, n: usize) {
    match classify_overlap(src, dst, n) {
        Overlap::Disjoint | Overlap::DstAfterSrc => backward_copy(buf, src, dst, n),
        Overlap::DstBeforeSrc => forward_copy(buf, src, dst, n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn arena(len: usize) -> GuardedArena {
        GuardedArena::materialize(
            vec![
                RegionSpec {
                    kind: RegionKind::Input,
                    bytes: (0..len as u8).collect(),
                },
                RegionSpec {
                    kind: RegionKind::Output,
                    bytes: vec![0u8; len],
                },
            ],
            4,
        )
    }

    #[test]
    fn test_returns_the_destination_and_detects_a_change() {
        // Simulate a candidate that copies region 0 into region 1.
        let a = arena(8);
        let before = a.before();
        let mut after = before.clone();
        for i in 0..8 {
            after[a.offsets[1] + i] = after[a.offsets[0] + i];
        }
        let obs = a.observe(&before, &after);
        assert!(obs.guard_intact);
        assert_eq!(obs.writes.len(), 7);
        assert!(obs.writes.iter().all(|w| w.offset >= a.offsets[1]));
        let exp = EffectExpectation {
            return_value: a.offsets[1] as i64,
            allowed_writes: vec![(a.offsets[1], a.offsets[1] + 8)],
            expected_regions: {
                let mut v = (0..8u8).collect::<Vec<u8>>();
                v.extend(0..8u8);
                v
            },
        };
        let r = compare_effects(&a, &exp, a.offsets[1] as i64, &obs);
        assert!(r.matches, "{r:?}");
    }

    #[test]
    fn test_a_guard_clobber_is_detected() {
        let a = arena(4);
        let before = a.before();
        let mut after = before.clone();
        after[0] = 0x00; // write into the left guard
        let obs = a.observe(&before, &after);
        assert!(!obs.guard_intact);
        let exp = EffectExpectation {
            return_value: 0,
            allowed_writes: vec![(a.offsets[1], a.offsets[1] + 4)],
            expected_regions: vec![0, 1, 2, 3, 0, 0, 0, 0],
        };
        let r = compare_effects(&a, &exp, 0, &obs);
        assert_eq!(r.class, EffectResidualClass::GuardViolated);
    }

    #[test]
    fn test_a_write_outside_the_allowed_range_is_detected() {
        let a = arena(4);
        let before = a.before();
        let mut after = before.clone();
        after[a.offsets[0]] = 0xff; // write into the read-only input
        let obs = a.observe(&before, &after);
        let exp = EffectExpectation {
            return_value: 0,
            allowed_writes: vec![(a.offsets[1], a.offsets[1] + 4)],
            expected_regions: vec![0, 1, 2, 3, 0, 0, 0, 0],
        };
        let r = compare_effects(&a, &exp, 0, &obs);
        assert_eq!(r.class, EffectResidualClass::WriteOutsideRange);
        assert!(!r.matches);
    }

    #[test]
    fn test_overlap_classification() {
        assert_eq!(classify_overlap(0, 8, 4), Overlap::Disjoint);
        assert_eq!(classify_overlap(0, 2, 4), Overlap::DstAfterSrc);
        assert_eq!(classify_overlap(2, 0, 4), Overlap::DstBeforeSrc);
        assert_eq!(classify_overlap(0, 0, 4), Overlap::DstBeforeSrc);
        assert_eq!(classify_overlap(0, 0, 0), Overlap::Disjoint);
    }

    #[test]
    fn test_the_court_distinguishes_memcpy_from_memmove_under_overlap() {
        // Moving [0,4) to [1,5): the classic clobber case. A forward copy
        // (memcpy) corrupts the tail; a backward copy (memmove) does not.
        let base: Vec<u8> = vec![1, 2, 3, 4, 0, 0];
        let mut fwd = base.clone();
        forward_copy(&mut fwd, 0, 1, 4);
        let mut mv = base.clone();
        memmove_copy(&mut mv, 0, 1, 4);
        assert_ne!(fwd, mv, "the court must see the difference");
        assert_eq!(mv, vec![1, 1, 2, 3, 4, 0]);
        assert_eq!(fwd, vec![1, 1, 1, 1, 1, 0]);
    }

    #[test]
    fn test_the_court_distinguishes_memcpy_from_memmove_for_the_other_direction() {
        // Moving [1,5) to [0,4): a *forward* copy is correct here; only the
        // backward-only model would fail. memmove must choose forward.
        let base: Vec<u8> = vec![0, 1, 2, 3, 4];
        let mut mv = base.clone();
        memmove_copy(&mut mv, 1, 0, 4);
        assert_eq!(mv, vec![1, 2, 3, 4, 4]);
    }
}
