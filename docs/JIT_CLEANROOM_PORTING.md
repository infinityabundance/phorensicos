# Just-In-Time Clean-Room Black-Box Porting

## Overview

Porting in Phorensic OS is an OS-level service, not an external tool. Foreign code is methodically observed, caged, residualized, replayed, explained, sealed, and progressively replaced with native safe semantic slices.

**Replace only what has been observed, explained, replayed, and sealed.**

Unknown regions remain sandboxed, interpreted, provisional, fail-closed, or kept as foreign dialect evidence.

## Porting Pipeline

```text
Foreign Code
→ Dialect Classification
→ Cage Loading
→ Observation / Execution
→ Residual Extraction
→ Semantic Analysis
→ Candidate Translation Generation
→ Replay Verification
→ Oracle Comparison
→ Court Verification
→ Native Slice Extraction
→ Slice Sealing
→ Package Container Assembly
→ Promotion into Forensic Store
```

## Stage Detail

### 1. Dialect Classification

The foreign code is scanned to determine its dialect profile:

```text
DialectClassification:
  - source language(s) detected
  - ABI family
  - syscall surface
  - library dependencies
  - build system type
  - target platform assumptions
  - compiler/runtime version requirements
  - undefined behavior reliance
  - dialect complexity score
```

### 2. Cage Loading

Based on classification, an appropriate dialect cage is created and configured:

```text
CageLoading:
  - create cage with matched dialect profile
  - configure initial capability grants (often empty or minimal)
  - load foreign code (source, object, or binary)
  - set execution bounds (time, memory, effects)
  - register oracle interface (if available)
```

### 3. Observation / Execution

The caged foreign code is executed in observed mode:

```text
Observation:
  - capture all syscalls/intercepted operations
  - record memory access patterns
  - log capability checks
  - trace dialect boundary crossings
  - measure resource usage
  - record timing nondeterminism
  - capture output behavior
```

### 4. Residual Extraction

All observed behavior is structured into residuals:

```text
ResidualExtraction:
  - operation vectors (sequence of all operations)
  - data flow graph (which operations depend on which)
  - capability usage patterns
  - effect signatures
  - boundary transitions
  - oracle deltas (if oracle comparison performed)
```

### 5. Semantic Analysis

Residuals are analyzed to infer semantic intent:

```text
SemanticAnalysis:
  - group operations into functional clusters
  - identify control flow patterns
  - recognize common idioms
  - map foreign operations to candidate native equivalents
  - flag ambiguous or underspecified behavior
  - produce confidence scores for each candidate mapping
```

### 6. Candidate Translation Generation

Candidate translations are generated for observed operations:

```text
CandidateGeneration:
  - for each observed foreign operation:
    - propose one or more native operation sequences
    - annotate each proposal with confidence and evidence references
  - generate candidate native code slices
  - preserve original behavior specification
```

**Candidate translations are never trusted directly.** They must pass replay and court verification.

### 7. Replay Verification

Each candidate translation is replayed in a controlled environment:

```text
ReplayVerification:
  - execute candidate native slice with test inputs
  - compare output and behavior with caged foreign execution
  - measure behavioral match (identical, equivalent, diverging)
  - produce replay report with residuals
  - if match: promote candidate to verified
  - if mismatch: refine or reject candidate
```

### 8. Oracle Comparison

If a reference oracle is available, candidate output is compared against it:

```text
OracleComparison:
  - execute same inputs on oracle reference (e.g., reference compiler, reference OS)
  - compare oracle output with native candidate output
  - measure divergence
  - record oracle residuals
  - if match: promote trust state
  - if mismatch: investigate divergence cause
```

### 9. Court Verification

A formal court session verifies the translation:

```text
CourtVerification:
  - present all evidence: residuals, replay reports, oracle comparisons
  - court evaluates:
    - is the behavioral match sufficient?
    - are all edge cases covered?
    - are there residual discrepancies?
    - is the translation safe to promote?
  - court issues verdict: accept, refine, reject, or require more evidence
```

### 10. Native Slice Extraction

Successful translations become native semantic slices:

```text
NativeSliceExtraction:
  - extract verified translation as standalone native code
  - compile with Phorensic compiler (full receipts)
  - verify slice against original behavior
  - seal slice with evidence chain
```

### 11. Slice Sealing

Each native slice is sealed into the forensic store:

```text
SliceSealing:
  - create StoreObject with slice binary and evidence
  - sign with developer and auditor keys
  - record trust state (usually Sealed or Promoted)
  - link to original foreign code provenance
```

### 12. Package Container Assembly

The native slices and remaining caged code are assembled into a package:

```text
PackageAssembly:
  - native slices replace equivalent foreign operations
  - remaining foreign code stays in cage
  - package contains:
    - native slices (sealed)
    - residual cage evidence
    - translation receipts
    - court verdicts
    - source↔binary seals for native slices
  - package has explicit: this is partially promoted
```

### 13. Promotion into Forensic Store

The package is stored and made available for activation:

```text
StorePromotion:
  - store package in forensic store
  - update generation with new package reference
  - make available for activation (subject to trust policy)
```

## Core Rule

**Replace only what has been observed, explained, replayed, and sealed.**

This means:

- A foreign function that has been executed 10 times with identical behavior may be a candidate for native replacement.
- A foreign function that has not been executed remains in the cage.
- A foreign function whose behavior is not fully understood remains in the cage.
- A foreign function whose native replacement fails replay verification stays in the cage.
- Unknown regions are never promoted.

## Dialect as Residual Surface

POSIX, GNU, Win32, BSD, C, assembly dialects, build systems, ABIs, object formats, linkers, vendor runtimes, GUI protocols, compositor protocols, kernel APIs, and package systems are import dialects.

They are not native OS law.

A dialect is observed, not inherited.

## Example: Porting a File I/O Operation

```text
Foreign Code Observation:
  - Operation: fopen("config.cfg", "r")  returns FILE*
  - Subsequent: fread(buf, 1, 256, fp)    returns 256
  - Subsequent: fclose(fp)                 returns 0

Residual Extraction:
  - fopen: file path = "config.cfg", mode = "r"
    → maps to native: store.resolve("config.cfg") with read capability
  - fread: read 256 bytes from position 0
    → maps to native: object.read(0, 256)
  - fclose: release handle
    → maps to native: drop capability

Candidate Translation:
  - fopen → StoreManager.resolve_node(hash_of("config.cfg")) + get_read_capability
  - fread → capability.read(0, 256)
  - fclose → session.drop(capability)

Replay Verification:
  - Run native translation with same config.cfg content
  - Compare: same bytes, same behavior, same residual pattern
  - Replay report: MATCH

Court Verification:
  - Evidence: residuals, replay report, oracle comparison
  - Verdict: ACCEPT — file I/O translation is behaviorally equivalent

Native Slice:
  - "config file reader" native implementation
  - Full Phorensic source + compilation receipts
  - Sealed with evidence chain

Package:
  - Original foreign code (C source) in dialect cage
  - Replaced: file I/O operations → native store operations
  - Remaining: computation logic still in cage (not yet observed)
  - Trust state: partially promoted (Sealed for I/O, Observed for computation)
```

## Candidate Semantic Witnesses

Machine-suggested semantics may be used only as candidate witnesses:

```text
candidate rule
→ generated replay cases
→ oracle comparison
→ residuals
→ accept/refine/reject
→ Atlas entry
```

This is not "AI writes the port." It is candidate generation under forensic courts.

## Just-In-Time Porting Example

```text
// Foreign code arrives as C source (a small utility)
// No prior knowledge of this utility exists

// Step 1: Classify
let profile = classify_dialect("posix-c-v1", code)
let cage = create_cage(profile)

// Step 2: Observe
cage.grant_capability(fs_read_cap)?
cage.grant_capability(stdout_cap)?
let result = cage.execute("main", ["input.txt"])?
let residuals = cage.extract_residuals()

// Step 3: Analyze
let analysis = analyze_residuals(residuals)
// Found: open, read, write (stdout), close
// Confidence: high for open/read/close, medium for write

// Step 4: Candidate generation
let candidates = generate_candidates(analysis)
// candidate[0]: open → store.resolve (confidence 0.95)
// candidate[1]: read → store.read (confidence 0.95)
// candidate[2]: write → port.write (confidence 0.80)

// Step 5: Verify and court
for candidate in candidates {
    let replay = verify_replay(candidate, residuals)
    if replay.matches {
        let verdict = court_verify(candidate, replay, oracle)?
        if verdict == Accept {
            let slice = extract_native_slice(candidate)
            seal_slice(slice)
        }
    }
}

// Step 6: Package
let pkg = assemble_package(original_code, native_slices, cage_remainder)
store.store_package(pkg)
// pkg has partial promotion: I/O is native, logic is still caged
```
