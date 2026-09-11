# Autonomous Porting Foundry — Phase 4 (FRF-Fuzz counterexample engine)

**Status: Phase 4 core complete** (`phorport/`).

Phases 1–3 made the courts generic and made them falsifiable. Phase 4 turns the
gap between designed cases into a **counterexample engine**: FRF-Fuzz attacks the
space the design corpus does not enumerate, and every discovery is minimized by
the same court that found it.

## 1. The host crate

`phorport` is the one host-side orchestration crate. It is deliberately outside
the `phost` / `phost_kernel` dependency closure, so nothing it uses can enter the
sealed runtime. It does **not** depend on FRF-Fuzz at build time: the engine is
driven through its CLI, located by path (the same pattern the Phase 0 baseline
uses for the sibling repositories).

| Module | Role |
|---|---|
| `case` | the fuzz-input ↔ port-case encoding (and its inverse, so the design corpus seeds exploration); total and non-panicking |
| `harness` | the differential comparison: precondition gate → foreign oracle → compiled `.phor` object → normalized comparison → structural residual |
| `residual` | the Phorensicos structural residual bank (`PORT.*`) |
| `minimize` | court-verified minimization |
| `counterexample` | durable, content-addressed counterexample records |
| `explore` | the FRF-Fuzz bridge: generate target, seed, run campaign, extract + minimize |

## 2. The comparison harness

A probe is the *only* comparison implementation; the fuzz target wrapper and the
out-of-process minimizer both call it, so one semantic comparison decides every
stage:

1. decode the fuzz input into a port case (total on a prefix);
2. **validate it against the port's `PortSpec` preconditions** — only a validated
   case reaches the foreign oracle, so undefined/out-of-contract inputs are never
   used as an oracle, and a missing terminator is repaired rather than rejected so
   exploration stays dense;
3. observe the foreign implementation through the dialect cage;
4. execute the **uninstrumented compiled `.phor` object** (hash-verified before
   mapping);
5. compare the normalized observables and classify the divergence.

The fuzz target deliberately **panics on divergence**: a mismatch is a worker-level
failure, so FRF-Fuzz's crash ledger reproduces the exact input and its scheduler
promotes it to a finding.

## 3. Court-verified minimization

The FRF principle: **the minimizer proposes, the court decides.** A reduction is
accepted only when the comparison harness re-runs the whole pipeline on the
reduced input and still reports a valid, diverging case. Every attempt — accepted
or refused, with its residual lineage — is retained, so a reduction that destroyed
the defect is evidence, not a silent success.

## 4. Gate E, demonstrated

A deliberately defective `strspn` candidate is used: its set-membership fold is an
**XOR** instead of an OR, which is *equivalent* for a set with distinct bytes but
wrong when the accept set contains a duplicate byte. The design corpus uses only
distinct accept bytes, so it cannot see the defect.

```sh
$ phorport corpus strspn --candidate-src examples/jit_port_strspn_xor_lane.phor
design cases run:  578
diverged:          0
design corpus MISSES the defect

$ phorport fuzz strspn --candidate-src examples/jit_port_strspn_xor_lane.phor \
      --frf-fuzz-bin <frf-fuzz> --frf-fuzz-root <frf-fuzz repo> --max-time 90
findings:    246
counterexample:      c6faff444bca08337fd6f8a9bbdc0fe2543d46b0b3a6c781eb1664fa2c28d7b9
original residual:   PORT.LENGTH
minimal residual:    PORT.LENGTH
original (hex):      0804017f7ffe017f80fe5a7f8000   (15 bytes)
minimal (hex):       08047ffe017f7f                   (7 bytes)
reductions:          7 accepted / 46 refused
```

The minimized counterexample replays on the ordinary uninstrumented object
(`phorport probe … --data 08047ffe017f7f` → oracle `1`, candidate `0`,
`PORT.LENGTH`). The durable record is committed at
`phost/evidence/phorport/strspn/<content_id>.json` and replayed by
`verify_phorport.sh`, which needs no nightly instrumented toolchain.

## 5. A defect found in phorc (the executor fix)

Running the harness under the instrumented worker exposed a **genuine ABI
violation in the emitted object**: phorc's codegen uses `RBX`/`R12`–`R15` as
scratch registers without saving them, so a JIT function can corrupt its caller.
It went unnoticed because at low optimisation levels the Rust caller happened not
to keep a live value in those registers across the call; at `-O3` (the
instrumented worker) `RBX` is live and the corruption segfaults the caller.

The fix is in the **executor**, not the compiler, precisely so no emitted object
changes: `phost::porting::exec::invoke` is a register-preserving trampoline that
saves and restores the callee-saved registers around every JIT call. Every
committed seal, verdict and session hash is therefore byte-identical, while the
sealed runtime can no longer be corrupted by a candidate no matter what the
compiler emits. (Fixing the compiler would have regenerated every object and
invalidated every seal; that is a deliberate, versioned act, not a silent one.)

## 6. What Phase 4 does not yet do

* The comparison is per-leaf. A composition's *compiled* candidate is not yet a
  target (compositions are sealed data, not objects, in Phase 2).
* The fuzz target currently runs the candidate **in-process**. Phase 7 moves
  candidate execution into a contained, untrusted worker; the harness is written
  so its comparison is unchanged by that move.
* Experiment selection is FRF-Fuzz's; the candidate-disagreement selector is
  Phase 6.
