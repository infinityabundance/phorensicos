// porting/generation_session.rs — consuming an immutable store generation (§19)
//
// §19 gives a *publication* model: a newly sealed port is a new immutable
// generation, never a mutation of a running store. This module is the
// **consumption** side. It materializes the exact runtime index a generation
// represents, so a service bound to generation N can actually serve every
// artifact generation N publishes — not merely guard the artifacts the legacy
// index happened to already contain.
//
// The invariant established here is **equality**, not containment:
//
//     Artifacts(dispatcher) == Artifacts(generation)
//
// The committed baseline index supplies the object paths and metadata of the
// ports it already knows; a committed **autonomous artifact registry** supplies
// the locator for a port a later autonomous campaign published. Every artifact is
// verified against the generation's recorded hash, and every object's bytes
// against that hash, *before* the service is opened — so a substituted, missing or
// extra entry fails before the first call.
//
// This path needs no compiler, oracle, FRF, FRF-Fuzz, Gemel, agent or network: it
// reads committed JSON and committed object bytes and maps the objects. Emission
// of the registry itself (`emit_autonomous_artifact`) is an evidence-generation
// helper and is not part of the runtime path.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::fs;
use std::path::{Path, PathBuf};

use crate::porting::candidate::candidate_behavior_hash;
use crate::porting::dialect_cage::observe_target;
use crate::porting::json::Json;
use crate::porting::oracle_trace::combined_oracle_hash;
use crate::porting::promotion::TrustState;
use crate::porting::service::{run_plan, SealedNativeService, SessionCall, SessionMismatch};
use crate::porting::store_generation::{genesis_from_index, GenerationBinding, StoreGeneration};
use crate::porting::target::{cases_for, resolve_target};
use crate::porting::{
    sha256_hex, PortingAuthority, SealedArtifact, SealedPortEntry, SealedPortIndex,
};

/// The identity domain tag for a materialized runtime index.
pub const GENERATION_INDEX_DOMAIN: &[u8] = b"PHOR/GENERATION-INDEX/v1\0";

/// The committed registry of autonomously published leaf artifacts.
pub const ARTIFACT_REGISTRY_SCHEMA: &str = "phorensic.porting.autonomous_artifacts.v1";
/// The generation-session evidence schema.
pub const GENERATION_SESSION_SCHEMA: &str = "phorensic.porting.generation_session.v1";
/// The generation-session target.
pub const GENERATION_SESSION_TARGET: &str = "phor:generation-session:v1";

/// The committed generation the reference generation-session consumes.
pub const DEFAULT_GENERATION_PATH: &str =
    "phost/evidence/phorport/autonomy/libc-strspn-c-locale-u64-v1/store_generation.json";
/// The committed registry of autonomous artifacts.
pub const DEFAULT_ARTIFACT_REGISTRY_PATH: &str = "phost/evidence/autonomous/artifacts.json";

/// Why a generation session could not be materialized or run. Every variant fails
/// closed: the service is never opened on a partial or mismatched index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationSessionError {
    /// An I/O failure reading committed evidence.
    Io(String),
    /// A malformed record.
    Malformed(String),
    /// The generation failed to parse or verify.
    Generation(String),
    /// The registry failed to parse.
    Registry(String),
    /// The generation's parent is not the genesis derived from the baseline index.
    ParentMismatch,
    /// The generation dropped a baseline entry (lineage is not preserved).
    BaselineEntryDropped(String),
    /// A generation entry has no artifact locator.
    MissingArtifact(String),
    /// The materialized index contains a port the generation does not bind.
    ExtraArtifact(String),
    /// The artifact recorded for a target does not match the generation's.
    ArtifactSubstitution {
        target: String,
        expected: String,
        found: String,
    },
    /// The locator names an artifact kind this materializer does not support.
    UnsupportedKind { target: String, kind: String },
    /// The committed object bytes are absent.
    ObjectMissing(String),
    /// The committed object's bytes do not hash to the generation's artifact.
    ObjectHashMismatch {
        path: String,
        expected: String,
        found: String,
    },
}

impl GenerationSessionError {
    pub fn as_str(&self) -> &'static str {
        match self {
            GenerationSessionError::Io(_) => "generation-session I/O failure",
            GenerationSessionError::Malformed(_) => "generation-session malformed record",
            GenerationSessionError::Generation(_) => "store generation failed verification",
            GenerationSessionError::Registry(_) => "artifact registry failed to parse",
            GenerationSessionError::ParentMismatch => {
                "the generation's parent is not the baseline genesis"
            }
            GenerationSessionError::BaselineEntryDropped(_) => {
                "the generation dropped a baseline entry"
            }
            GenerationSessionError::MissingArtifact(_) => {
                "a generation entry has no committed artifact"
            }
            GenerationSessionError::ExtraArtifact(_) => {
                "the materialized index has a port the generation does not bind"
            }
            GenerationSessionError::ArtifactSubstitution { .. } => {
                "an artifact was substituted for the one the generation binds"
            }
            GenerationSessionError::UnsupportedKind { .. } => {
                "unsupported artifact kind in a generation entry"
            }
            GenerationSessionError::ObjectMissing(_) => "a committed object is missing",
            GenerationSessionError::ObjectHashMismatch { .. } => {
                "a committed object does not hash to its bound artifact"
            }
        }
    }
}

impl core::fmt::Display for GenerationSessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GenerationSessionError::Io(m)
            | GenerationSessionError::Malformed(m)
            | GenerationSessionError::Generation(m)
            | GenerationSessionError::Registry(m)
            | GenerationSessionError::BaselineEntryDropped(m)
            | GenerationSessionError::MissingArtifact(m)
            | GenerationSessionError::ExtraArtifact(m)
            | GenerationSessionError::ObjectMissing(m) => write!(f, "{}: {}", self.as_str(), m),
            GenerationSessionError::ArtifactSubstitution {
                target,
                expected,
                found,
            } => write!(
                f,
                "{}: {} (expected {}, found {})",
                self.as_str(),
                target,
                expected,
                found
            ),
            GenerationSessionError::ObjectHashMismatch {
                path,
                expected,
                found,
            } => write!(
                f,
                "{}: {} (expected {}, found {})",
                self.as_str(),
                path,
                expected,
                found
            ),
            GenerationSessionError::UnsupportedKind { target, kind } => {
                write!(f, "{}: {} ({})", self.as_str(), target, kind)
            }
            GenerationSessionError::ParentMismatch => write!(f, "{}", self.as_str()),
        }
    }
}

/// The committed locator for one autonomously published leaf artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutonomousArtifact {
    pub target: String,
    /// `"leaf-object"` (the only kind this materializer supports today).
    pub kind: String,
    /// Repo-relative path to the committed object.
    pub object_path: String,
    pub object_hash: String,
    pub oracle_hash: String,
    pub candidate_behavior_hash: String,
    pub candidate_source_hash: String,
    /// Repo-relative path to the committed autonomous promotion receipt.
    pub sealed_package: String,
}

fn req(v: &Json, key: &str) -> Result<String, GenerationSessionError> {
    v.str_at(key)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| GenerationSessionError::Malformed(format!("missing {}", key)))
}

impl AutonomousArtifact {
    pub fn from_json(v: &Json) -> Result<Self, GenerationSessionError> {
        Ok(AutonomousArtifact {
            target: req(v, "target")?,
            kind: req(v, "kind")?,
            object_path: req(v, "object_path")?,
            object_hash: req(v, "object_hash")?,
            oracle_hash: req(v, "oracle_hash")?,
            candidate_behavior_hash: req(v, "candidate_behavior_hash")?,
            candidate_source_hash: req(v, "candidate_source_hash")?,
            sealed_package: req(v, "sealed_package")?,
        })
    }

    pub fn to_json(&self) -> String {
        format!(
            "    {{\n      \"target\": \"{}\",\n      \"kind\": \"{}\",\n      \"object_path\": \"{}\",\n      \"object_hash\": \"{}\",\n      \"oracle_hash\": \"{}\",\n      \"candidate_behavior_hash\": \"{}\",\n      \"candidate_source_hash\": \"{}\",\n      \"sealed_package\": \"{}\"\n    }}",
            crate::porting::json_escape(&self.target),
            crate::porting::json_escape(&self.kind),
            crate::porting::json_escape(&self.object_path),
            self.object_hash,
            self.oracle_hash,
            self.candidate_behavior_hash,
            self.candidate_source_hash,
            crate::porting::json_escape(&self.sealed_package)
        )
    }
}

/// The committed registry of autonomously published artifacts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtifactRegistry {
    pub artifacts: Vec<AutonomousArtifact>,
}

impl ArtifactRegistry {
    pub fn parse(text: &str) -> Result<Self, GenerationSessionError> {
        let doc = Json::parse(text).map_err(|e| GenerationSessionError::Registry(e.to_string()))?;
        if doc.str_at("schema") != Some(ARTIFACT_REGISTRY_SCHEMA) {
            return Err(GenerationSessionError::Registry(format!(
                "schema is not {}",
                ARTIFACT_REGISTRY_SCHEMA
            )));
        }
        let arr = doc
            .get("artifacts")
            .and_then(|v| v.as_arr())
            .ok_or_else(|| GenerationSessionError::Registry(String::from("missing artifacts")))?;
        let mut artifacts: Vec<AutonomousArtifact> = Vec::with_capacity(arr.len());
        for a in arr {
            artifacts.push(AutonomousArtifact::from_json(a)?);
        }
        Ok(ArtifactRegistry { artifacts })
    }

    pub fn get(&self, target: &str) -> Option<&AutonomousArtifact> {
        self.artifacts.iter().find(|a| a.target == target)
    }

    pub fn to_json(&self) -> String {
        let body: Vec<String> = self.artifacts.iter().map(|a| a.to_json()).collect();
        format!(
            "{{\n  \"schema\": \"{}\",\n  \"artifacts\": [\n{}\n  ]\n}}\n",
            ARTIFACT_REGISTRY_SCHEMA,
            body.join(",\n")
        )
    }
}

/// Derive the committed locator for an autonomously published leaf from committed
/// evidence. This observation is evidence generation, not runtime: it observes the
/// declared corpus through the dialect cage and reads the compiled object.
pub fn emit_autonomous_artifact(
    target_id: &str,
    object_path: &str,
    sealed_package: &str,
    base: &Path,
) -> Result<AutonomousArtifact, GenerationSessionError> {
    let target = resolve_target(target_id)
        .ok_or_else(|| GenerationSessionError::Malformed(format!("unknown target {target_id}")))?;
    if target.id != target_id {
        return Err(GenerationSessionError::Malformed(format!(
            "{target_id} resolved to the partial match {}; use the full target id",
            target.id
        )));
    }
    let auth = PortingAuthority::granted();
    let cases = cases_for(&target);
    let traces = observe_target(&target, &cases, &auth)
        .map_err(|e| GenerationSessionError::Malformed(format!("observe: {e}")))?;
    let oracle_hash = combined_oracle_hash(&traces);
    let behavior = candidate_behavior_hash(&traces);
    let object_bytes = fs::read(base.join(object_path))
        .map_err(|_| GenerationSessionError::ObjectMissing(object_path.to_string()))?;
    let object_hash = sha256_hex(&object_bytes);
    let src_bytes = fs::read(base.join(target.candidate_source))
        .map_err(|_| GenerationSessionError::ObjectMissing(target.candidate_source.to_string()))?;
    let candidate_source_hash = sha256_hex(&src_bytes);
    Ok(AutonomousArtifact {
        target: target.id.to_string(),
        kind: String::from("leaf-object"),
        object_path: object_path.to_string(),
        object_hash,
        oracle_hash,
        candidate_behavior_hash: behavior,
        candidate_source_hash,
        sealed_package: sealed_package.to_string(),
    })
}

/// The identity of a materialized runtime index: the sorted `target:kind:artifact`
/// triples, domain-separated and length-free (the delimiter is unique in an id).
pub fn materialized_index_id(index: &SealedPortIndex) -> String {
    let mut parts: Vec<String> = index
        .entries()
        .iter()
        .map(|e| format!("{}:{}:{}", e.target, e.artifact_kind(), e.artifact_hash()))
        .collect();
    parts.sort();
    let mut pre = Vec::with_capacity(GENERATION_INDEX_DOMAIN.len() + 64 * parts.len());
    pre.extend_from_slice(GENERATION_INDEX_DOMAIN);
    pre.extend_from_slice(parts.join("|").as_bytes());
    sha256_hex(&pre)
}

/// Materialize the exact runtime index a generation represents.
///
/// Every entry is resolved against the baseline index first (carrying its object
/// path and metadata) and otherwise against the committed autonomous registry.
/// Every artifact is checked against the generation's recorded hash, and each new
/// object's bytes against that hash. The result is checked for **exact** equality
/// with the generation, and every baseline entry must be carried forward — so a
/// dropped, substituted, missing or extra port fails closed.
pub fn materialize_index(
    baseline: &SealedPortIndex,
    generation: &StoreGeneration,
    registry: &ArtifactRegistry,
    base: &Path,
) -> Result<SealedPortIndex, GenerationSessionError> {
    let mut index = SealedPortIndex::new();

    for g in &generation.entries {
        match baseline.lookup(&g.target) {
            Some(entry) => {
                if entry.artifact_hash() != g.artifact_hash {
                    return Err(GenerationSessionError::ArtifactSubstitution {
                        target: g.target.clone(),
                        expected: g.artifact_hash.clone(),
                        found: entry.artifact_hash().to_string(),
                    });
                }
                index.insert(entry.clone());
            }
            None => {
                let loc = registry
                    .get(&g.target)
                    .ok_or_else(|| GenerationSessionError::MissingArtifact(g.target.clone()))?;
                if loc.kind != "leaf-object" {
                    return Err(GenerationSessionError::UnsupportedKind {
                        target: g.target.clone(),
                        kind: loc.kind.clone(),
                    });
                }
                if loc.object_hash != g.artifact_hash {
                    return Err(GenerationSessionError::ArtifactSubstitution {
                        target: g.target.clone(),
                        expected: g.artifact_hash.clone(),
                        found: loc.object_hash.clone(),
                    });
                }
                let abs = base.join(&loc.object_path);
                let bytes = fs::read(&abs)
                    .map_err(|_| GenerationSessionError::ObjectMissing(loc.object_path.clone()))?;
                let found = sha256_hex(&bytes);
                if found != g.artifact_hash {
                    return Err(GenerationSessionError::ObjectHashMismatch {
                        path: loc.object_path.clone(),
                        expected: g.artifact_hash.clone(),
                        found,
                    });
                }
                index.insert(SealedPortEntry {
                    target: g.target.clone(),
                    trust: TrustState::Sealed,
                    artifact: SealedArtifact::leaf_object(
                        g.artifact_hash.clone(),
                        abs.display().to_string(),
                    ),
                    oracle_hash: loc.oracle_hash.clone(),
                    candidate_behavior_hash: loc.candidate_behavior_hash.clone(),
                    candidate_source_hash: loc.candidate_source_hash.clone(),
                    sealed_package: loc.sealed_package.clone(),
                });
            }
        }
    }

    // Lineage: a generation descends from the baseline, so it must carry every
    // baseline entry. A generation that dropped one is not a descendant of it.
    for b in baseline.entries() {
        if generation.entries.iter().all(|g| g.target != b.target) {
            return Err(GenerationSessionError::BaselineEntryDropped(
                b.target.clone(),
            ));
        }
    }

    verify_exact(&index, generation)?;
    Ok(index)
}

/// Assert `Artifacts(index) == Artifacts(generation)` exactly.
pub fn verify_exact(
    index: &SealedPortIndex,
    generation: &StoreGeneration,
) -> Result<(), GenerationSessionError> {
    if index.len() != generation.entries.len() {
        return Err(GenerationSessionError::ExtraArtifact(format!(
            "materialized {} entries, generation binds {}",
            index.len(),
            generation.entries.len()
        )));
    }
    for e in index.entries() {
        match generation.entries.iter().find(|g| g.target == e.target) {
            Some(g) if g.artifact_hash == e.artifact_hash() => {}
            Some(g) => {
                return Err(GenerationSessionError::ArtifactSubstitution {
                    target: e.target.clone(),
                    expected: g.artifact_hash.clone(),
                    found: e.artifact_hash().to_string(),
                })
            }
            None => return Err(GenerationSessionError::ExtraArtifact(e.target.clone())),
        }
    }
    for g in &generation.entries {
        if index.lookup(&g.target).is_none() {
            return Err(GenerationSessionError::MissingArtifact(g.target.clone()));
        }
    }
    Ok(())
}

/// The generation-session plan: every port the baseline session serves, plus the
/// corrected successor — so the lineage does not merely *remember* both
/// identities, the runtime distinguishes and serves both.
pub fn generation_session_plan() -> Vec<SessionCall> {
    let mut plan = crate::porting::service::session_plan();
    plan.push(SessionCall {
        label: "strspn_corrected(\"abc\\0\",\"ab\",4)",
        port: crate::porting::target::LIBC_STRSPN.id,
        args: alloc::vec![
            alloc::vec![0x61, 0x62, 0x63, 0x00],
            alloc::vec![0x61, 0x62],
            (4u64).to_le_bytes().to_vec()
        ],
        expect_hex: "0200000000000000",
    });
    plan
}

/// The generation-session residual: the exact generation's ports, served from one
/// verified load under a generation binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationSessionVerdict {
    pub generation_id: String,
    pub parent_generation_id: String,
    /// The identity of the exact runtime index the generation materialized.
    pub materialized_index_hash: String,
    /// Ports the generation binds (== calls when every port is served once).
    pub ports_available: usize,
    pub store_loads: u64,
    pub calls: u64,
    pub native_calls: u64,
    pub fallback_calls: u64,
    pub broken_seal_calls: u64,
    pub objects_mapped: usize,
    pub dispatches: u64,
    pub per_port: BTreeMap<String, u64>,
    /// `port -> object_hash`: the exact sealed artifact each native call was served.
    pub served: BTreeMap<String, String>,
    pub session_hash: String,
    pub mismatches: Vec<SessionMismatch>,
}

impl GenerationSessionVerdict {
    pub fn target(&self) -> &'static str {
        GENERATION_SESSION_TARGET
    }

    /// Consistent only when every bound port was served natively, once, with the
    /// recorded expectation.
    pub fn is_consistent(&self) -> bool {
        self.calls > 0
            && self.calls == self.native_calls
            && self.fallback_calls == 0
            && self.broken_seal_calls == 0
            && self.mismatches.is_empty()
            && self.calls == self.ports_available as u64
    }

    pub fn verdict_str(&self) -> &'static str {
        if self.is_consistent() {
            "consistent"
        } else {
            "inconsistent"
        }
    }

    pub fn canonical(&self) -> String {
        let per_port: Vec<String> = self
            .per_port
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        let served: Vec<String> = self
            .served
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        format!(
            "target={};generation={};parent={};materialized_index={};ports_available={};store_loads={};calls={};native_calls={};fallback_calls={};broken_seal_calls={};objects_mapped={};dispatches={};per_port={};served={};session_hash={};verdict={}",
            self.target(),
            self.generation_id,
            self.parent_generation_id,
            self.materialized_index_hash,
            self.ports_available,
            self.store_loads,
            self.calls,
            self.native_calls,
            self.fallback_calls,
            self.broken_seal_calls,
            self.objects_mapped,
            self.dispatches,
            per_port.join(","),
            served.join(","),
            self.session_hash,
            self.verdict_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        let per_port: Vec<String> = self
            .per_port
            .iter()
            .map(|(k, v)| format!("\n      \"{}\": {}", crate::porting::json_escape(k), v))
            .collect();
        let served: Vec<String> = self
            .served
            .iter()
            .map(|(k, v)| format!("\n      \"{}\": \"{}\"", crate::porting::json_escape(k), v))
            .collect();
        let body: Vec<String> = self
            .mismatches
            .iter()
            .map(|m| {
                format!(
                    "    {{\n      \"label\": \"{}\",\n      \"port\": \"{}\",\n      \"expected_hex\": \"{}\",\n      \"actual_hex\": \"{}\",\n      \"reason\": \"{}\"\n    }}",
                    crate::porting::json_escape(&m.label),
                    crate::porting::json_escape(&m.port),
                    crate::porting::json_escape(&m.expected_hex),
                    crate::porting::json_escape(&m.actual_hex),
                    crate::porting::json_escape(&m.reason)
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"{}\",\n  \"target\": \"{}\",\n  \"generation\": \"{}\",\n  \"parent\": \"{}\",\n  \"materialized_index_hash\": \"{}\",\n  \"ports_available\": {},\n  \"store_loads\": {},\n  \"calls\": {},\n  \"native_calls\": {},\n  \"fallback_calls\": {},\n  \"broken_seal_calls\": {},\n  \"objects_mapped\": {},\n  \"dispatches\": {},\n  \"per_port\": {{{}\n  }},\n  \"served\": {{{}\n  }},\n  \"session_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            GENERATION_SESSION_SCHEMA,
            self.target(),
            self.generation_id,
            self.parent_generation_id,
            self.materialized_index_hash,
            self.ports_available,
            self.store_loads,
            self.calls,
            self.native_calls,
            self.fallback_calls,
            self.broken_seal_calls,
            self.objects_mapped,
            self.dispatches,
            per_port.join(","),
            served.join(","),
            self.session_hash,
            self.verdict_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// Materialize the generation and run the generation-session plan through one
/// bound service. Nothing is read but committed evidence and committed objects.
pub fn run_generation_session(
    generation_path: &str,
    registry_path: &str,
    store_path: &str,
    auth: &PortingAuthority,
) -> Result<(GenerationSessionVerdict, SealedNativeService), GenerationSessionError> {
    if !auth.can_observe() {
        return Err(GenerationSessionError::Malformed(String::from(
            "capability denied",
        )));
    }
    let base = crate::porting::compiled::workspace_root();

    let (_doc, baseline) = crate::porting::store::load_with_document(store_path)
        .map_err(|e| GenerationSessionError::Generation(e.to_string()))?;

    let gen_abs = resolve(&base, generation_path);
    let gen_text = fs::read_to_string(&gen_abs)
        .map_err(|e| GenerationSessionError::Io(format!("{}: {}", gen_abs.display(), e)))?;
    let generation = StoreGeneration::parse(&gen_text)
        .map_err(|e| GenerationSessionError::Generation(e.as_str().to_string()))?;

    // Ancestry: the generation's parent must be the genesis of the committed
    // baseline index. A generation descending from a different baseline is refused.
    let genesis = genesis_from_index(&baseline)
        .map_err(|e| GenerationSessionError::Generation(e.as_str().to_string()))?;
    if generation.parent.as_ref() != Some(&genesis.generation_id) {
        return Err(GenerationSessionError::ParentMismatch);
    }

    let reg_abs = resolve(&base, registry_path);
    let reg_text = fs::read_to_string(&reg_abs)
        .map_err(|e| GenerationSessionError::Io(format!("{}: {}", reg_abs.display(), e)))?;
    let registry = ArtifactRegistry::parse(&reg_text)?;

    let index = materialize_index(&baseline, &generation, &registry, &base)?;
    let mat_id = materialized_index_id(&index);

    let binding = GenerationBinding::bind(&generation)
        .map_err(|e| GenerationSessionError::Generation(e.as_str().to_string()))?;
    let mut service = SealedNativeService::from_materialized(store_path, index, binding);

    let mismatches = run_plan(&mut service, &generation_session_plan(), auth);

    let verdict = GenerationSessionVerdict {
        generation_id: generation.generation_id.as_str().to_string(),
        parent_generation_id: generation
            .parent
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default(),
        materialized_index_hash: mat_id,
        ports_available: generation.entries.len(),
        store_loads: crate::porting::service::STORE_LOADS_PER_SERVICE,
        calls: service.calls(),
        native_calls: service.native_calls(),
        fallback_calls: service.fallback_calls(),
        broken_seal_calls: service.broken_seal_calls(),
        objects_mapped: service.objects_mapped(),
        dispatches: service.dispatches(),
        per_port: service.per_port_dispatches().clone(),
        served: service.served_artifacts().clone(),
        session_hash: service.session_hash(),
        mismatches,
    };
    Ok((verdict, service))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::autonomous_seal::SealProfile;
    use crate::porting::dispatch::DispatchError;
    use crate::porting::ident::EvidenceClosureId;
    use crate::porting::store_generation::GenerationEntry;
    use crate::porting::target::{LIBC_STRSPN, POSIX_STRSPN};

    const SUCC_OBJECT: &str = "c40c4e3a5100257b04f7df0b593e961bb0acc4ad2658e1604fda068fc7dd54e2";
    const HIST_OBJECT: &str = "c93271d069fd998e6de8c2cd07e665bd098f6fb64072cdd98fb44ae1375dbafc";
    const SUCC_SOURCE: &str = "libc:strspn:c-locale:u64:v1";
    const HIST_SOURCE: &str = "posix:strspn:c-locale:u64:v1";

    fn auth() -> PortingAuthority {
        PortingAuthority::granted()
    }

    fn base() -> PathBuf {
        crate::porting::compiled::workspace_root()
    }

    fn strspn_args() -> Vec<Vec<u8>> {
        alloc::vec![
            alloc::vec![0x61, 0x62, 0x63, 0x00],
            alloc::vec![0x61, 0x62],
            (4u64).to_le_bytes().to_vec()
        ]
    }

    fn committed_baseline() -> SealedPortIndex {
        crate::porting::store::load(crate::porting::store::STORE_PATH).expect("committed store")
    }

    fn committed_generation() -> StoreGeneration {
        let text = fs::read_to_string(base().join(DEFAULT_GENERATION_PATH)).expect("generation");
        StoreGeneration::parse(&text).expect("parse generation")
    }

    fn committed_registry() -> ArtifactRegistry {
        let text =
            fs::read_to_string(base().join(DEFAULT_ARTIFACT_REGISTRY_PATH)).expect("registry");
        ArtifactRegistry::parse(&text).expect("parse registry")
    }

    fn replace_artifact(
        index: &SealedPortIndex,
        target: &str,
        hash: &str,
        path: &str,
    ) -> SealedPortIndex {
        let mut out = SealedPortIndex::new();
        for e in index.entries() {
            if e.target == target {
                out.insert(SealedPortEntry {
                    target: e.target.clone(),
                    trust: e.trust.clone(),
                    artifact: SealedArtifact::leaf_object(hash.to_string(), path.to_string()),
                    oracle_hash: e.oracle_hash.clone(),
                    candidate_behavior_hash: e.candidate_behavior_hash.clone(),
                    candidate_source_hash: e.candidate_source_hash.clone(),
                    sealed_package: e.sealed_package.clone(),
                });
            } else {
                out.insert(e.clone());
            }
        }
        out
    }

    #[test]
    fn test_materialized_index_equals_the_generation_exactly() {
        let index = materialize_index(
            &committed_baseline(),
            &committed_generation(),
            &committed_registry(),
            &base(),
        )
        .expect("materialize");
        assert_eq!(index.len(), 14);
        verify_exact(&index, &committed_generation()).expect("exact");
        assert_eq!(materialized_index_id(&index).len(), 64);
    }

    #[test]
    fn test_generation_session_serves_every_bound_port_and_distinguishes_both_strspn() {
        let (v, _svc) = run_generation_session(
            DEFAULT_GENERATION_PATH,
            DEFAULT_ARTIFACT_REGISTRY_PATH,
            crate::porting::store::STORE_PATH,
            &auth(),
        )
        .expect("generation session");
        assert!(v.is_consistent(), "{v:?}");
        assert_eq!(v.ports_available, 14);
        assert_eq!(v.calls, 14);
        assert_eq!(v.native_calls, 14);
        assert_eq!(v.fallback_calls, 0);
        assert_eq!(v.broken_seal_calls, 0);
        assert_eq!(v.objects_mapped, 7);
        assert_eq!(
            v.generation_id,
            "8e6fafec7b2b66ced51d8699d86fc264983fe2cd501c11b9c316f4d125912ba5"
        );
        assert_eq!(
            v.parent_generation_id,
            "f47d1bec90d8bba92f441138fc29db77094c9a35420057a5d60c3ee4163bdf73"
        );
        // The same bounded behavior, two distinct identities, two distinct
        // artifacts, served by their own port id in one generation.
        assert_eq!(
            v.served.get(HIST_SOURCE).map(String::as_str),
            Some(HIST_OBJECT)
        );
        assert_eq!(
            v.served.get(SUCC_SOURCE).map(String::as_str),
            Some(SUCC_OBJECT)
        );
        // Every bound port was served *its own* artifact.
        let gen = committed_generation();
        for (port, served) in &v.served {
            let bound = gen.artifact_for(port).expect("bound port");
            assert_eq!(served, bound, "{port}");
        }
    }

    #[test]
    fn test_a_g0_bound_session_refuses_the_successor_but_serves_the_historical() {
        let baseline = committed_baseline();
        let g0 = genesis_from_index(&baseline).expect("genesis");
        let binding = GenerationBinding::bind(&g0).expect("bind");
        let mut svc = SealedNativeService::from_materialized(
            crate::porting::store::STORE_PATH,
            baseline,
            binding,
        );
        let err = svc.call(LIBC_STRSPN.id, &strspn_args(), &auth());
        assert!(matches!(err, Err(DispatchError::SealBroken(_))), "{err:?}");
        let ok = svc.call(POSIX_STRSPN.id, &strspn_args(), &auth());
        assert!(ok.map(|o| o.source.is_native()).unwrap_or(false));
    }

    #[test]
    fn test_serving_the_historical_port_with_the_successor_object_is_seal_broken() {
        let base = base();
        let gen = committed_generation();
        let index = materialize_index(&committed_baseline(), &gen, &committed_registry(), &base)
            .expect("materialize");
        let succ_path = base
            .join("phost/evidence/autonomous/libc-strspn/candidate.o")
            .display()
            .to_string();
        let tampered = replace_artifact(&index, POSIX_STRSPN.id, SUCC_OBJECT, &succ_path);
        let binding = GenerationBinding::bind(&gen).expect("bind");
        let mut svc = SealedNativeService::from_materialized(
            crate::porting::store::STORE_PATH,
            tampered,
            binding,
        );
        let err = svc.call(POSIX_STRSPN.id, &strspn_args(), &auth());
        assert!(matches!(err, Err(DispatchError::SealBroken(_))), "{err:?}");
    }

    #[test]
    fn test_serving_the_successor_port_with_the_historical_object_is_seal_broken() {
        let base = base();
        let gen = committed_generation();
        let index = materialize_index(&committed_baseline(), &gen, &committed_registry(), &base)
            .expect("materialize");
        let hist_path = base
            .join("phost/evidence/porting/strspn/candidate.o")
            .display()
            .to_string();
        let tampered = replace_artifact(&index, LIBC_STRSPN.id, HIST_OBJECT, &hist_path);
        let binding = GenerationBinding::bind(&gen).expect("bind");
        let mut svc = SealedNativeService::from_materialized(
            crate::porting::store::STORE_PATH,
            tampered,
            binding,
        );
        let err = svc.call(LIBC_STRSPN.id, &strspn_args(), &auth());
        assert!(matches!(err, Err(DispatchError::SealBroken(_))), "{err:?}");
    }

    #[test]
    fn test_a_substituted_successor_hash_fails_before_the_session_starts() {
        let base = base();
        let gen = committed_generation();
        let mut entries = gen.entries.clone();
        for e in entries.iter_mut() {
            if e.target == LIBC_STRSPN.id {
                e.artifact_hash = HIST_OBJECT.to_string();
            }
        }
        let tampered = StoreGeneration::genesis(entries, gen.evidence_closure.clone()).unwrap();
        let err = materialize_index(
            &committed_baseline(),
            &tampered,
            &committed_registry(),
            &base,
        );
        assert!(
            matches!(
                err,
                Err(GenerationSessionError::ArtifactSubstitution { .. })
            ),
            "{err:?}"
        );
    }

    #[test]
    fn test_a_generation_that_drops_the_historical_entry_fails_materialization() {
        let base = base();
        let gen = committed_generation();
        let entries: Vec<GenerationEntry> = gen
            .entries
            .iter()
            .filter(|e| e.target != POSIX_STRSPN.id)
            .cloned()
            .collect();
        let reduced = StoreGeneration::genesis(entries, gen.evidence_closure.clone()).unwrap();
        let err = materialize_index(
            &committed_baseline(),
            &reduced,
            &committed_registry(),
            &base,
        );
        assert!(
            matches!(err, Err(GenerationSessionError::BaselineEntryDropped(_))),
            "{err:?}"
        );
    }

    #[test]
    fn test_a_materialized_index_with_an_extra_unbound_port_fails_exact_equality() {
        let base = base();
        let gen = committed_generation();
        let mut index =
            materialize_index(&committed_baseline(), &gen, &committed_registry(), &base)
                .expect("materialize");
        index.insert(SealedPortEntry {
            target: String::from("libc:bogus:c-locale:u8:v1"),
            trust: TrustState::Sealed,
            artifact: SealedArtifact::leaf_object("a".repeat(64), String::from("/nonexistent")),
            oracle_hash: "b".repeat(64),
            candidate_behavior_hash: "c".repeat(64),
            candidate_source_hash: "d".repeat(64),
            sealed_package: String::from("p"),
        });
        let err = verify_exact(&index, &gen);
        assert!(
            matches!(err, Err(GenerationSessionError::ExtraArtifact(_))),
            "{err:?}"
        );
    }

    #[test]
    fn test_a_generation_with_a_foreign_parent_is_refused() {
        let other = StoreGeneration::genesis(
            alloc::vec![GenerationEntry::new(
                "libc:other:c-locale:u8:v1",
                "a".repeat(64),
                SealProfile::LegacyV1,
                "p",
            )],
            EvidenceClosureId::new("other"),
        )
        .unwrap();
        let g = StoreGeneration::publish(
            &other,
            alloc::vec![GenerationEntry::new(
                LIBC_STRSPN.id,
                SUCC_OBJECT,
                SealProfile::AutonomousV1,
                "e",
            )],
            EvidenceClosureId::new("c"),
        )
        .unwrap();
        let path =
            std::env::temp_dir().join(format!("phor_gen_parent_{}.json", std::process::id()));
        fs::write(&path, g.to_json()).unwrap();
        let err = run_generation_session(
            path.to_str().unwrap(),
            DEFAULT_ARTIFACT_REGISTRY_PATH,
            crate::porting::store::STORE_PATH,
            &auth(),
        )
        .err();
        let _ = fs::remove_file(&path);
        assert!(
            matches!(err, Some(GenerationSessionError::ParentMismatch)),
            "{err:?}"
        );
    }
}
