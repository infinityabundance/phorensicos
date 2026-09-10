# Reviewer Protocol

## Purpose

This project is not designed for skim review. The Phorensic OS stack is a deep, coherent system where every component depends on the evidence architecture. Reviewers must understand this depth before forming conclusions.

## Core Principles for Reviewers

### Claim Boundaries Are Warranty Boundaries

Each document makes explicit claims and explicit non-claims. A claim is a warranty about what the system does. A non-claim is a deliberate choice not to address something.

**Do not mistake a non-claim for absence of code.** Non-claims are explicit design decisions. If a document says "not POSIX-first," it means POSIX is handled as a dialect cage, not that POSIX support is missing due to oversight.

### Conservative Non-Claims

The project is deliberately conservative in its claims. When the documentation says:

- "not claiming universal support before residual evidence exists"
- "not a promise that every foreign program can be natively sealed immediately"

These are architectural constraints, not admissions of incompleteness. They reflect the evidence-first philosophy: nothing is trusted without replayable evidence.

### Verbose Commentary Is Intentional

Code and documentation in this project include verbose commentary. This is not poor style. It is a deliberate choice to:

- Preserve hard-won context about why a decision was made
- Document the residual evidence that supports each claim
- Make the reasoning auditable by courts and future engineers
- Reduce the risk of misinformed "simplifications" that break the evidence chain

**Clean minimal code can be wrong in deep infrastructure.** In a forensic residual-primacy system, the evidence chain is as important as the code itself. A "clean" implementation that drops residual recording is incorrect by design.

## Reviewer Workflow

Follow this workflow for every claim, module, and fixture:

```text
1. Read the relevant documentation.
2. Locate the source module (in .phor files).
3. Locate the fixture/court/golden file (in .phor-evidence or .phor-test).
4. Run the check (compile, test, court session, oracle comparison).
5. Compare emitted artifacts (hashes, residuals, receipts, verdicts).
6. Only then assess the claim.
```

### Step 1: Read the Relevant Documentation

Start with the document that makes the claim. Understand:

- What is being claimed?
- What is the evidence for the claim?
- What is explicitly not claimed?
- What are the assumptions and dependencies?

### Step 2: Locate the Source Module

Find the Phorensic source code that implements the claim. Source modules are in `.phor` files under `src/`.

Check:

- Does the module implement what the document claims?
- Does the module have the expected effects and capability declarations?
- Does the module produce residuals as specified?
- Are there trusted blocks with rationales?

### Step 3: Locate the Fixture/Court/Golden File

Every claim should have corresponding test fixtures, court sessions, or golden files:

```text
src/                        # Phorensic source
tests/                      # Test fixtures
  courts/                   # Court session definitions
    {claim_name}.court.phor
  oracles/                  # Oracle definitions
    {claim_name}.oracle.phor
  golden/                   # Expected outputs
    {claim_name}.golden.bin
  residuals/                # Expected residual patterns
    {claim_name}.residuals.json
```

### Step 4: Run the Check

Run the appropriate verification:

```text
# Compile and check
phc build --verify src/module.phor

# Run court session
phc court run tests/courts/claim.court.phor

# Compare with oracle
phc court compare --oracle tests/oracles/ref.oracle.phor

# Check residuals
phc court residuals --expect tests/golden/claim.residuals.json
```

### Step 5: Compare Emitted Artifacts

Compare the actual output with expected:

```text
# Compare binary hash
phc hash compare output.bin tests/golden/claim.golden.bin

# Compare residual chain
phc court residuals --compare tests/golden/claim.residuals.json

# Verify source↔binary seal
phc package verify output.phor-spec
```

### Step 6: Assess the Claim

Only after completing steps 1–5 should you assess:

- Does the implementation match the documentation?
- Is the evidence chain complete and verifiable?
- Are the residuals correct?
- Are the non-claims respected?
- Is the trust state appropriate?

## Common Reviewer Mistakes

### Mistake 1: Skimming Non-Claims

Some reviewers see a non-claim like "not POSIX-first" and assume POSIX support is missing entirely. In this system, POSIX is a dialect cage — it is supported, but as an observed dialect, not as native law.

**Correct interpretation:** Non-claims define architectural boundaries, not feature gaps.

### Mistake 2: Judging Verbosity as Poor Style

Code and documentation are intentionally verbose about evidence chains, residuals, and rationales.

**Correct interpretation:** Verbose forensic commentary preserves auditable context. Removing it would break the evidence architecture.

### Mistake 3: Assuming Traditional Layering

This system does not follow traditional OS layering (libc → POSIX → kernel → drivers).

**Correct interpretation:** The layering is evidence-first: nucleus → kernel capabilities → dialect cages → forensic store → courts. Do not evaluate it using POSIX/Unix architectural assumptions.

### Mistake 4: Looking Only at Code

In a traditional project, the code is the primary artifact. In this project, the evidence chain (residuals, receipts, court verdicts) is equally important.

**Correct interpretation:** Code and evidence are a single unit. Review both.

### Mistake 5: Expecting Traditional Package Structure

Packages are not installed directories. They are sealed semantic containers.

**Correct interpretation:** A `.phor-spec` file is a complete package with source, binary, evidence, and signatures. There is no `/usr/bin`, `/etc`, or `/var` equivalent in the traditional sense.

## How to Distinguish Implemented, Verified, Claimed, and Future

Every component has a status:

```text
Status markers in documentation:

[IMPLEMENTED]  — Code exists, tests pass, evidence chain is complete
[VERIFIED]     — Code exists, tests pass, court sessions verified, oracle compared
[CLAIMED]      — Design is specified, but implementation may be partial
[FUTURE]       — Design is outlined, not yet implemented
```

When reviewing:

- Do not criticize a [FUTURE] component as incomplete — it is by design.
- Do not treat a [CLAIMED] component as [VERIFIED] — check the status.
- Do evaluate whether [IMPLEMENTED] and [VERIFIED] components match their documentation.

## Evidence Traceability

Every claim in the documentation should be traceable to:

1. Source code that implements it
2. Test fixture that exercises it
3. Court session that verifies it
4. Residual pattern that documents it
5. Golden file that captures expected output

If a claim lacks traceability, it is either [CLAIMED] (designed but not yet full-implemented) or needs a reviewer flag.

## Writing Review Reports

A review report should include:

```text
ReviewReport:
  reviewer_identity
  component_under_review
  claims_verified:
    - claim: "..."
      status: Verified | Rejected | NeedsWork
      evidence: [references to files, court sessions, residuals]
  claims_not_verified:
    - claim: "..."
      status: IncompleteEvidence | NotTested | DesignOnly
      notes: "..."
  non_claims_respected:
    - non_claim: "..."
      status: Respected | Violated
  evidence_chain:
    status: Complete | Partial | Broken
    missing_links: [...]
  overall_verdict:
    status: Accept | ConditionalAccept | Reject
    conditions: [...] (if applicable)
```

## Final Reminder

This system is not Unix. It is not POSIX. It is not a Linux clone. It is not a normal compatibility layer. It is not an edge-only OS.

It is a forensic residual-primacy computing substrate where evidence is the first-class citizen.

Review it on its own terms.
