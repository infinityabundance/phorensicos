// porting/store_generation.rs — immutable store generations (§19)
//
// Phase 8. A published port is not a mutation of the running store: it is a new
// **immutable generation**. A session binds to one generation for its whole
// lifetime, so it can never silently change semantics halfway through.
//
// A generation binds its parent, its sorted entry identities, each entry's seal
// profile, an evidence closure, and a schema version. Its identity is a pure
// function of those (no clock). Verification fails closed on a missing parent, an
// unknown schema, an entry mix-and-match, an artifact substitution or an
// evidence-closure mismatch; and a minimum-generation policy refuses a rollback.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::porting::autonomous_seal::SealProfile;
use crate::porting::ident::{EvidenceClosureId, StoreGenerationId};
use crate::porting::sha256_hex;

/// The content-identity domain tag for a generation.
pub const STORE_GENERATION_DOMAIN: &[u8] = b"PHOR/STORE-GENERATION/v1\0";
/// The schema this build writes and accepts.
pub const STORE_GENERATION_SCHEMA: u32 = 1;

/// One sealed port in a generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationEntry {
    /// The qualified port target id (leaf or composition).
    pub target: String,
    /// The published artifact identity: a leaf object hash or a chain hash.
    pub artifact_hash: String,
    /// The profile the entry was sealed under.
    pub seal_profile: SealProfile,
    /// The evidence identity that licensed the entry (a promotion receipt id or
    /// an evidence closure id).
    pub evidence_id: String,
}

impl GenerationEntry {
    pub fn new(
        target: impl Into<String>,
        artifact_hash: impl Into<String>,
        seal_profile: SealProfile,
        evidence_id: impl Into<String>,
    ) -> Self {
        GenerationEntry {
            target: target.into(),
            artifact_hash: artifact_hash.into(),
            seal_profile,
            evidence_id: evidence_id.into(),
        }
    }

    fn canonical_into(&self, out: &mut Vec<u8>) {
        enc(out, &self.target);
        enc(out, &self.artifact_hash);
        out.push(self.seal_profile.as_tag());
        enc(out, &self.evidence_id);
    }
}

fn enc(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Why a generation was refused. Every variant fails closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationError {
    UnknownSchema(u32),
    Empty,
    DuplicateTarget,
    MissingParent,
    ParentMismatch,
    SelfParent,
    IdentityMismatch,
    ClosureMismatch,
    RollbackRefused,
    ArtifactNotInGeneration,
    EntrySubstitution,
    MalformedEntry,
}

impl GenerationError {
    pub fn as_str(&self) -> &'static str {
        match self {
            GenerationError::UnknownSchema(_) => "unknown store-generation schema version",
            GenerationError::Empty => "store generation is empty",
            GenerationError::DuplicateTarget => "duplicate target in a store generation",
            GenerationError::MissingParent => "a parent generation is required but absent",
            GenerationError::ParentMismatch => "the parent does not match the current generation",
            GenerationError::SelfParent => "a generation cannot descend from itself",
            GenerationError::IdentityMismatch => "the recorded generation identity does not match",
            GenerationError::ClosureMismatch => "the evidence closure does not match",
            GenerationError::RollbackRefused => {
                "a rollback below the minimum generation was refused"
            }
            GenerationError::ArtifactNotInGeneration => {
                "the artifact is not the one bound by the session's generation"
            }
            GenerationError::EntrySubstitution => "an entry was substituted",
            GenerationError::MalformedEntry => "a generation entry is malformed",
        }
    }
}

/// An immutable store generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGeneration {
    pub schema_version: u32,
    /// The content identity (recomputable; verified by [`Self::verify`]).
    pub generation_id: StoreGenerationId,
    pub parent: Option<StoreGenerationId>,
    /// Entries, sorted by target, with unique targets.
    pub entries: Vec<GenerationEntry>,
    pub evidence_closure: EvidenceClosureId,
}

impl StoreGeneration {
    /// The genesis generation: the sealed baseline.
    pub fn genesis(
        entries: Vec<GenerationEntry>,
        closure: EvidenceClosureId,
    ) -> Result<StoreGeneration, GenerationError> {
        let mut sorted = entries;
        sorted.sort_by(|a, b| a.target.cmp(&b.target));
        let id = derive_id(None, &sorted, &closure);
        let g = StoreGeneration {
            schema_version: STORE_GENERATION_SCHEMA,
            generation_id: id,
            parent: None,
            entries: sorted,
            evidence_closure: closure,
        };
        g.validate()?;
        Ok(g)
    }

    /// Publish a new generation from `parent`, replacing additions by target.
    pub fn publish(
        parent: &StoreGeneration,
        new_entries: Vec<GenerationEntry>,
        closure: EvidenceClosureId,
    ) -> Result<StoreGeneration, GenerationError> {
        parent.verify()?;
        let mut entries = parent.entries.clone();
        for e in new_entries {
            if let Some(slot) = entries.iter_mut().find(|x| x.target == e.target) {
                *slot = e;
            } else {
                entries.push(e);
            }
        }
        entries.sort_by(|a, b| a.target.cmp(&b.target));
        let id = derive_id(Some(&parent.generation_id), &entries, &closure);
        let g = StoreGeneration {
            schema_version: STORE_GENERATION_SCHEMA,
            generation_id: id,
            parent: Some(parent.generation_id.clone()),
            entries,
            evidence_closure: closure,
        };
        g.validate()?;
        Ok(g)
    }

    fn validate(&self) -> Result<(), GenerationError> {
        if self.schema_version != STORE_GENERATION_SCHEMA {
            return Err(GenerationError::UnknownSchema(self.schema_version));
        }
        if self.entries.is_empty() {
            return Err(GenerationError::Empty);
        }
        if self.evidence_closure.is_empty() {
            return Err(GenerationError::ClosureMismatch);
        }
        if let Some(p) = &self.parent {
            if p == &self.generation_id {
                return Err(GenerationError::SelfParent);
            }
        }
        for (i, e) in self.entries.iter().enumerate() {
            if e.target.is_empty() || e.artifact_hash.is_empty() || e.evidence_id.is_empty() {
                return Err(GenerationError::MalformedEntry);
            }
            if i > 0 && self.entries[i - 1].target == e.target {
                return Err(GenerationError::DuplicateTarget);
            }
            if i > 0 && self.entries[i - 1].target > e.target {
                return Err(GenerationError::EntrySubstitution); // not sorted
            }
        }
        Ok(())
    }

    /// Verify the recorded identity and structure (recompute, never trust).
    pub fn verify(&self) -> Result<(), GenerationError> {
        self.validate()?;
        let expected = derive_id(self.parent.as_ref(), &self.entries, &self.evidence_closure);
        if expected != self.generation_id {
            return Err(GenerationError::IdentityMismatch);
        }
        Ok(())
    }

    /// The artifact hash bound for `target`, if any.
    pub fn artifact_for(&self, target: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.target == target)
            .map(|e| e.artifact_hash.as_str())
    }

    pub fn to_json(&self) -> String {
        let entries: Vec<String> = self
            .entries
            .iter()
            .map(|e| {
                format!(
                    "    {{\"target\": \"{}\", \"artifact_hash\": \"{}\", \"seal_profile\": \"{}\", \"evidence_id\": \"{}\"}}",
                    e.target, e.artifact_hash, e.seal_profile.as_str(), e.evidence_id
                )
            })
            .collect();
        let parent = self
            .parent
            .as_ref()
            .map(|p| format!("\"{}\"", p.as_str()))
            .unwrap_or_else(|| String::from("null"));
        format!(
            "{{\n  \"schema\": \"phorensic.porting.store_generation.v1\",\n  \"schema_version\": {},\n  \"generation_id\": \"{}\",\n  \"parent\": {},\n  \"evidence_closure\": \"{}\",\n  \"entries\": [\n{}\n  ]\n}}\n",
            self.schema_version,
            self.generation_id.as_str(),
            parent,
            self.evidence_closure.as_str(),
            entries.join(",\n")
        )
    }
}

fn derive_id(
    parent: Option<&StoreGenerationId>,
    entries: &[GenerationEntry],
    closure: &EvidenceClosureId,
) -> StoreGenerationId {
    let mut out = Vec::with_capacity(STORE_GENERATION_DOMAIN.len() + 256);
    out.extend_from_slice(STORE_GENERATION_DOMAIN);
    out.extend_from_slice(&STORE_GENERATION_SCHEMA.to_le_bytes());
    enc(&mut out, parent.map(|p| p.as_str()).unwrap_or(""));
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        e.canonical_into(&mut out);
    }
    enc(&mut out, closure.as_str());
    StoreGenerationId::new(sha256_hex(&out))
}

/// A session's binding to one generation. It can never serve an artifact that is
/// not the one its generation bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationBinding {
    pub generation_id: StoreGenerationId,
    entries: Vec<GenerationEntry>,
}

impl GenerationBinding {
    pub fn bind(generation: &StoreGeneration) -> Result<Self, GenerationError> {
        generation.verify()?;
        Ok(GenerationBinding {
            generation_id: generation.generation_id.clone(),
            entries: generation.entries.clone(),
        })
    }

    /// The artifact this binding will serve for `target`.
    pub fn expected_artifact(&self, target: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.target == target)
            .map(|e| e.artifact_hash.as_str())
    }

    /// Check that `target` is served by exactly the bound artifact.
    pub fn assert_serves(&self, target: &str, artifact_hash: &str) -> Result<(), GenerationError> {
        match self.expected_artifact(target) {
            Some(expected) if expected == artifact_hash => Ok(()),
            _ => Err(GenerationError::ArtifactNotInGeneration),
        }
    }
}

/// An ordered ledger of immutable generations.
#[derive(Clone, Debug, Default)]
pub struct GenerationLedger {
    generations: Vec<StoreGeneration>,
}

impl GenerationLedger {
    pub fn new() -> Self {
        GenerationLedger {
            generations: Vec::new(),
        }
    }

    /// Append a generation. The first must be genesis; each later one must name
    /// the current head as its parent.
    pub fn append(&mut self, g: StoreGeneration) -> Result<(), GenerationError> {
        g.verify()?;
        match self.generations.last() {
            None => {
                if g.parent.is_some() {
                    return Err(GenerationError::ParentMismatch);
                }
            }
            Some(head) => match &g.parent {
                None => return Err(GenerationError::MissingParent),
                Some(p) if p == &head.generation_id => {}
                Some(_) => return Err(GenerationError::ParentMismatch),
            },
        }
        self.generations.push(g);
        Ok(())
    }

    pub fn head(&self) -> Option<&StoreGeneration> {
        self.generations.last()
    }

    pub fn len(&self) -> usize {
        self.generations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.generations.is_empty()
    }

    pub fn get(&self, id: &StoreGenerationId) -> Option<&StoreGeneration> {
        self.generations.iter().find(|g| &g.generation_id == id)
    }

    /// Open a session at `id`, refusing a rollback below `minimum` (if any).
    pub fn open(
        &self,
        id: &StoreGenerationId,
        minimum: Option<&StoreGenerationId>,
    ) -> Result<GenerationBinding, GenerationError> {
        let g = self
            .get(id)
            .ok_or(GenerationError::ArtifactNotInGeneration)?;
        if let Some(min) = minimum {
            // A minimum means: do not open a generation older than it. Position
            // in the ledger is the total order.
            let pos = self
                .generations
                .iter()
                .position(|x| &x.generation_id == id)
                .ok_or(GenerationError::ArtifactNotInGeneration)?;
            let min_pos = self
                .generations
                .iter()
                .position(|x| &x.generation_id == min)
                .ok_or(GenerationError::RollbackRefused)?;
            if pos < min_pos {
                return Err(GenerationError::RollbackRefused);
            }
        }
        GenerationBinding::bind(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn closure(seed: &str) -> EvidenceClosureId {
        EvidenceClosureId::new(seed)
    }

    fn entry(target: &str, hash: &str, profile: SealProfile, ev: &str) -> GenerationEntry {
        GenerationEntry::new(target, hash, profile, ev)
    }

    fn genesis() -> StoreGeneration {
        StoreGeneration::genesis(
            vec![
                entry(
                    "libc:toupper:c-locale:u8:v1",
                    "aa",
                    SealProfile::LegacyV1,
                    "p1",
                ),
                entry(
                    "libc:strlen:c-locale:u64:v1",
                    "bb",
                    SealProfile::LegacyV1,
                    "p2",
                ),
            ],
            closure("baseline-closure"),
        )
        .expect("genesis")
    }

    #[test]
    fn test_genesis_identity_is_deterministic_and_order_independent() {
        let a = StoreGeneration::genesis(
            vec![
                entry(
                    "libc:strlen:c-locale:u64:v1",
                    "bb",
                    SealProfile::LegacyV1,
                    "p2",
                ),
                entry(
                    "libc:toupper:c-locale:u8:v1",
                    "aa",
                    SealProfile::LegacyV1,
                    "p1",
                ),
            ],
            closure("baseline-closure"),
        )
        .unwrap();
        let b = genesis();
        assert_eq!(a.generation_id, b.generation_id);
        assert!(a.verify().is_ok());
    }

    #[test]
    fn test_publish_creates_a_child_and_replaces_by_target() {
        let g0 = genesis();
        let g1 = StoreGeneration::publish(
            &g0,
            vec![entry(
                "libc:toupper:c-locale:u8:v1",
                "cc",
                SealProfile::AutonomousV1,
                "autonomous-1",
            )],
            closure("closure-1"),
        )
        .expect("publish");
        assert_eq!(g1.parent.as_ref(), Some(&g0.generation_id));
        assert_eq!(g1.entries.len(), 2);
        assert_eq!(g1.artifact_for("libc:toupper:c-locale:u8:v1"), Some("cc"));
        assert_eq!(
            g1.artifact_for("libc:strlen:c-locale:u64:v1"),
            Some("bb"),
            "the other entry is carried forward"
        );
        assert_ne!(g0.generation_id, g1.generation_id);
    }

    #[test]
    fn test_verification_fails_closed_on_tampering() {
        let mut g = genesis();
        g.entries
            .push(entry("z", "zz", SealProfile::LegacyV1, "pz"));
        assert!(matches!(
            g.verify(),
            Err(GenerationError::EntrySubstitution) | Err(GenerationError::IdentityMismatch)
        ));
        let mut g = genesis();
        g.evidence_closure = EvidenceClosureId::new("");
        assert!(matches!(
            g.verify(),
            Err(GenerationError::ClosureMismatch) | Err(GenerationError::IdentityMismatch)
        ));
        let mut g = genesis();
        g.schema_version = 99;
        assert_eq!(g.verify(), Err(GenerationError::UnknownSchema(99)));
        let mut g = genesis();
        g.generation_id = StoreGenerationId::new("tampered");
        assert_eq!(g.verify(), Err(GenerationError::IdentityMismatch));
    }

    #[test]
    fn test_ledger_requires_a_chain_and_refuses_rollback() {
        let g0 = genesis();
        let g1 = StoreGeneration::publish(
            &g0,
            vec![entry(
                "libc:strspn:c-locale:u64:v1",
                "dd",
                SealProfile::AutonomousV1,
                "a1",
            )],
            closure("c1"),
        )
        .unwrap();
        let mut ledger = GenerationLedger::new();
        ledger.append(g0.clone()).unwrap();
        ledger.append(g1.clone()).unwrap();
        assert_eq!(ledger.head().unwrap().generation_id, g1.generation_id);

        // A generation whose parent is not the head is refused.
        let orphan = StoreGeneration::genesis(
            vec![entry("x", "x", SealProfile::LegacyV1, "x")],
            closure("x"),
        )
        .unwrap();
        let mut l2 = ledger.clone();
        assert!(l2.append(orphan).is_err());

        // Opening the genesis with a minimum of g1 is a rollback: refused.
        assert_eq!(
            ledger.open(&g0.generation_id, Some(&g1.generation_id)),
            Err(GenerationError::RollbackRefused)
        );
        // Opening g1 with a minimum of g1 is fine.
        assert!(ledger
            .open(&g1.generation_id, Some(&g1.generation_id))
            .is_ok());
    }

    #[test]
    fn test_binding_refuses_an_artifact_substitution() {
        let g0 = genesis();
        let b = GenerationBinding::bind(&g0).unwrap();
        assert!(b.assert_serves("libc:toupper:c-locale:u8:v1", "aa").is_ok());
        assert_eq!(
            b.assert_serves("libc:toupper:c-locale:u8:v1", "zz"),
            Err(GenerationError::ArtifactNotInGeneration)
        );
        assert_eq!(
            b.assert_serves("libc:absent:v1", "aa"),
            Err(GenerationError::ArtifactNotInGeneration)
        );
    }
}
