// porting/registry.rs — the extension registry (the one target table)
//
// Phase 1 of the autonomous porting foundry. Before this module, the generic
// machinery contained a chain of target-id branches in three places:
//
//   * `target::cases_for`      — `match target.id { … }` over the corpus generators
//   * `target::resolve_target` — `match name { … }` over the six names
//   * `candidate::run_candidate` — `if target_id == …` over the Rust mirrors
//
// …and the ABI harness in `exec.rs` branched the same way. That is precisely the
// shape a generic engine must not have: adding a target would mean editing the
// engine, and the engine would slowly become a list of special cases.
//
// This module is the single registration table. Each row binds:
//
//   * the target's canonical `PortSpec` (Phase 1 `docs/PORT_SPEC.md`), which is
//     the identity a seal binds;
//   * the bootstrap `PortTarget` record (kept for the existing evidence model);
//   * the **case generator**   (a `CaseGenerator` extension point);
//   * the **candidate adapter** (the clean-room Rust mirror, `candidate.rs`);
//   * the **ABI adapter**       (`abi.rs`) — how the compiled entry is called.
//
// The foreign **oracle adapter** remains `dialect_cage.rs`; it observes foreign
// symbols through FFI and is a separate extension boundary.
//
// Adding a leaf target is therefore a new narrowly scoped set of extension
// points plus a new `PortSpec`, and *no* change to the generic engine. A static
// audit (`test_generic_machinery_has_no_target_branches`) fails the build if a
// target-id branch reappears in a module declared generic.

use alloc::vec::Vec;

use crate::porting::abi::{self, AbiFn};
use crate::porting::candidate::{
    cand_memchr, cand_memcmp, cand_strlen, cand_strrchr, cand_strspn, cand_toupper, CandidateError,
};
use crate::porting::portspec::{
    PortSpec, SPEC_MEMCHR, SPEC_MEMCMP, SPEC_STRLEN, SPEC_STRRCHR, SPEC_STRSPN, SPEC_STRSPN_ISO_C,
    SPEC_TOUPPER,
};
use crate::porting::target::{
    byte_domain_cases, memchr_corpus, memcmp_corpus, strlen_corpus, strrchr_corpus, strspn_corpus,
    PortTarget, TestCase, LIBC_MEMCHR, LIBC_MEMCMP, LIBC_STRLEN, LIBC_STRRCHR, LIBC_STRSPN,
    LIBC_TOUPPER, POSIX_STRSPN,
};

/// One registered leaf target: its spec, its bootstrap record, and its three
/// extension points.
#[derive(Clone, Copy)]
pub struct PortExtension {
    /// The canonical specification (the identity a seal binds).
    pub spec: &'static PortSpec,
    /// The bootstrap target record, so existing evidence and the seal model are
    /// preserved (Gate A).
    pub target: PortTarget,
    /// The deterministic `CaseGenerator` extension point.
    pub cases: fn() -> Vec<TestCase>,
    /// The clean-room candidate adapter.
    pub candidate: fn(&[Vec<u8>]) -> Result<Vec<u8>, CandidateError>,
    /// The ABI adapter for the compiled entry point.
    pub abi: AbiFn,
}

/// The one target table. Order matches the store's canonical leaf order, and the
/// **last** row is the corrected `strspn` successor: because `extension_by_name`
/// finds the first match, bare-symbol resolution (`"strspn"`) keeps returning the
/// historical target (baseline preservation), while the successor is addressed by
/// its full id (`libc:strspn:c-locale:u64:v1`).
pub const EXTENSIONS: [PortExtension; 7] = [
    PortExtension {
        spec: &SPEC_TOUPPER,
        target: LIBC_TOUPPER,
        cases: byte_domain_cases,
        candidate: cand_toupper,
        abi: abi::toupper,
    },
    PortExtension {
        spec: &SPEC_MEMCMP,
        target: LIBC_MEMCMP,
        cases: memcmp_corpus,
        candidate: cand_memcmp,
        abi: abi::memcmp,
    },
    PortExtension {
        spec: &SPEC_MEMCHR,
        target: LIBC_MEMCHR,
        cases: memchr_corpus,
        candidate: cand_memchr,
        abi: abi::memchr,
    },
    PortExtension {
        spec: &SPEC_STRLEN,
        target: LIBC_STRLEN,
        cases: strlen_corpus,
        candidate: cand_strlen,
        abi: abi::strlen,
    },
    PortExtension {
        spec: &SPEC_STRRCHR,
        target: LIBC_STRRCHR,
        cases: strrchr_corpus,
        candidate: cand_strrchr,
        abi: abi::strrchr,
    },
    PortExtension {
        spec: &SPEC_STRSPN,
        target: POSIX_STRSPN,
        cases: strspn_corpus,
        candidate: cand_strspn,
        abi: abi::strspn,
    },
    // The corrected `strspn` successor (docs/CONTRACT_PROVENANCE_MIGRATION.md).
    // Same surface and extension points as the historical row; a different
    // provenance claim and therefore a different `PortSpecId`.
    PortExtension {
        spec: &SPEC_STRSPN_ISO_C,
        target: LIBC_STRSPN,
        cases: strspn_corpus,
        candidate: cand_strspn,
        abi: abi::strspn,
    },
];

/// Every registered extension, in canonical order.
pub fn all() -> &'static [PortExtension] {
    &EXTENSIONS
}

/// Look an extension up by qualified target id.
pub fn extension_for(target_id: &str) -> Option<&'static PortExtension> {
    EXTENSIONS.iter().find(|e| e.spec.target_id == target_id)
}

/// Look an extension up by bare symbol or qualified target id.
pub fn extension_by_name(name: &str) -> Option<&'static PortExtension> {
    EXTENSIONS
        .iter()
        .find(|e| e.spec.symbol == name || e.spec.target_id == name)
}

/// The canonical `PortSpec` for a target id, if registered.
pub fn spec_for(target_id: &str) -> Option<&'static PortSpec> {
    extension_for(target_id).map(|e| e.spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target;

    #[test]
    fn test_every_spec_has_exactly_one_extension() {
        assert_eq!(all().len(), 7);
        for spec in crate::porting::portspec::all() {
            let matches = EXTENSIONS
                .iter()
                .filter(|e| e.spec.target_id == spec.target_id)
                .count();
            assert_eq!(
                matches, 1,
                "{} must have exactly one extension",
                spec.target_id
            );
        }
        // And every extension's spec is in the canonical spec list.
        for e in all() {
            assert!(
                crate::porting::portspec::by_target_id(e.spec.target_id).is_some(),
                "{} extension has no spec",
                e.spec.target_id
            );
        }
    }

    #[test]
    fn test_registry_is_the_source_of_truth_for_names() {
        for e in all() {
            // Every row resolves by its full id (this is how the store and the
            // runtime address a port, so a duplicate symbol cannot confuse it).
            let t = target::resolve_target(e.spec.target_id).expect("resolves by id");
            assert_eq!(t.id, e.spec.target_id);
            // And a row's symbol resolves to *a* registered row with that symbol.
            let by_symbol = extension_by_name(e.spec.symbol).expect("resolves by symbol");
            assert_eq!(by_symbol.spec.symbol, e.spec.symbol);
        }
        // Bare-symbol resolution keeps returning the historical `strspn` target
        // (the successor is addressed by its full id), so the baseline verifiers
        // and the committed evidence are unaffected.
        assert_eq!(
            target::resolve_target("strspn")
                .expect("strspn resolves")
                .id,
            POSIX_STRSPN.id
        );
        assert_eq!(
            target::resolve_target(LIBC_STRSPN.id)
                .expect("successor resolves by id")
                .id,
            LIBC_STRSPN.id
        );
        assert!(extension_by_name("strcspn").is_none());
        assert!(target::resolve_target("strcspn").is_none());
    }

    #[test]
    fn test_registry_generators_and_adapters_agree_with_the_engine() {
        for e in all() {
            let cases = (e.cases)();
            assert!(!cases.is_empty(), "{} has no cases", e.spec.target_id);
            assert_eq!(target::cases_for(&e.target), cases);
            // The candidate adapter agrees with the registry-driven dispatcher.
            if let Some(first) = cases.first() {
                let direct = (e.candidate)(&first.args);
                let dispatched =
                    crate::porting::candidate::run_candidate(e.spec.target_id, &first.args);
                assert_eq!(
                    direct, dispatched,
                    "{} candidate dispatch diverged",
                    e.spec.target_id
                );
            }
        }
    }

    /// The static audit: the generic machinery must not branch on a target id.
    ///
    /// This is the guard against another central `match target.id` reappearing.
    /// The declared extension boundaries are `registry.rs`, `target.rs`,
    /// `portspec.rs`, `candidate.rs`, `abi.rs` and `dialect_cage.rs` (the oracle
    /// adapter); every other module in `porting/` is generic and audited. Test
    /// scaffolding (`#[cfg(test)]`) is not engine logic and is stripped first.
    #[test]
    fn test_generic_machinery_has_no_target_branches() {
        const GENERIC: &[&str] = &[
            "behavior_signature.rs",
            "dispatch.rs",
            "evidence.rs",
            "exec.rs",
            "mod.rs",
            "oracle_trace.rs",
            "promotion.rs",
            "replay_court.rs",
            "service.rs",
            "store.rs",
        ];
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/porting/");
        for file in GENERIC {
            let path = alloc::format!("{}{}", dir, file);
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));
            let engine = match src.find("#[cfg(test)]") {
                Some(i) => &src[..i],
                None => src.as_str(),
            };
            assert!(
                !engine.contains(".id =="),
                "{} branches on a target id (`.id ==`); move it to an extension boundary",
                file
            );
        }
    }
}
