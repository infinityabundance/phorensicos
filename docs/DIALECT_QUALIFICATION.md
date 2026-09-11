# Dialect Qualification

How a target earns its `dialect:` namespace, what that namespace does **not** claim,
and how a mistaken qualification is corrected.

A port target's identity is qualified —
`dialect:symbol:locale:contract:version` — for one reason: so two behaviors that are
not the same thing can never be recorded as if they were. The target id is bound into
the sealed package, the oracle hash and every `chain_hash`, so a mistake here is not
cosmetic; it is a claim about *which contract* was observed.

## The rule

> `dialect` names the **specification the behavioral contract is drawn from** — not
> the library that happened to implement it, and not the language it is written in.

The five leaf targets `toupper`, `memcmp`, `memchr`, `strlen`, `strrchr` use
`dialect: libc` because their contracts are ISO C surfaces, and their observable
behavior (the observable is normalized: a pointer return becomes an index, a raw
comparison becomes a sign) is what ISO C specifies.

`strspn` is also an **ISO C** surface: it is specified by ISO C (C90 4.11.5.4; C99 and
later 7.21.5.4), and POSIX states that its `strspn` specification is aligned with and
defers to ISO C. Its corrected identity is therefore `libc:strspn:c-locale:u64:v1`, and
it is a `libc` target like the other five.

## The `strspn` correction: a mistaken qualification, preserved

This repository **historically** recorded `strspn` as:

```text
posix:strspn:c-locale:u64:v1
```

on the claim that **"ISO C does not specify `strspn`."** That claim was wrong. So was
the companion claim that `strcspn`, `strpbrk` and `strtok` are POSIX-only; all three are
ISO C surfaces. (Genuine POSIX-only string functions include `strdup` — until C23 —
`strcasecmp`, `strncasecmp`, `strsep`, `strtok_r` and `strndup`.)

The mistaken claim was stated in this document, in `README.md`, in
`VERIFICATION_REPORT.md`, in `docs/REPLAY_COURTS.md`, and in the target's doc comment.
It is preserved here **as part of the record**, not deleted: the point of a
provenance-qualified identity is that the claim is explicit and therefore can be found
wrong and corrected. The correction is an evidence-preserving migration, not a rename:

| | Historical (preserved) | Successor (new) |
|---|---|---|
| Target id | `posix:strspn:c-locale:u64:v1` | `libc:strspn:c-locale:u64:v1` |
| `dialect` | `posix` | `libc` |
| Contract source | `ContractSource::Posix` | `ContractSource::IsoC` |
| Seal profile | `LegacyV1` | `AutonomousV1` |
| `PortSpecId` | `bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23` | `a4ee309a8959b40a1f8d32bf3944156a70fbc15b0a4a8e85a10de5344bd1e697` |

The historical target, its `PortSpecId`, its seal and its evidence are **unchanged**.
Bare-symbol resolution (`strspn`) still returns the historical target so the committed
store, the JIT-porting court verifier and the committed historical evidence are
unaffected; the successor is addressed by its full id. The successor is requalified and
resealed on its own identity. See **`docs/CONTRACT_PROVENANCE_MIGRATION.md`** for the
record, `phorport supersessions` for the machine-readable correction, and
`./verify_supersession.sh` for the verification.

The observed behavior was never in question: the cage observes the host C library and
the seal binds the observed, normalized behavior. Only the provenance metadata was
wrong.

## What the qualification asserts

For the corrected `libc:strspn:c-locale:u64:v1` (and identically for the preserved
historical record, apart from the dialect):

| Field | Value | Why |
|-------|-------|-----|
| `dialect` | `libc` | The contract is specified by ISO C (7.21.5.4 and predecessors). |
| `symbol` | `strspn` | The specified name. |
| `version` | `host-observed-v1` | The provenance of the observation: the behavior was observed on the host, not derived from a specimen implementation. |
| `locale_contract` | `C` | The behavior observed under the C locale. Recording the locale keeps a locale-aware variant from being conflated with this one. |
| `output_schema` | `u64` span length | The observable is the length of the initial segment of `s` consisting only of bytes in `accept` — a prefix length decided by **set membership**. |

The corpus is derived from the ISO C contract, not from the host implementation's
source: the complete `(span, n)` grid, the empty set, the empty string, a stop byte
before the terminator, every set size 1..=8, a disjoint set, the unsigned edge bytes,
and two exhaustive 0..=255 sweeps (the accepted byte value and the stopping byte
value). The cage observes the foreign function as a black box through the same narrow
FFI shim used for the other ISO C targets; it does not read or copy it.

## What it does *not* claim

**The implementation observed is the host C library.** `dialect: libc` records the
specification the contract is drawn from; it does not claim that a distinct ISO C
implementation was observed, and it does not claim implementation-independence.

That limit is not left to prose — the seal binds the observed behavior by hash:

- `oracle_traces.json` is the observation; `behavior_signature.json` binds it with a
  combined oracle hash;
- the sealed package binds that oracle hash together with the locale contract and the
  dialect, so a different implementation whose behavior differs cannot pass
  silently — the replay and dispatch courts would fail against the sealed traces.

So the honest reading is: *the ISO C contract for `strspn` in the C locale, as observed
on this host*. The **implementation axis** is a separate court, and it now exists: the
cross-implementation court (`docs/REPLAY_COURTS.md`, `verify_cross_implementation.sh`)
observes the *same sealed corpus* through a **second, independent implementation** —
musl, compiled statically by `musl-gcc` so an out-of-process observer cannot be the
host's library in disguise — and requires agreement on every case. It records that the
two implementations produced the same **sealed trace set**, not merely the same
answers, and binds the leaf's committed sealed oracle hash.

That is a stronger claim than "as this host implements it", and it is still bounded:
agreement over a finite corpus is **evidence, not proof** of equivalence. It shows the
sealed corpus does not distinguish glibc from musl; it does not show the contract holds
for every implementation or every input. The qualification (`dialect`) and the
implementation axis are orthogonal: the first names which specification the contract is
drawn from, the second names the implementations it has been checked against.

## What would not qualify

- **Renaming an ISO C target to `posix:`.** No information is gained; the id would
  assert a provenance that is not true. This is exactly the mistake corrected above.
- **Claiming a dialect without a citation.** The step that makes any dialect legitimate
  is naming the specification and showing the symbol is absent from, or deferred to by,
  the other dialects already in use.
- **Hiding a locale or ABI dependency in the symbol name.** If behavior depends on a
  locale, that belongs in `locale_contract`, not in a suffix.
- **Editing a wrong claim in place.** A correction is a successor identity; rewriting
  the historical record would make old seals appear to have always carried the new
  claim.

## Checklist for adding a dialect or target

1. Cite the specification the contract comes from, and show the symbol is absent from
   the dialects already in use (or, as with POSIX and `strspn`, that the other
   specification defers to the cited one — in which case the cited one is the dialect).
2. State the observable precisely — including any normalization (a pointer becomes an
   index; a comparison becomes a sign; a span is a length).
3. State the locale and ABI contract, and put any dependence there rather than in the
   symbol.
4. Fit the leaf ABI: pure, relocation-free, ≤8-byte packed words, no heap, no
   syscalls — the constraint the execution court enforces.
5. Derive the corpus from the contract, and make it falsify the plausible wrong
   implementations (here: a single-byte compare instead of a set test).
6. Keep the verifier honest: the sealed package's `dialect` must equal the namespace
   of the target id, and the corpus checks must pin the axes the contract names.
7. If the contract is claimed to be implementation-independent, run the
   cross-implementation court (`./verify_cross_implementation.sh`) and state the limit:
   agreement over a bounded corpus is evidence, not proof.
8. If an existing qualification is found wrong, correct it by **supersession**
   (`docs/CONTRACT_PROVENANCE_MIGRATION.md`), preserving the historical record.
