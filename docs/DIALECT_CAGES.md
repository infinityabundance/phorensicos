# Dialect Cages Specification

## Overview

Dialect cages are the mechanism by which Phorensic OS imports, observes, and isolates foreign code. A dialect cage is a bounded execution environment that captures residuals from foreign code execution, enabling progressive understanding and eventual native replacement.

## Dialect Concept

A dialect is any non-native computing convention: POSIX, GNU, Win32, BSD, C, assembly dialects, build systems, ABIs, object formats, linkers, vendor runtimes, GUI protocols, compositor protocols, kernel APIs, and package systems.

A dialect is observed, not inherited. Every dialect interaction produces residuals.

## Cage Architecture

```text
DialectCage =
  dialect_profile      // which dialect(s) this cage handles
  code_store           // loaded foreign code
  translation_map      // observed foreign → native operations
  residual_buffer      // captured execution residuals
  capability_filter    // which native capabilities are reachable
  effect_boundary      // which effects are permitted
  trust_state          // current understanding of the caged code
  oracle_interface     // connection to dialect oracle
```

### Dialect Profile

```text
DialectProfile =
  dialect_name: String
  dialect_version: semver::Version
  dialect_family: String  // "posix", "win32", "c-abi", etc.
  entry_points: list of (name, signature, effects)
  syscall_table: list of (number, name, argument_types)
  library_imports: list of (name, expected_signature)
  ABI_spec: ABISpecification
  object_format: ObjectFormatSpec
  linker_assumptions: LinkerSpec
```

## Cage Operations

### Create Cage

```text
create_cage(profile: DialectProfile) -> Result[DialectCage, CageError]
```

Creates an empty cage configured for the given dialect. No capabilities are granted initially.

### Load Code

```text
load_into_cage(cage, code: bytes, format: CodeFormat) -> Result[(), CageError]
```

Loads foreign code into the cage. The format specifies whether this is source, object, or binary.

### Grant Capability

```text
grant_cage_capability(cage, cap: Capability) -> Result[(), CageError]
```

Grants a native capability to the cage. Each grant is logged as a residual.

### Execute

```text
execute_in_cage(cage, entry: String, args: Arguments) -> Result[CageResult, CageError]
```

Executes the caged code at the specified entry point with the given arguments. Execution is bounded (time, memory, effects). All observable behavior is captured as residuals.

### Extract Residuals

```text
extract_residuals(cage) -> Sequence[ResidualRecord]
```

Returns all residuals captured during execution, including:

- each syscall/intercepted operation
- each memory access outside permitted regions
- each capability check
- each dialect translation event
- timing and resource usage
- oracle comparison results (if oracle available)

### Close Cage

```text
close_cage(cage) -> CageReport
```

Finalizes the cage and produces a comprehensive report:

```text
CageReport =
  cage_id
  dialect_profile
  loaded_code_hash
  execution_count
  observed_operations: list of OperationRecord
  translation_attempts: list of TranslationRecord
  successful_translations: u32
  failed_translations: u32
  capability_grants: list of CapabilityGrant
  effect_violations: list of EffectViolation
  trust_recommendation: TrustState
  residual_collection: Sequence[ResidualRecord]
```

## Translation Maps

As the cage observes foreign code, it builds a translation map:

```text
TranslationMap =
  entries: list of TranslationEntry

TranslationEntry =
  foreign_operation: OperationSignature
  native_operation: OperationSignature
  observation_count: u32
  oracle_verified: bool
  court_verified: bool
  residual_references: list of ResidualRecordRef
```

Translations start as candidate (observed once) and progress through replay and court verification to become promoted (stable, trusted).

## C Import Cage

C projects are scanned for:

- build context
- macros
- headers
- ABI assumptions
- syscalls
- environment use
- locale use
- file behavior
- undefined-but-relied-on behavior
- compiler flags
- linker assumptions
- generated files
- configure assumptions

POSIX/GNU calls are translated into native semantic operations only when observed and justified. Every translation is logged as residual evidence.

## Expansion Receipts as Residuals

Expansion systems are high-value residual surfaces. The dialect cage captures:

- macro expansion
- template expansion
- include resolution
- C preprocessor operations
- M4 expansion
- Autoconf detection
- Automake rule application
- Make variable expansion
- CMake command execution
- LLVM TableGen expansion
- MLIR dialect expansion
- build recipe expansion
- package derivation expansion

Each expansion receipt records:

```text
ExpansionReceipt =
  input_fragment
  macro/template/include context
  rule_selected
  dialect_profile_flags
  symbol_table_before
  symbol_table_after
  emitted_bytes
  oracle_bytes (if oracle available)
  diff_class: "identical" | "equivalent" | "divergent" | "new"
  provenance
  casefile_id
  replay_seed
  deterministic_hash
```

## Cage Trust Progression

A cage's trust state evolves as understanding grows:

```text
Unknown
  → Observed (first execution captured)
  → Replayed (execution replay verified)
  → OracleCompared (compared against reference oracle)
  → ResidualStable (residual pattern is consistent)
  → Sealed (translation map sealed with evidence)
  → Promoted (native replacement available)
```

## Example: C Dialect Cage

```text
// Create a POSIX/ C dialect cage
let profile = DialectProfile::load("posix-c-v1.phor-dialect")
let cage = DialectCageManager.create_cage(profile)?

// Grant limited capabilities
let fs_read = session.get_capability("filesystem_read")?
let alloc = session.get_capability("bounded_alloc_64k")?
cage.grant_capability(fs_read)?
cage.grant_capability(alloc)?

// Load foreign code
let code = read_file("foreign_app.c")
cage.load_into_cage(code, CodeFormat::Source)?

// Execute observed entry
let result = cage.execute_in_cage("main", ["--help"])?

// Extract residuals
let residuals = cage.extract_residuals()
for r in residuals {
    if r.operation == "open" {
        // Observed: POSIX open() → native StoreNode resolution
        translation_map.add_candidate("posix.open", "store.resolve_node")
    }
}

// Close cage and get report
let report = cage.close_cage()
// report.trust_recommendation = Observed
// report.successful_translations = 12
// report.failed_translations = 3
```
