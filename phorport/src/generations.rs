// phorport/generations.rs — publishing immutable store generations (§19)
//
// The committed store is a single generation (the sealed baseline). A successful
// autonomous campaign publishes generation N+1 from generation N rather than
// mutating the baseline. This module builds the genesis generation from the
// committed index and publishes a new sealed port into a child generation.

use phost::porting::autonomous_seal::SealProfile;
use phost::porting::ident::EvidenceClosureId;
use phost::porting::store_generation::{GenerationEntry, StoreGeneration};
use phost::porting::SealedPortIndex;

/// Build the genesis generation from a verified sealed-port index.
///
/// The legacy store's evidence did not carry an autonomous closure, so the
/// genesis closure is the deterministic digest of the entries it publishes; a
/// later autonomous publication binds its own closure as the child generation.
pub fn genesis_from_index(index: &SealedPortIndex) -> Result<StoreGeneration, String> {
    let mut entries: Vec<GenerationEntry> = Vec::new();
    for e in index.entries() {
        entries.push(GenerationEntry::new(
            e.target.clone(),
            e.artifact_hash(),
            SealProfile::LegacyV1,
            format!("legacy:{}:{}", e.target, e.artifact_hash()),
        ));
    }
    let closure = EvidenceClosureId::new(genesis_closure_id(index));
    StoreGeneration::genesis(entries, closure).map_err(|e| e.as_str().to_string())
}

/// The deterministic closure identity of a sealed-port index.
pub fn genesis_closure_id(index: &SealedPortIndex) -> String {
    let mut parts: Vec<String> = index
        .entries()
        .iter()
        .map(|e| format!("{}:{}", e.target, e.artifact_hash()))
        .collect();
    parts.sort();
    crate::compile::sha256_hex(
        format!("PHOR/STORE-GENERATION/genesis/v1|{}", parts.join("|")).as_bytes(),
    )
}

/// Publish one newly sealed port into a child generation.
pub fn publish_port(
    parent: &StoreGeneration,
    target: &str,
    artifact_hash: &str,
    profile: SealProfile,
    evidence_id: &str,
    closure: EvidenceClosureId,
) -> Result<StoreGeneration, String> {
    StoreGeneration::publish(
        parent,
        vec![GenerationEntry::new(
            target,
            artifact_hash,
            profile,
            evidence_id,
        )],
        closure,
    )
    .map_err(|e| e.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::store_generation::GenerationLedger;

    #[test]
    fn test_genesis_reproduces_the_committed_store_and_publishes_a_child() {
        let index = phost::porting::store::load_default().expect("committed store");
        let g0 = genesis_from_index(&index).expect("genesis");
        assert_eq!(g0.entries.len(), index.len());
        g0.verify().expect("genesis verifies");
        // Deterministic.
        let g0b = genesis_from_index(&index).expect("genesis");
        assert_eq!(g0.generation_id, g0b.generation_id);

        // Publish a newly sealed port as generation 1 (strspn is already in the
        // baseline; use a surface the store does not yet contain).
        let closure = EvidenceClosureId::new("autonomous-closure");
        let g1 = publish_port(
            &g0,
            "posix:strcspn:c-locale:u64:v1",
            "autonomous-object-hash",
            SealProfile::AutonomousV1,
            "phor.promotion:receipt",
            closure,
        )
        .expect("publish");
        assert_eq!(g1.parent.as_ref(), Some(&g0.generation_id));
        assert_eq!(g1.entries.len(), g0.entries.len() + 1);
        assert_eq!(
            g1.artifact_for("posix:strcspn:c-locale:u64:v1"),
            Some("autonomous-object-hash")
        );
        // The baseline entry is carried forward unchanged.
        assert_eq!(
            g1.artifact_for("posix:strspn:c-locale:u64:v1"),
            g0.artifact_for("posix:strspn:c-locale:u64:v1")
        );
        g1.verify().expect("child verifies");

        // The ledger accepts the chain and refuses a rollback.
        let mut ledger = GenerationLedger::new();
        ledger.append(g0.clone()).unwrap();
        ledger.append(g1.clone()).unwrap();
        assert_eq!(ledger.len(), 2);
        assert!(ledger
            .open(&g0.generation_id, Some(&g1.generation_id))
            .is_err());
    }
}
