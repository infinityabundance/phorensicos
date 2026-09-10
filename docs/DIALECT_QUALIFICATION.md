# Dialect Qualification

How a target earns its `dialect:` namespace, and what that namespace does **not**
claim.

A port target's identity is qualified —
`dialect:symbol:locale:contract:version` — for one reason: so two behaviors that are
not the same thing can never be recorded as if they were. The target id is bound into
the sealed package, the oracle hash and every `chain_hash`, so a mistake here is not
cosmetic; it is a claim about *which contract* was observed.

## The rule

> `dialect` names the **specification the behavioral contract is drawn from** — not
> the library that happened to implement it, and not the language it is written in.

The existing five leaf targets use `dialect: libc` because their contracts are ISO C
surfaces: `toupper`, `memcmp`, `memchr`, `strlen`, `strrchr`. Their observable
behavior (the observable is normalized: a pointer return becomes an index, a raw
comparison becomes a sign) is what ISO C specifies.

`strspn` is different, and the difference is checkable: **ISO C does not specify
`strspn`.** It is a POSIX function. So are `strcspn`, `strpbrk`, `strtok`,
`strcasecmp`, `strdup` and others. Recording `strspn` as `libc:strspn:…` would
conflate two standards in exactly the way this scheme exists to prevent, and would
leave the id unable to say "this contract is not ISO C".

Hence `posix:strspn:c-locale:u64:v1`.

## What the qualification asserts

For `posix:strspn`:

| Field | Value | Why |
|-------|-------|-----|
| `dialect` | `posix` | The contract is specified by POSIX, not ISO C. |
| `symbol` | `strspn` | The specified name. |
| `version` | `host-observed-v1` | The provenance of the observation: the behavior was observed on the host, not derived from a specimen implementation. |
| `locale_contract` | `C` | The behavior observed under the C locale. Some POSIX functions are locale-sensitive; recording the locale keeps a locale-aware variant from being conflated with this one. |
| `output_schema` | `u64` span length | The observable is the length of the initial segment of `s` consisting only of bytes in `accept` — a prefix length decided by **set membership**. |

The corpus is derived from the POSIX contract, not from the host implementation's
source: the complete `(span, n)` grid, the empty set, the empty string, a stop byte
before the terminator, every set size 1..=8, a disjoint set, the unsigned edge bytes,
and two exhaustive 0..=255 sweeps (the accepted byte value and the stopping byte
value). The cage observes the foreign function as a black box through the same narrow
FFI shim used for the ISO C targets; it does not read or copy it.

## What it does *not* claim

**The implementation observed is the host C library.** `posix:` records the
specification the contract is drawn from; it does not claim that a distinct POSIX
implementation was observed, and it does not claim implementation-independence.

That limit is not left to prose — the seal binds the observed behavior by hash:

- `oracle_traces.json` is the observation; `behavior_signature.json` binds it with a
  combined oracle hash;
- the sealed package binds that oracle hash together with the locale contract and the
  dialect, so a different implementation whose behavior differs cannot pass
  silently — the replay and dispatch courts would fail against the sealed traces.

So the honest reading is: *the POSIX-specified contract for `strspn` in the C locale,
as observed on this host*. Turning that into *implementation-independent* behavior
would require observing a **second** implementation (for example a musl-container
run of the same corpus) and promoting only if both agree — a separate axis, and
future work.

## What would not qualify

- **Renaming an ISO C target to `posix:`.** No information is gained; the id would
  assert a provenance that is not true.
- **Claiming a dialect without a citation.** The step that makes `posix:` legitimate
  is the demonstrable absence of the symbol from the other dialects already in use.
- **Hiding a locale or ABI dependency in the symbol name.** If behavior depends on a
  locale, that belongs in `locale_contract`, not in a suffix.

## Checklist for adding a dialect or target

1. Cite the specification the contract comes from, and show the symbol is absent from
   the dialects already in use.
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
