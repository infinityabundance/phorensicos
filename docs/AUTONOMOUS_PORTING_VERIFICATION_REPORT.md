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
phost     306 passed, 0 failed, 4 ignored
phorc      50 passed, 0 failed
phorport   27 passed, 0 failed
```

New hostile tests include: precondition enforcement in the qualification
generator; universe determinism and independence (novelty over the design
corpus); receipt redaction; the isolation audit detecting an injected marker; the
contained worker agreeing with the in-process court, rejecting a corrupted
object, and recovering from a killed worker; the FRF court verifying a real
divergence and preserving a parity receipt; every autonomous obligation failing
closed when absent or inconsistent; and the pipeline refusing to seal a
never-frozen candidate.
