# Autonomous Porting Architecture

**Status: normative architecture + Phase 0 implemented.** This document is the
contract for the residual-native, evidence-first, increasingly autonomous
behavioral reconstruction foundry. Sections marked *normative* describe the
target architecture; the **Status** section records what is actually
implemented, and no planned mechanism is described there as shipped.

The division of labour is fixed:

```text
exploration proposes
residuals direct
history remembers
courts falsify
qualification separates
evidence scopes
and Phorensicos authority seals
```

**Automation may propose everything. Automation may approve nothing.**

---

## 1. The four-repository epistemic stack

| Repository | Role |
|---|---|
| **phorensicos** | Authority boundary, dialect cages, `phorc` compilation, execution/dispatch/composition courts, sealed store, sealed service. The final authority that admits a native artifact. |
| **frf** | The epistemic outer court: authority → court → capture → residual → endoduction → disposition → receipt → claim. Durable, re-verifiable evidence for observations, comparisons, minimization, challenge/sensitivity, execution series and bounded claims. |
| **frf-fuzz** | The counterexample engine: deterministic exploration, compare feedback, structural residuals, boundary search, precedent, experiment scheduling, crash-class outcome determinism. |
| **gemel** | Longitudinal engineering memory: intents, attempts, trajectories, failed attempts/negative knowledge, evidence-bearing changes, revision-aware provenance, continuation. |

Identity namespaces are **never** collapsed: a phorensicos SHA-256 object hash,
an FRF `receipt-…`/`run-…` id, an frf-fuzz `ContentId` (BLAKE3) and a Gemel
`Gid` are four distinct identity systems. Cross-system records carry
*references*, never reinterpretation.

---

## 2. Non-negotiable invariants (normative)

1. **Runtime isolation.** FRF, FRF-Fuzz, Gemel and host orchestration machinery
   never enter `phost_kernel`, the `no_std + alloc` runtime closure of `phost`,
   or the frf-fuzz `target-runtime` plane. At most one new host-side
   orchestration crate (`phorport`) is added.
2. **Authority separation.** No `CandidateProducer`, FRF-Fuzz finding, Gemel
   precedent, LLM verdict or mutation score may reach `Sealed`. The only route
   is the complete Phorensicos qualification and promotion state machine.
3. **Search artifacts can never become runtime artifacts.** Instrumented or
   search-only candidates are distinct types that cannot be supplied to the
   sealing API — enforced structurally, not by comment.
4. **Evidence identities never collapse namespaces** (§1).
5. **Fail closed.** Unknown schema versions, malformed identities, missing
   oracles, broken seals, corrupted store entries, mismatched artifacts, stale
   premise identities, missing challenge requirements and qualification leakage
   are all terminal errors. A high-assurance campaign never degrades silently.
6. **The compiled `.phor` ELF artifact is authoritative** for native behavior;
   the Rust mirror is a replay aid. The final artifact qualified and dispatched
   is the ordinary uninstrumented `candidate.o`.
7. **No wall-clock value participates in a semantic identity or verdict hash.**
   Timestamps may exist as non-authoritative metadata only.
8. **Scope.** API-surface behavioral reconstruction and native porting — not
   arbitrary binary translation.

---

## 3. Phase plan (normative) and status

A phase is not complete because code exists. It is complete when acceptance
tests pass, new invariants are hostile-tested, evidence is generated and
reproduces, existing behavior is preserved or the migration is explicitly
versioned, and the documentation states exactly what was demonstrated.

| Phase | Content | Status |
|---|---|---|
| **0** | Compatibility reconciliation (frf-fuzz → frf 0.1.86 / gemel 0.11.1) and the executable baseline seal | **done** — see below |
| **1** | Canonical typed `PortSpec`; migrate all existing leaves. No new leaf ports. | **done** — typed spec + content identity + all six leaf specs + precondition validator + registry-driven generic machinery (`docs/PORT_SPEC.md`) |
| **2** | Typed `CompositionIR` + one generic interpreter; migrate every existing composition | not started |
| **3** | Court sensitivity/challenge + semantic mutation profiles | not started |
| **4** | FRF-Fuzz differential exploration + residual semantic bank + court-verified minimization | not started |
| **5** | Gemel longitudinal memory + failed-attempt/negative-knowledge integration | not started |
| **6** | `CandidateProducer` + isolated synthesis workspace + bounded CEGIS + disagreement-driven experiment selection | not started |
| **7** | Held-out qualification + multi-oracle policy + FRF outer-court binding + `AUTONOMOUS-SEAL/v1` | not started |
| **8** | Immutable store generations + demand-driven port queue + explicit service generation binding | not started |
| **9** | Blind-regeneration demonstrations + controlled ablation + empirical verification report | not started |
| **10** | Automatic bounded `CompositionIR` synthesis | not started |
| **11** | ABI v2 bounded memory effects (only after the above) | not started |

### Phase 0 — done

* **Compatibility:** `frf-fuzz` upgraded from `frf =0.1.72`/`gemel =0.11.0` to
  `frf =0.1.86`/`gemel =0.11.1`. Every consumed FRF/Gemel signature was
  compared symbol for symbol; no semantic mismatch was found. Committed in
  `frf-fuzz` as `610b688` with `docs/DEPENDENCY_RECONCILIATION.md`. All frf-fuzz
  gates pass: 350 coordinator tests, 124 target-runtime tests, I15 dependency
  closure, fmt, clippy `-D warnings`, `golden_demo.sh`, `phase8_ablation_demo.sh`.
* **Baseline seal:** `foundry/baseline/dependency_pins.json` pins the four
  repositories; `scripts/integration_baseline.sh` derives a machine-readable
  receipt (`foundry/baseline/integration_baseline_receipt.json`) from the
  executable courts. It records the four commits, the toolchain, the `phorc`
  identity, the phorc/phost test counts, the sealed leaf/composition counts, the
  store residual identity, the session identity and the evidence roots, using
  the repository's asserted/observed split. It is deterministic: two runs at the
  same toolchain are byte-identical, and environment-bound build bytes are
  excluded from the residual hash.

  Baseline at `11ee334`: **6 sealed leaves + 7 sealed compositions = 13 ports**;
  store residual `181ace83…`; session `13 calls / 68 dispatches / 6 objects`,
  hash `d219be2c…`; phorc 50 tests, phost 241 tests (4 ignored), 0 failures.

### Phase 1 — done

`phost/src/porting/portspec.rs` defines the typed `PortSpec`, the domain-separated
canonical encoding (`PortSpecId = SHA-256("PHOR/PORTSPEC/v1\0" || canonical_bytes)`),
all six leaf specs, and the precondition validator (`validate_case` →
`ValidatedCase`; only a validated case may reach the foreign oracle). The six
content identities are pinned by a golden test, and a test proves every existing
leaf corpus satisfies its own declared preconditions. `PortSpec::of_target`
bridges every bootstrap target. See `docs/PORT_SPEC.md`.

The generic machinery is now registry-driven, not branch-driven:
`target::cases_for` / `target::resolve_target`, `candidate::run_candidate` and the
execution court's `call_target` all look the target up in
`phost/src/porting/registry.rs` (spec + bootstrap target + CaseGenerator +
candidate adapter + ABI adapter). `dialect_cage.rs` remains the `OracleAdapter`
extension boundary. Adding a leaf target is a new `PortSpec` plus three narrow
extension points and one registry row — no engine change. A static audit
(`test_generic_machinery_has_no_target_branches`) reads every generic `porting/`
module and fails if a `.id ==` target branch reappears.

The refactor is behavior-preserving: all committed leaf, composition, store,
session and cross-implementation evidence is byte-identical (Gate A).

---

## 4. Acceptance gates (normative)

The integration is not complete until all of these hold:

```text
A  baseline preservation        every existing leaf/composition remains valid
B  generic target model         all leaves represented through PortSpec; no target-ID
                                branches in the generic engine
C  generic composition model    all compositions are CompositionIR; no bespoke runner
D  court sensitivity            known-wrong semantic families are actively rejected
E  counterexample quality       a defect the design corpus alone misses is found,
                                minimized and replayed on the uninstrumented object
F  negative knowledge           a failed attempt and its reason are retrievable and
                                not treated as new work
G  qualification isolation      the producer workspace provably contains no
                                qualification material
H  autonomous reconstruction    one meaningful candidate withheld and reconstructed
                                through the whole pipeline
I  artifact authority           the final qualified/dispatched thing is the ordinary
                                compiled .phor ELF
J  runtime purity               opening/calling the sealed service needs no FRF,
                                FRF-Fuzz, Gemel, agent, compiler, oracle or network
K  immutable publication        a successful campaign creates a new store generation
                                rather than mutating a bound one
L  clean-room reproduction      the host integration reproduces from a clean checkout
```

---

## 5. Reporting vocabulary (normative)

Allowed: *observed, consistent, divergent, qualified under `<policy>`,
court-sensitive to `<families>`, multi-implementation concordant over `<region>`,
no counterexample found within `<explicit budget>`, sealed under `<seal
profile>`, refused, inconclusive.*

Not allowed unless a genuinely exhaustive finite domain or formal proof licenses
it: *proven equivalent, universally correct, bug-free, fully portable,
guaranteed correct, specification proven.*

Agreement on a bounded corpus is **evidence, not proof**.
