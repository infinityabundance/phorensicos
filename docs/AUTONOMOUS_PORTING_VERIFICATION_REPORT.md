# Autonomous porting verification report

This report records **empirical results** and distinguishes them from normative
architecture (`docs/AUTONOMOUS_PORTING_ARCHITECTURE.md`) and current
implementation status. It is updated as phases complete; a result is only listed
when it was actually produced and reproduces.

## Phase 7 — held-out qualification, the implementation axis, the FRF outer court, `AUTONOMOUS-SEAL/v1`

**Demonstration.** `phorport autonomy strspn` was run from the repository root
with a two-revision candidate catalogue:

```text
revision 0  examples/jit_port_strspn_always_zero.phor   (returns 0 for every input)
revision 1  examples/jit_port_strspn.phor               (the clean-room candidate)
```

The policy is `HostObserved`; the budgets are the CLI defaults. The run:

1. the design court rejected revision 0 and minimized the counterexample;
2. revision 1 was frozen;
3. a 498-case held-out universe (role-lattice, `exhaustive_domain: false`) was
   built after the freeze and revision 1 passed **498/498**;
4. the court-sensitivity challenge detected every non-equivalent declared
   family for `strspn`;
5. the multi-oracle court observed the sealed corpus through the host
   implementation: `concordant`, 578 cases, 1 witness (the `HostObserved`
   policy);
6. the FRF outer court produced two receipts, retained verbatim: the divergence
   of revision 0 (`verified`) and the parity of the frozen candidate
   (`failed`, preserved as evidence of non-reproduction);
7. the ordinary uninstrumented object passed the execution court;
8. the native dispatch court served every case from a temporary sealed entry
   with zero fallbacks and zero broken seals;
9. the profile was satisfied: **13/13 obligations**.

Committed identities (all reproduce byte-for-byte; no wall clock participates):

```text
promotion receipt  b82e62678b905bf10ab737bfc248a912ed1d6d1fad9ff17fee2edc13ae5833a0
evidence closure   cf8b202a568d63dae771800f1a9ce953b46dca9f05817cccb43109d2729bad84
qualification      3bdde55877e1f0ee825c851c313e1babc42b43ccc201804c64168b4c568a974e (498 cases)
isolation          997 markers checked, 0 leaks, universe built after freeze
```

`verify_autonomous_seal.sh` checks the profile, the closure binding, the
qualification receipt, the isolation report, the FRF receipts, and then re-runs
the pipeline and requires the promotion and qualification receipts to be
**byte-identical**.

**What this demonstrates (bounded).** Under the declared bounded target contract,
candidate language, oracle set, experiment budgets and qualification policy, the
integrated foundry reconstructed a native Phor candidate and admitted it through
the complete declared evidence pipeline, with the qualified and dispatched
artifact being the ordinary compiled `.phor` ELF.

**What it does not demonstrate.**

* One semantic shape (`strspn`). Broader automation claims need more shapes
  (Phase 9).
* The model is a scripted catalogue, not a synthesizer; the CEGIS loop and its
  isolation boundary are exercised, the producer is deterministic.
* The committed campaign's policy is `HostObserved` (one witness). The
  `MultiImplementation` path is implemented and tested (concordant or
  `InsufficientWitnesses`), but the committed seal does not depend on it.
* The FRF authority is a generated helper around the host dialect cage; FRF
  admits and executes its bytes, and the implementation behind it is provenance,
  not a hashed identity. This is stated, not hidden.
* Immutable store generations and demand-driven publication are Phase 8; the
  Phase 7 seal records the parent/current relation against the sealed baseline.

## Test suites at this phase

```text
phost     317 passed, 0 failed, 4 ignored
phorc      50 passed, 0 failed
phorport   37 passed, 0 failed
```

New hostile tests include: precondition enforcement in the qualification
generator; universe determinism and independence (novelty over the design
corpus); receipt redaction; the isolation audit detecting an injected marker; the
contained worker agreeing with the in-process court, rejecting a corrupted
object, and recovering from a killed worker; the FRF court verifying a real
divergence and preserving a parity receipt; every autonomous obligation failing
closed when absent or inconsistent; the pipeline refusing to seal a
never-frozen candidate; store-generation tamper/rollback/artifact-substitution
refusal; a session bound to a generation refusing an unbound artifact; a runtime
miss emitting a demand without blocking; and the ablation being deterministic.

## Phase 9 — blind regeneration and controlled ablation

### Blind regeneration (Gate H, two shapes)

`phorport/src/synth.rs` is a **bounded enumerative Phor synthesizer** behind the
`CandidateProducer` trait. It sees only the public `PortSpec` — the ABI symbol and
the declared observable — and enumerates the branchless lane-scan family the
lowerer requires, varying the genuinely algorithmic axes (lane order, lane
masking). It never reads the withheld candidate source, the held-out corpus or
the challenge expectations.

For `strlen` the family declares four variants, offered wrong-first (a low→high
last-match scan), so the CEGIS loop must falsify it and revise. The test
`test_blind_regeneration_of_strlen_seals` runs the whole Phase 7 pipeline with the
synthesizer and seals under `AutonomousV1`, with at least one rejected revision —
so the reconstruction went through qualification, challenge, multi-oracle, FRF,
uninstructed execution, dispatch and the 13 obligations from the spec alone.
`memchr` is supported by the same family.

**Bounded claim.** Under the declared target contract, candidate language, oracle
set, experiment budgets and qualification policy, the integrated foundry
autonomously reconstructed a native Phor candidate for `strlen` (and supports
`memchr`) through the complete declared evidence pipeline without being given the
original implementation. The family is deliberately small: no arbitrary API has
been synthesized, and the harder semantic shapes (NUL termination in the
composed chains, set membership as a *set*) are not synthesized by this family.

### Controlled ablation

`phorport/src/ablation.rs` runs five arms (A design-only, B + random, C +
role-lattice guided, D C with precedent suppression, E D with disagreement
selection) against the declared `strspn` defect families with a shared seed
(`0x41424c4154494f4e`) and a fixed budget.

Result at budget 256:

```text
A-design       4/4 families, median 9 executions, IQR [1, 9], 4 residual classes
B-random       4/4 families, median 9 executions, IQR [1, 9], 4 residual classes
C-guided       4/4 families, median 9 executions, IQR [1, 9], 4 residual classes
D-precedent    4/4 families, median 9 executions, IQR [1, 9], 4 residual classes
E-disagreement 4/4 families, median 9 executions, IQR [1, 9], 4 residual classes
```

**This is a negative result, reported as such.** Every declared `strspn` defect
family is already distinguished by the design corpus, so the exploration arms add
nothing measurable on them. That is evidence that for these families the *challenge
profile*, not more exploration, is the binding constraint — consistent with the
Phase 3 finding that the court detects every declared family. Exploration matters
for defects the design corpus *misses*; that case is Phase 4's Gate E (the
`strspn` XOR-fold defect: 578 design cases, 0 divergences, then an FRF-Fuzz
campaign) and is measured there, not here. No p-value is converted into a
correctness claim.
