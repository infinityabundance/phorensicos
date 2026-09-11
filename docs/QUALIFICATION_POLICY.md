# Qualification and the implementation axis (Phase 7)

**Status: Phase 7 core implemented.** This document is normative for held-out
qualification, the multi-oracle policy, and the isolation boundary that keeps the
candidate producer away from qualification material.

## 1. The four case universes

Every case carries its universe and provenance (`phorport/src/case.rs`):

| Universe | Role | Visible to the producer? |
|---|---|---|
| DESIGN | specification-derived; the registered corpus | yes |
| DISCOVERY | found after a candidate exists (mutation, compare, disagreement) | yes, once discovered |
| QUALIFICATION | held-out evaluation | **no** |
| CHALLENGE | deliberately wrong implementations (court sensitivity) | no |

## 2. Held-out by construction, not by naming

`phorport/src/qualification.rs` builds the qualification universe **after** a
candidate is frozen, from a generator that is structurally independent of the
design/discovery generators:

* the design corpora are exhaustive deterministic enumerators over concrete
  pattern families (`zero/ones/asc/desc/alt`, edge bytes);
* the qualification generator samples **semantic role lattices** — accept-member
  / non-member / terminator, match / non-match, equal / ordered-differing — and
  instantiates each cell into concrete bytes.

The two constructions share no axis decomposition. Independence is *constructional*;
the universe is never an input to a producer request.

When a `PortSpec` declares an exhaustive finite case space (`toupper`'s 256
bytes), held-out sampling cannot add coverage, so the universe enumerates the
complete domain and says so (`exhaustive_domain: true`). "Held out" is only
claimed where the space is not already finite.

A generated case that violates its own declared preconditions is a generator
defect: `build` fails closed rather than silently dropping it.

## 3. The receipt is redacted

`QualificationReceipt` carries counts and case **ids**, never input bytes or
oracle outputs. The raw observations live only in `QualificationLedger`, the
auditor's view, which is never materialized into a synthesis workspace. A
qualification failure therefore cannot leak the held-out answer to the next
revision, and `SealRefusal::describe` names the obligation without echoing
evidence.

## 4. Isolation is audited

`IsolationReport` records that the synthesis workspace was materialized from
permitted material only and audited against the universe's markers. The markers
are high-entropy, namespaced tokens (case ids, `phor.qualification-seed:…`,
`phor.qualification-input:<sha256>`), not raw short hex, so the audit cannot
false-positive on an unrelated hex string. Raw held-out bytes are prevented from
reaching a workspace by construction; the audit is the defense-in-depth detector
for the token forms. `isolated` is false iff the audit found a leak or the
universe was not built after the freeze; the seal obligation O7 then refuses.

## 5. The implementation axis

A single host C library is one *observed implementation*. `QualificationPolicy`
(`phost/src/porting/portspec.rs`) declares how many independent witnesses a
contract requires:

```text
HostObserved                        1 witness
ImplementationFamily { family }     1 witness, named
MultiImplementation { n }           >= max(n, 2)
MultiEnvironment { n }              >= max(n, 2)
PortableCandidate { n }             >= max(n, 2)
```

`phorport/src/multioracle.rs` observes the same sealed corpus through the host
implementation and, when the policy requires it, through a second implementation
(musl, out-of-process, reusing `phost::porting::cross_impl`). It reports a
`MultiOracleVerdict`:

* **concordant** — the required witnesses answered and agreed on every case;
* **divergent** — at least one case differed, reported as an
  `OracleDivergenceResidual` to classify, never majority-voted away;
* **insufficient-witnesses** — fewer witnesses answered than the policy requires;
  this is a fail-closed refusal, never a silent success with one.

The committed campaign uses `HostObserved` (one witness). Agreement over a
bounded region is *concordance observed over that region*, never a proof of the
specification.

## 6. What is not claimed

A passing held-out qualification is evidence that the candidate is consistent
with the oracle over the declared universe under the declared policy. It is not
equivalence, and it is not the specification.
