# Court Sensitivity (Challenge)

**Status: Phase 3 complete** (`phost/src/porting/challenge.rs`).

Phases 1 and 2 made the courts generic and proved they *accept* what is correct.
Phase 3 asks the question that makes the instrument falsifiable: can the court
actually **see** the semantic defect classes it claims to discriminate? A passing
candidate is weak evidence if the measuring instrument is blind.

## 1. The claim, bounded

The challenge court runs a bounded `MutationProfile` of **intentionally wrong**
implementations against the **same corpus and oracle the real court uses**, and
reports, per mutant:

| Field | Meaning |
|---|---|
| `family` | the semantic defect family (e.g. `PORT.FIRST_VS_LAST`) |
| `id` | a stable id within the family |
| `valid` | a well-formed observable could be produced |
| `detected` | at least one case diverged (the court rejects the mutant) |
| `detected_cases` / `total_cases` | how localized the detection is |
| `first_differing_case` | the witness case |
| `specific` | detection is localized, not a blanket mismatch |
| `equivalent` + `equivalence_note` | provably equivalent under the declared domain |

The statement this licenses is **bounded**:

> court-sensitive to mutation families [X, Y, …] over this declared corpus

— never "proves all bugs detectable", and never one magical "mutation score".

## 2. Equivalent and undetermined mutants

The specification's rule is explicit, and this implementation enforces it:

* an **equivalent** mutant (equal to the correct implementation on every valid
  input *under the declared preconditions*) is recorded as equivalent, with the
  reason, and is **never counted as killed or missed**. Example: `strlen`'s
  "ignore the bound" mutant is equivalent because the `strlen` precondition
  guarantees a NUL within `n`, so ignoring the bound cannot change a valid
  observation. Out-of-contract scanning is deliberately *not* tested here; it
  belongs to a separate, explicit court against declared fail-closed behavior —
  never to a foreign undefined-behavior oracle.
* an **invalid** (undetermined) mutant means the court demonstrated nothing about
  that family, so it **fails closed**: `court_sensitive` is false.

`court_sensitive` is true only when the profile is non-empty and every declared,
non-equivalent mutant is detected.

## 3. The profiles

Leaf profiles (`phost/src/porting/challenge.rs`), named by family:

| Target | Families demonstrated |
|---|---|
| `toupper` | identity, wrong polarity (lower/swap), locale widening |
| `memcmp` | signedness, inclusive/exclusive bound, reversed order, bound substitution |
| `memchr` | first-vs-last, bound escape, absent sentinel |
| `strlen` | first-vs-last, zero termination (ignore NUL, 0x80 terminator), off-by-one, bound escape (equivalent) |
| `strrchr` | first-vs-last, post-terminator visibility, NUL-needle handling |
| `strspn` | set-vs-sequence (prefix-only, wrong polarity), post-terminator visibility, lane order |

Composition profiles are *wrong `CompositionIR`s* — the same data model Phase 2
introduced — evaluated over the sealed store and compared against the correct
chain's committed oracle:

| Chain | Wrong shape |
|---|---|
| `toupper_memchr` | drop the fold (dependency omission) |
| `toupper_strlen_memchr` | search with the caller's `n` instead of the derived bound |
| `toupper_strlen_memchr_pair` | the second search substitutes `n` for the shared derived bound |
| `toupper_each` | fold only the first byte |
| `toupper_each_strlen_memchr` | search with `n` instead of the derived bound |
| `toupper_memchr_suffix` | search before the slice |
| `toupper_each_slice_search` | bare `memchr` on the unfolded slice |

A composition mutant that cannot even run (an ill-formed graph, or a broken seal)
is still **rejected** by the court, so it counts as detected.

## 4. Evidence and verification

Each target's challenge evidence is committed at
`phost/evidence/challenge/<name>/challenge_verdict.json`
(schema `phorensic.porting.challenge_verdict.v1`) and verified by
`verify_challenge_court.sh`, which regenerates it and checks the schema, the
counts, that no declared family is a blind spot, that equivalent mutants carry a
note, and that the residual hash is a canonical digest. It is deterministic: two
fresh runs are byte-identical.

```sh
cargo run -p phost -- port challenge strlen       # one target
./verify_challenge_court.sh --target strlen --check-committed
```

## 5. Relationship to FRF

This mirrors FRF's challenge discipline: a court must demonstrate that it can
observe the defect classes it claims. Where FRF provides a challenge court, a
Phorensicos seal **references** that evidence rather than reinterpreting it; the
Phorensicos challenge court is the local, corpus-bound instrument check that the
autonomous qualification profile (Phase 7) will require.
