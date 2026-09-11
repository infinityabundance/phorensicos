# Foundry

The host-side **autonomous JIT-porting foundry**: the residual-native,
evidence-first reconstruction machinery that sits *outside* the runtime.

This directory holds the foundry's machine-readable artifacts. It is not part of
the `phost` runtime dependency closure and never enters `phost_kernel`.

## Baseline

| File | Purpose |
|------|---------|
| `baseline/dependency_pins.json` | Authoritative pins for the four-repository epistemic stack (phorensicos, frf, frf-fuzz, gemel). |
| `baseline/integration_baseline_receipt.json` | The Phase 0 baseline receipt, **derived from executable courts** by `scripts/integration_baseline.sh`. |

Regenerate the receipt with:

```sh
./scripts/integration_baseline.sh
# optional: verify the external pins against working copies
FRF_ROOT=… FRF_FUZZ_ROOT=… GEMEL_ROOT=… ./scripts/integration_baseline.sh
```

The receipt uses the repository's asserted/observed split: environment-bound
build bytes (the `phorc` binary hash, the `rustc` commit hash) are recorded as
observed and excluded from the residual hash. No wall-clock value enters the
hash, so the receipt is byte-reproducible at a given toolchain.

## Releases

| File | Purpose |
|------|---------|
| `releases/<tag>.json` | A **release lock**: the empirical identities and tables of the autonomous-porting disclosure at a tagged revision, derived from committed evidence by `scripts/release_lock.sh`. |

A release tag binds the revision; the lock binds the identities. Regenerate and
verify with:

```sh
./scripts/release_lock.sh autonomous-porting-v1 foundry/releases/autonomous-porting-v1.json
./scripts/release_lock.sh --check autonomous-porting-v1 foundry/releases/autonomous-porting-v1.json
```

The check asserts the committed lock's asserted identities and residual hash still
recompute from committed evidence (the tag binding is reported, and is not needed
inside the container, whose build context has no `.git`). `scripts/ci_host.sh` runs
it, so the lock cannot drift from the courts silently.

## Architecture

The normative architecture, the non-negotiable invariants, the phase plan and
the acceptance gates live in
[`docs/AUTONOMOUS_PORTING_ARCHITECTURE.md`](../docs/AUTONOMOUS_PORTING_ARCHITECTURE.md).

The one-line summary:

```text
exploration proposes
residuals direct
history remembers
courts falsify
qualification separates
evidence scopes
and Phorensicos authority seals
```
