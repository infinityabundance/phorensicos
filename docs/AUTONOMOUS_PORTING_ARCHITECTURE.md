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
| **2** | Typed `CompositionIR` + one generic interpreter; migrate every existing composition | **done** — the seven chains are IR data, evaluated by one interpreter on both the foreign and sealed side; the `composition_runner` branch is removed and the runtime resolves compositions as data; artifact identities versioned v2; equivalence to the legacy court and the committed v1 evidence is proven (`docs/COMPOSITION_IR.md`) |
| **3** | Court sensitivity/challenge + semantic mutation profiles | **done** — bounded mutation profiles for all six leaves and all seven compositions, an equivalent-mutant discipline, committed evidence and a verifier (`docs/CHALLENGE.md`); every declared family is detected, so the courts are demonstrably not blind |
| **4** | FRF-Fuzz differential exploration + residual semantic bank + court-verified minimization | **core done** — the `phorport` host crate: comparison harness, `PORT.*` residual bank, court-verified minimization, durable counterexamples, and an FRF-Fuzz campaign bridge; Gate E demonstrated on a defect the design corpus misses (`docs/AUTONOMOUS_PORTING.md`) |
| **5** | Gemel longitudinal memory + failed-attempt/negative-knowledge integration | **core done** — durable counterexample and rejected-candidate records in Gemel (its own object model, optional), retrieved so failed work is not re-explored; Gate F demonstrated (`docs/GEMEL_MEMORY.md`); intents/trajectories/revision replay remain |
| **6** | `CandidateProducer` + isolated synthesis workspace + bounded CEGIS + disagreement-driven experiment selection | **core done** — the producer trait (scripted + external-command adapters), constraint enforcement, the isolated synthesis workspace with a leak audit, the monotonic campaign state machine, and the bounded CEGIS loop (design + discovery courts, revision on failure, freeze on survival); disagreement selection remains (`docs/CEGIS.md`) |
| **7** | Held-out qualification + multi-oracle policy + FRF outer-court binding + `AUTONOMOUS-SEAL/v1` + candidate containment | **core done** — an independent-construction held-out universe with a redacted receipt and a leakage audit; the multi-oracle policy (host-observed / multi-implementation, never a majority vote); the FRF outer court as a differential court (receipts retained verbatim); the `AUTONOMOUS-SEAL/v1` 13-obligation profile; and an out-of-process contained candidate worker (rlimits, `no_new_privs`, timeout, crash recovery). The pipeline seals a reconstructed `strspn` end-to-end and reproduces byte-for-byte (`verify_autonomous_seal.sh`); see `docs/QUALIFICATION_POLICY.md`, `docs/EVIDENCE_MODEL.md`, `docs/THREAT_MODEL_AUTONOMOUS_PORTING.md` |
| **8** | Immutable store generations + demand-driven port queue + explicit service generation binding | **core done** — `StoreGeneration`/`GenerationLedger` with content-identity lineage and fail-closed verification; a session binds a generation and refuses an unbound artifact; a runtime miss emits a bounded, non-blocking `PortDemand`; deterministic integer demand ranking with a recorded breakdown (`docs/STORE_GENERATIONS.md`) |
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

### Phase 2 — done

`phost/src/porting/composition_ir.rs` defines the typed `CompositionIR` (SSA-like
values; `Input`/`ConstantScalar`/`Call`/`MapBytes`/`PackUsize`/`Observe`/`Slice`/
`Compare`/`Binary`/`Select` nodes), validation (bounded, typed, and acyclic because
every operand must reference an earlier node), the domain-separated content identity
`CompositionIrId = SHA-256("PHOR/COMPOSITION-IR/v1\0" || canonical_bytes)`, and one
lazy generic `eval` over a `PortBackend` so the oracle side and the sealed side run
the *same* graph.

`phost/src/porting/composition_registry.rs` is the one composition table: each of
the seven chains is IR data plus a deterministic corpus. Adding a composition is new
data, not a new runner. `phost/src/porting/composition_engine.rs` provides the
`ForeignBackend` (the cage) and `SealedBackend` (`NativeDispatcher`), the generic
court, and `eval_composition_port` — the runtime path. The `composition_runner(id)`
branch is gone; `NativeDispatcher` resolves a composition port to its IR and
recurses through the same dispatcher, so every leaf-seal check and dispatch count is
preserved (the committed session verdict `d219be2c…` is byte-identical).

A composed artifact binds four v2 identities — `composition_ir_hash`,
`dependency_binding_hash`, `behavior_hash`, `composition_artifact_hash` — and the
historical v1 `chain_hash` remains the seal the store publishes. The v2 evidence
lives in `phost/evidence/composition_ir/<name>/` and is verified by
`verify_composition_ir_court.sh` (four identities, per-stage accounting, and each
stage's seal cross-checked against the committed store — a seal of a seal).
Equivalence is proven two ways: the in-test cross-check
(`test_ir_court_agrees_with_the_legacy_court_for_every_composition`) and the
byte-identical v1 evidence under the legacy verifier. See `docs/COMPOSITION_IR.md`.

### Phase 3 — done

`phost/src/porting/challenge.rs` makes the measuring instrument falsifiable. A
bounded `MutationProfile` of intentionally wrong implementations is run against
the **same corpus and oracle the real court uses**: leaf mutants are wrong
observable implementations, composition mutants are wrong `CompositionIR`s
evaluated over the sealed store against the correct chain's committed oracle.
Every declared family is detected, so the courts are demonstrably not blind to
them.

The equivalent/undetermined discipline is enforced: an equivalent mutant (equal to
the correct implementation on every valid input under the declared preconditions) is
recorded as equivalent with its reason and is never counted as killed or missed;
an undetermined mutant fails closed. `court-sensitive` is a bounded statement over
the declared profile and corpus.

Evidence is committed at `phost/evidence/challenge/<name>/challenge_verdict.json`
and verified by `verify_challenge_court.sh` (schema, counts, no blind spots,
equivalence notes, canonical residual). This is the local instrument check the
autonomous qualification profile (Phase 7) requires. See `docs/CHALLENGE.md`.

### Phase 7 — core done

The autonomous profile is now real and reproduces. In `phost`:

* `porting/ident.rs` — the identity namespaces (`PortSpecId`, `FrfReceiptId`,
  `GemelGid`, …) as distinct opaque newtypes whose canonical tokens never
  collide, plus the content-addressed **evidence closure** (sorted, length
  framed, domain-separated).
* `porting/oracle_witness.rs` — `OracleWitness`, the `MultiOracleVerdict`, and
  divergence classification: a disagreement is an `OracleDivergenceResidual` to
  classify, never a majority vote.
* `porting/autonomous_seal.rs` — the `AUTONOMOUS-SEAL/v1` profile: obligations
  O1–O13, each carrying the references it is built from, verified by a pure
  fail-closed function; `LegacyV1` history stays immutable.
* `portspec.rs` — `QualificationPolicy` extended to `ImplementationFamily`,
  `MultiImplementation`, `MultiEnvironment`, `PortableCandidate` (existing specs
  keep their identities: `HostObserved` is unchanged).

In `phorport`:

* `qualification.rs` — the held-out universe, built **after** the candidate is
  frozen from a role-lattice construction independent in kind from the design
  corpora; the receipt is redacted, and the raw observations live only in the
  auditor's ledger.
* `multioracle.rs` — the implementation axis, reusing the cross-implementation
  court; an unavailable second implementation is an explicit
  `InsufficientWitnesses`, never a silent success with one.
* `frf.rs` — the FRF outer court as a differential court; run/receipt/claim ids
  are retained verbatim.
* `worker.rs` — the untrusted candidate worker (out-of-process, `no_new_privs`,
  `RLIMIT_AS/CPU/CORE/NOFILE`, timeout, crash recovery).
* `pipeline.rs` — the end-to-end flow (CEGIS → qualification → challenge →
  multi-oracle → FRF → execution → dispatch → seal), with the isolation audit and
  fail-closed refusal.

Demonstrated: `phorport autonomy strspn` reconstructs a candidate and seals it
under `AutonomousV1` with all 13 obligations; `verify_autonomous_seal.sh` checks
the committed evidence and proves the promotion and qualification receipts
reproduce byte-for-byte. Honest limits are stated in the phase docs (one shape,
host-observed policy in the committed campaign, FRF authority is a helper around
the host dialect cage whose bytes FRF admits).

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
