# Sealed Package Format (.phor-spec)

## Overview

A Phorensic OS package is not an installed directory. It is a sealed semantic container that includes source, binary, evidence, receipts, and signatures in one verifiable bundle.

## Mandatory Package Source-Seal Rule

Every Phorensic OS package must include its source.

A package is invalid unless it contains:

```text
source snapshot
source manifest
source hash
build recipe
compiler/court versions
emitted binary
binary hash
source ↔ binary byte-comparison receipt
signature over source hash
signature over binary hash
signature over full source→build→binary evidence chain
```

**No source, no sealed package. No source↔binary signature chain, no promotion.**

## Package Container Structure

```text
PackageContainer.phor-spec
├── phor-package-header        # magic, version, format version
├── source-evidence/
│   ├── source-snapshot.hash
│   ├── source-snapshot.sig
│   ├── source-manifest.json
│   └── source-archive.phor-source
├── build-evidence/
│   ├── build-recipe.json
│   ├── dialect-profile.json
│   ├── compiler-version.info
│   ├── environment-capsule.json
│   └── dependency-graph.json
├── expansion-evidence/
│   ├── expansion-receipts.phor-receipts
│   ├── include-resolution-log.json
│   └── macro-expansion-trace.json
├── compilation-evidence/
│   ├── parse-receipt.phor-receipt
│   ├── expansion-receipt.phor-receipt
│   ├── lowering-receipt.phor-receipt
│   ├── optimization-receipts.phor-receipts
│   ├── codegen-receipt.phor-receipt
│   └── link-receipt.phor-receipt
├── binary-evidence/
│   ├── executable-image.phor-bin
│   ├── binary.hash
│   ├── binary.sig
│   ├── source-to-binary-seal.json
│   └── byte-comparison-receipt.json
├── replay-evidence/
│   ├── replay-checkpoints.phor-checkpoints
│   ├── residual-graph.phor-graph
│   └── replay-verification-report.json
├── court-evidence/
│   ├── oracle-traces.phor-traces
│   ├── oracle-comparison-results.json
│   ├── court-verdicts.json
│   └── trust-promotion-records.json
└── signatures/
    ├── full-container.sig
    ├── evidence-chain.sig
    └── auditor-stamps.json
```

## Header Format

```text
PackageHeader:
  magic: "PHORSPEC\x00"
  format_version: u32
  container_version: u32
  package_name: String
  package_version: semver::Version
  target_arch: String
  target_profile: String
  source_hash: Hash256
  binary_hash: Hash256
  container_hash: Hash256
  trust_state: TrustState
  created_at: Timestamp
  created_by: Identity
```

## Source Evidence

The source archive must contain everything needed to reproduce the build:

```text
SourceEvidence:
  source_hash: Hash256
  source_manifest: list of (path, hash, size)
  source_archive: compressed tar or equivalent
  source_signature: signature over source_hash
```

## Build Evidence

The build recipe and environment must be fully specified:

```text
BuildEvidence:
  recipe: BuildRecipe
  dialect_profile: DialectProfile
  compiler_version: semantic version + compiler hash
  compiler_image: StoreKey reference
  environment_capsule: {
    cpu_features: [...],
    memory_config: {...},
    device_map: {...},
    oracle_identities: [...],
    court_versions: {...},
  }
  dependency_graph: list of (package_name, version, store_key)
```

## Expansion Evidence

All expansion activity is recorded:

```text
ExpansionEvidence:
  expansion_receipts: list of ExpansionReceipt
  include_resolution_log: list of (include_path, resolved_path, hash)
  macro_expansion_trace: list of (macro_name, input_fragment, expanded_output)
```

## Compilation Evidence

Each compilation stage receipt:

```text
CompilationEvidence:
  parse_receipt: StageReceipt
  expansion_receipt: StageReceipt
  lowering_receipt: StageReceipt
  optimization_receipts: list of (pass_name, StageReceipt)
  codegen_receipt: StageReceipt
  link_receipt: StageReceipt
```

Each StageReceipt:

```text
StageReceipt:
  stage_name: String
  stage_version: String
  input_hash: Hash256
  output_hash: Hash256
  applied_rules: list of (rule_id, rule_version)
  residuals: list of ResidualRecord
  timestamp: Timestamp
```

## Binary Evidence

The binary and its correspondence to source:

```text
BinaryEvidence:
  executable_image: bytes
  binary_hash: Hash256
  binary_signature: signature over binary_hash
  source_to_binary_seal: {
    source_hash: Hash256
    binary_hash: Hash256
    seal_type: "full-source-to-binary"
    seal_scheme: String
    seal_signature: signature over (source_hash || binary_hash)
  }
  byte_comparison_receipt: {
    // Demonstration that each byte in the binary corresponds to
    // a provenance trail back to source
    provenance_map: list of (byte_offset, source_location, pass, rule)
  }
```

## Replay Evidence

Evidence that the package can be replayed:

```text
ReplayEvidence:
  replay_checkpoints: list of (stage, state_hash, input_hash, output_hash)
  residual_graph: directed graph of residuals showing causal dependencies
  replay_verification_report: {
    replay_status: "full" | "partial" | "failed"
    replay_hash_chain: list of Hash256
    replay_residuals: list of ResidualRecord
    replay_verdict: "matches" | "diverges" | "inconclusive"
  }
```

## Court Evidence

Oracle comparison and court results:

```text
CourtEvidence:
  oracle_traces: list of (oracle_id, input, output, timestamp)
  oracle_comparison_results: list of {
    comparison_id: String
    oracle_id: String
    native_output: Hash256
    oracle_output: Hash256
    divergence: "none" | "acceptible" | "significant"
    comparison_residuals: list of ResidualRecord
  }
  court_verdicts: list of {
    court_id: String
    case_id: String
    object_under_judgment: Hash256
    verdict: ObjectVerdict
    verdict_timestamp: Timestamp
    judge_identity: Identity
  }
  trust_promotion_records: list of {
    object_hash: Hash256
    from_state: TrustState
    to_state: TrustState
    evidence_references: list of Hash256
    court_verdict_references: list of CaseId
    promotion_timestamp: Timestamp
  }
```

## Signatures

Multiple signatures over the container:

```text
Signatures:
  full_container_signature: signature over full container hash
  evidence_chain_signature: signature over ordered chain of evidence hashes
  auditor_stamps: list of {
    auditor_identity: Identity
    stamp_type: "review" | "audit" | "certify"
    stamp_timestamp: Timestamp
    stamp_signature: signature over container_hash
  }
```

## Package Verification

The verifier rejects any package where:

- source is missing
- source hash does not match embedded source
- binary hash does not match emitted binary
- source/binary chain is unsigned
- source and binary are signed independently but not sealed together
- build recipe is missing
- replay/court receipts are missing or stale

```text
verify_package(package) -> VerificationReport {
    checks: [
        check_present(source_evidence),
        check_hash(source_hash, source_archive),
        check_present(binary_evidence),
        check_hash(binary_hash, executable_image),
        check_signature(source_sig, source_hash, trusted_keys),
        check_signature(binary_sig, binary_hash, trusted_keys),
        check_seal_signature(seal_sig, source_hash || binary_hash, trusted_keys),
        check_present(build_recipe),
        check_receipts_chain(compilation_evidence),
        check_replay_verification(replay_evidence),
        check_court_verdicts(court_evidence),
    ],
    result: all checks pass ? SealVerified : SealBroken
}
```

## .phor-spec Package Example

```text
Package: editor.phor-spec
Version: 1.2.3
Source Hash: 0xA1B2C3...
Binary Hash: 0xD4E5F6...
Container Hash: 0xG7H8I9...
Trust State: Promoted (6)

Source Evidence:
  - Source contains all .phor files, build recipe, test fixtures
  - Source hash matches
  - Source signed by developer key

Build Evidence:
  - Recipe: phorensic-build -c optimized -t x86_64
  - Dialect: native-phorensic-v1
  - Compiler: phc 2.1.0 (hash 0xJ0K1L2...)
  - Dependencies: stdlib.phor-spec 1.0.0, gui.phor-spec 2.0.0

Binary Evidence:
  - Binary hash matches
  - Source ↔ binary seal: verified
  - Byte-comparison receipt shows full provenance chain

Court Evidence:
  - Oracle: reference compiler v2.0.0 produced identical output
  - Court: replay court confirmed full replayed match
  - Verdict: identity-preserved

Signatures:
  - Developer signature: valid
  - Auditor stamp: verified (3 auditors)
```
