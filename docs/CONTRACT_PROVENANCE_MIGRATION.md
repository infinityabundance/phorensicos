# Contract Provenance Migration

How Phorensicos corrects a **mistaken provenance claim** in its own history without
rewriting that history.

This document is normative for corrections of a `PortSpec`'s contract provenance. It
records one concrete correction — `strspn` — end to end, and states exactly what is
preserved, what is new, and what is not claimed.

---

## 1. Why a provenance claim is evidence

A port target's identity is qualified — `dialect:symbol:locale:contract:version` — and
`dialect` names **the specification the behavioral contract is drawn from** (see
`docs/DIALECT_QUALIFICATION.md`). That claim is not cosmetic. It is bound into:

- the target id;
- the `PortSpecId` (a versioned, domain-separated hash of the typed specification);
- the autonomous seal, the evidence closure, and every downstream receipt.

So a wrong `dialect` is a **semantic-metadata error**, and under the residual-primacy
thesis it must be handled the same way as any other residual: observed, stated
precisely, minimized to a correction, remembered, and superseded by a **new identity**.
It must never be fixed by editing the record in place, because existing seals bind the
old `PortSpecId`; rewriting it would fabricate a history in which those seals had
always carried the corrected claim.

---

## 2. The defect

`strspn` was recorded as:

```text
posix:strspn:c-locale:u64:v1
dialect          = posix
contract source  = ContractSource::Posix
```

on the claim — stated in `docs/DIALECT_QUALIFICATION.md`, `README.md`,
`VERIFICATION_REPORT.md`, `docs/REPLAY_COURTS.md` and the target's doc comment — that
**"ISO C does not specify `strspn`."**

That claim is **false**.

`strspn` is specified by ISO C:

| Edition | Clause |
|---------|--------|
| ISO C (C90/C89) | 4.11.5.4 |
| ISO C99 and every later edition | 7.21.5.4 |

and POSIX states that its `strspn` specification is **aligned with and defers to** ISO
C. The same applies to the other functions the old text grouped with it: `strcspn`,
`strpbrk` and `strtok` are all ISO C surfaces. Genuine POSIX-only string functions
include `strdup` (until C23), `strcasecmp`, `strncasecmp`, `strsep`, `strtok_r` and
`strndup`.

The **observed behavior was never in question.** The cage observes the host C library,
and the seal binds the observed, normalized behavior by hash. The defect is confined to
the **provenance metadata**: the record said "POSIX, not ISO C" when the contract is an
ISO C contract. The correct dialect is therefore `libc`, not `posix`.

### What the defect did *not* do

- It did not invalidate the target's behavior, corpus, candidate or sealed artifact.
- It did not weaken the historical seal: that seal binds the observed behavior, which is
  correct, and it remains verifiable.
- It is a **residual on Phorensicos itself** — the architecture caught a
  semantic-metadata mistake in its own record — not an external integration failure.

---

## 3. The rule

> A contract-provenance correction is a **successor identity**, never a rename.
>
> The historical target, its `PortSpecId` and its evidence are preserved exactly. The
> successor is issueable only after it is requalified and resealed on its own
> `PortSpecId`, through the same complete qualification and promotion state machine as
> any other candidate.

`SUPERSESSIONS` in `phost/src/porting/supersession.rs` is the one declared table of such
corrections. Each record:

- names the historical and successor target ids;
- states the historical and corrected contract sources;
- states the normative reason and the authority the correction rests on;
- binds **both** `PortSpecId`s into a content-addressed record
  (`PHOR/CONTRACT-SUPERSESSION/v1\0` followed by a deterministic typed encoding; the
  identity is a pure function of content — **no wall clock participates**).

The operator surface is:

```text
phorport supersessions
```

which validates every declared record (`is_consistent()`), then prints the
`phorensic.phorport.supersessions.v1` JSON envelope. A declared-but-inconsistent
correction is a hard error, not a warning.

---

## 4. The correction

| | Historical (preserved) | Successor (new) |
|---|---|---|
| Target id | `posix:strspn:c-locale:u64:v1` | `libc:strspn:c-locale:u64:v1` |
| `dialect` | `posix` | `libc` |
| Contract source | `ContractSource::Posix` | `ContractSource::IsoC` |
| `PortSpecId` | `bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23` | `a4ee309a8959b40a1f8d32bf3944156a70fbc15b0a4a8e85a10de5344bd1e697` |
| Seal profile | `LegacyV1` (committed store) | `AutonomousV1` |
| Evidence | `phost/evidence/phorport/autonomy/strspn/` | `phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1/` |
| Evidence closure | `cf8b202a568d63dae771800f1a9ce953b46dca9f05817cccb43109d2729bad84` | `b7ec423a12e27a409fc47944ace471ada1fae9b44d0dd18d89aedef16fd2fc0b` |

Everything else about the surface is **identical**: symbol `strspn`, observable
`Length`, projection `Identity`, ABI symbol `phor_strspn_len`, preconditions, the
`strspn_corpus` design generator (578 cases) and the candidate source. The two specs
differ only in their provenance and therefore in their identity.

The historical `PortSpecId` is pinned by a golden test
(`portspec.rs::test_spec_ids_are_pinned_golden`) and must never drift.

### Bare-symbol resolution is preserved

Two registered specs now share the symbol `strspn`. `registry.rs` orders the
**historical row first** and the successor row last, and `extension_by_name` returns the
first match. Therefore:

- `resolve_target("strspn")` returns the **historical** target (`posix:strspn:…`);
- the successor is addressed only by its full id
  (`resolve_target("libc:strspn:c-locale:u64:v1")`).

This keeps the committed store, the JIT-porting court verifier and the committed
historical evidence addressing the same target they always did. Generic machinery
resolves leaves by **full id**, because an id — not a symbol — is the identity.

---

## 5. Requalification path

The successor does not inherit the historical seal. It re-earns authority:

1. **Design replay** over the same bounded, contract-derived corpus (578 cases) through
   the dialect cage;
2. **Bounded CEGIS** production of a candidate from the candidate language: an incorrect
   first revision is falsified and minimized before the surviving revision is frozen;
3. **Held-out qualification** over an independently constructed 498-case universe built
   *after* the candidate is frozen, with the isolation audit reporting zero leaks
   (997 markers checked);
4. **Court-sensitivity challenge** against the declared non-equivalent mutation families;
5. **FRF outer court** on the counterexample and on a parity fixture — FRF receipt ids
   retained verbatim, in the FRF namespace, never reinterpreted as Phorensicos hashes;
6. **Ordinary uninstrumented execution** of the emitted ELF object;
7. **Native dispatch** with zero broken seals and zero fallbacks;
8. **`AUTONOMOUS-SEAL/v1`** with all 13 obligations satisfied.

The FRF court names the **observed surface** (`phorport-strspn`), so the historical and
successor campaigns share a court name; their FRF stores are separate per target
directory, and their receipt and closure identities are distinct.

### Store generation

The successor's seal binds the §19 generation **relation** as obligation O13: a
`previous-store-generation` (the sealed baseline) and a `current` generation derived
from the successor's target, object and closure. The correction then **publishes the
successor as an immutable child generation** — additively, without mutating the
committed baseline:

```text
generation 0 (genesis)   f47d1bec90d8bba92f441138fc29db77094c9a35420057a5d60c3ee4163bdf73
                         13 entries, all LegacyV1, from phost/evidence/store/index.json
generation 1            8e6fafec7b2b66ced51d8699d86fc264983fe2cd501c11b9c316f4d125912ba5
                         parent = generation 0
                         14 entries = the 13 carried forward unchanged
                           + libc:strspn:c-locale:u64:v1 under AutonomousV1 (closure b7ec423a…)
```

Committed at
`phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1/store_generation.json`
and verified by `./verify_supersession.sh`. The committed v1 index is **not
mutated**: the long-lived session verdict (`d219be2c…`) is unchanged, existing
sessions stay bound to their generation, and a new session may open generation 1.
The published artifact is the successor's autonomous object
(`c40c4e3a…`), and the historical `posix:strspn` leaf is carried forward with its
committed `LegacyV1` artifact (`c93271d0…`) — so both the correction and the record
it corrects remain addressable in one lineage.

The successor is then **consumed**: `phost port generation-session` materializes
generation 1's exact runtime index and serves all 14 ports from 7 mapped objects,
distinguishing `posix:strspn` (served the historical object) from `libc:strspn`
(served the successor object). See `docs/STORE_GENERATIONS.md` §7 and
`./verify_generation_session.sh`.

---

## 6. Verification

```text
./verify_supersession.sh
```

asserts, in order:

1. the correction record is declared, consistent and content-addressed, with both
   golden `PortSpecId`s bound;
2. the historical target, its `PortSpecId` and its evidence are preserved (not renamed);
3. the historical autonomous seal still verifies (`./verify_autonomous_seal.sh`);
4. the successor has its own distinct `PortSpecId` and its own `AutonomousV1` seal;
5. the successor autonomous seal verifies
   (`./verify_autonomous_seal.sh --target strspn --target-id libc:strspn:c-locale:u64:v1 --evidence-slug libc-strspn-c-locale-u64-v1`);
6. the two evidence closures differ (no identity collapse);
7. the successor is published as an immutable child generation — the recorded
   identity recomputes, the parent is bound, and the historical baseline entry is
   carried forward unchanged.

`scripts/ci_host.sh` runs it after the Phase 7 verifier.

---

## 7. What this does not claim

- It does **not** claim the historical record was wrong about *behavior*. It was not.
- It does **not** rewrite or delete the historical record or its evidence. Both remain,
  carrying the mistaken claim, as part of the record.
- It does **not** claim the successor is "more correct" behaviorally. It is the same
  observed surface with a corrected provenance identity.
- It does **not** prove the ISO C contract holds for every input or every
  implementation. The successor's seal is bounded evidence over its declared corpus,
  exactly like every other `AutonomousV1` seal.
- It publishes the successor into an **additive** immutable generation; the committed
  v1 store index is not rewritten and existing sessions are unaffected.

---

## 8. Generalizing

Any future correction follows the same shape:

```text
discovered contract-provenance error
        ↓
explicit residual / migration record (SUPERSESSIONS)
        ↓
corrected successor identity
        ↓
replay the existing corpus
        ↓
requalify (held-out universe, challenge, FRF, execution, dispatch)
        ↓
new AUTONOMOUS-SEAL/v1
        ↓
new immutable store generation (additive; the prior generation is not mutated)
        ↓
generation session materializes the exact index and serves every bound port
```

The historical record is preserved, the successor is requalified, and the two are
distinct content-addressed identities. A declared correction that cannot satisfy
`is_consistent()` — both specs present, distinct identities, same surface, changed
provenance — fails closed.
