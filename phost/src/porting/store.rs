// porting/store.rs — the persistent sealed port store (std)
//
// The courts *derive* a seal: they observe foreign behavior, compile the
// clean-room candidate, replay it, and publish what survives. That derivation is
// expensive (it invokes `phorc` and re-runs nested composition courts), and it is
// not what a running system should do at a call site.
//
// This module turns the store into a **committed artifact**: a single verified
// index that names every sealed port — leaf objects *and* composition chain
// hashes — so the runtime can load the seal instead of re-deriving it. Loading
// does no compilation and no oracle replay; it reads the index, checks every
// entry, and refuses to return a partially-trusted store.
//
// What "verified" means here:
//   * the document matches the store schema and its residual hash;
//   * every entry is `sealed` and its target is unique;
//   * every leaf's `object_path` exists and its SHA-256 equals the recorded
//     `object_hash` (the seal is the *bytes* of the promoted object, not a claim);
//   * every composition's `leaves` are themselves present and sealed, so the
//     recursion bottoms out in verified objects;
//   * every composition's `chain_hash` is a 64-char lowercase hex digest.
//
// The store carries no ambient authority: reading it is a privileged act gated
// by the caller's `PORTING` capability (see `run_native_call` / dispatch).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::porting::compiled::workspace_root;
use crate::porting::json::Json;
use crate::porting::promotion::TrustState;
use crate::porting::{
    sha256_hex, CompositionKind, SealedArtifact, SealedPortEntry, SealedPortIndex,
};

/// The committed store index, repo-relative.
pub const STORE_PATH: &str = "phost/evidence/store/index.json";
/// The store document schema.
pub const STORE_SCHEMA: &str = "phorensic.porting.store.v1";

/// The leaf symbols the store covers, in the order they are emitted.
const LEAF_SYMBOLS: [&str; 5] = ["toupper", "memcmp", "memchr", "strlen", "strrchr"];

/// The compositions the store covers, in dependency order (a nested chain's inner
/// composition is emitted before the chain that consumes it).
const COMPOSITION_NAMES: [&str; 6] = [
    "toupper_memchr",
    "toupper_strlen_memchr",
    "toupper_strlen_memchr_pair",
    "toupper_each",
    "toupper_each_strlen_memchr",
    "toupper_memchr_suffix",
];

/// Every way the store can fail to verify. All of them are terminal: the loader
/// never returns a partially-trusted index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The index file (or a referenced evidence file) could not be read.
    Io(String),
    /// The document is not valid JSON.
    Parse(String),
    /// The document is not a store document of the expected schema/version.
    Schema(String),
    /// A field is missing, empty, or not in the expected form.
    Malformed(String),
    /// Two entries claim the same target.
    DuplicateTarget(String),
    /// An entry does not carry the `sealed` trust state.
    UnsealedEntry(String),
    /// A leaf's object file is absent — the seal cannot be checked.
    ObjectMissing(String),
    /// A leaf's object file does not hash to the recorded object hash.
    ObjectHashMismatch {
        path: String,
        expected: String,
        found: String,
    },
    /// A composition names a stage that is not itself in the store.
    DanglingLeaf { composition: String, leaf: String },
    /// The composition graph has a cycle: resolving it could never terminate.
    CompositionCycle(String),
    /// The document's recorded residual hash does not cover its entries.
    ResidualMismatch { expected: String, found: String },
    /// The committed evidence a generated entry is built from is missing or
    /// inconsistent (used by `document_from_evidence`).
    Evidence(String),
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::Io(m) => write!(f, "store I/O error: {}", m),
            StoreError::Parse(m) => write!(f, "store parse error: {}", m),
            StoreError::Schema(m) => write!(f, "store schema error: {}", m),
            StoreError::Malformed(m) => write!(f, "malformed store entry: {}", m),
            StoreError::DuplicateTarget(t) => write!(f, "duplicate store target: {}", t),
            StoreError::UnsealedEntry(t) => write!(f, "store entry is not sealed: {}", t),
            StoreError::ObjectMissing(p) => write!(f, "sealed object missing: {}", p),
            StoreError::ObjectHashMismatch {
                path,
                expected,
                found,
            } => write!(
                f,
                "sealed object hash mismatch for {}: expected {}, found {}",
                path, expected, found
            ),
            StoreError::DanglingLeaf { composition, leaf } => write!(
                f,
                "composition {} names a stage that is not in the store: {}",
                composition, leaf
            ),
            StoreError::CompositionCycle(path) => {
                write!(f, "composition cycle in the store: {}", path)
            }
            StoreError::ResidualMismatch { expected, found } => write!(
                f,
                "store residual hash mismatch: recorded {}, computed {}",
                expected, found
            ),
            StoreError::Evidence(m) => write!(f, "porting evidence error: {}", m),
        }
    }
}

/// One store entry, in its portable (repo-relative) document form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreEntry {
    /// A relocation-free ELF64 object with an ABI entry point.
    Leaf {
        target: String,
        object_path: String,
        object_hash: String,
        oracle_hash: String,
        candidate_behavior_hash: String,
        candidate_source_hash: String,
        sealed_package: String,
    },
    /// A chain over other sealed ports; it has no object of its own.
    Composition {
        target: String,
        composition_id: String,
        chain_hash: String,
        leaves: Vec<String>,
        sealed_package: String,
    },
}

impl StoreEntry {
    pub fn target(&self) -> &str {
        match self {
            StoreEntry::Leaf { target, .. } => target,
            StoreEntry::Composition { target, .. } => target,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, StoreEntry::Leaf { .. })
    }

    /// The verified artifact hash this entry publishes: the object hash for a
    /// leaf, the chain hash for a composition.
    pub fn artifact_hash(&self) -> &str {
        match self {
            StoreEntry::Leaf { object_hash, .. } => object_hash,
            StoreEntry::Composition { chain_hash, .. } => chain_hash,
        }
    }

    /// The canonical line this entry contributes to the store residual hash.
    fn canonical(&self) -> String {
        match self {
            StoreEntry::Leaf {
                target,
                object_path,
                object_hash,
                oracle_hash,
                candidate_behavior_hash,
                candidate_source_hash,
                sealed_package,
            } => format!(
                "leaf;{};{};{};{};{};{};{}",
                target,
                object_path,
                object_hash,
                oracle_hash,
                candidate_behavior_hash,
                candidate_source_hash,
                sealed_package
            ),
            StoreEntry::Composition {
                target,
                composition_id,
                chain_hash,
                leaves,
                sealed_package,
            } => format!(
                "composition;{};{};{};{};{}",
                target,
                composition_id,
                chain_hash,
                leaves.join(","),
                sealed_package
            ),
        }
    }

    /// The entry as deterministic JSON (four-space body inside the entries array).
    fn to_json(&self) -> String {
        match self {
            StoreEntry::Leaf {
                target,
                object_path,
                object_hash,
                oracle_hash,
                candidate_behavior_hash,
                candidate_source_hash,
                sealed_package,
            } => format!(
                "    {{\n      \"target\": \"{}\",\n      \"kind\": \"leaf-object\",\n      \"trust\": \"sealed\",\n      \"object_path\": \"{}\",\n      \"object_hash\": \"{}\",\n      \"oracle_hash\": \"{}\",\n      \"candidate_behavior_hash\": \"{}\",\n      \"candidate_source_hash\": \"{}\",\n      \"sealed_package\": \"{}\"\n    }}",
                crate::porting::json_escape(target),
                crate::porting::json_escape(object_path),
                object_hash,
                oracle_hash,
                candidate_behavior_hash,
                candidate_source_hash,
                crate::porting::json_escape(sealed_package)
            ),
            StoreEntry::Composition {
                target,
                composition_id,
                chain_hash,
                leaves,
                sealed_package,
            } => {
                let leaves_json = leaves
                    .iter()
                    .map(|l| format!("\"{}\"", crate::porting::json_escape(l)))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!(
                    "    {{\n      \"target\": \"{}\",\n      \"kind\": \"composition\",\n      \"trust\": \"sealed\",\n      \"composition_id\": \"{}\",\n      \"chain_hash\": \"{}\",\n      \"leaves\": [{}],\n      \"sealed_package\": \"{}\"\n    }}",
                    crate::porting::json_escape(target),
                    crate::porting::json_escape(composition_id),
                    chain_hash,
                    leaves_json,
                    crate::porting::json_escape(sealed_package)
                )
            }
        }
    }
}

/// The whole store document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreDocument {
    pub entries: Vec<StoreEntry>,
}

impl StoreDocument {
    pub fn new(entries: Vec<StoreEntry>) -> Self {
        Self { entries }
    }

    /// SHA-256 (lowercase hex) over the canonical entry list — the store's own
    /// residual. Reordering or editing any entry changes it.
    pub fn residual_hash(&self) -> String {
        let canonical = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| format!("{}|{}", i, e.canonical()))
            .collect::<Vec<String>>()
            .join("\n");
        sha256_hex(canonical.as_bytes())
    }

    /// The document as deterministic JSON. Field order is stable, entries keep
    /// their order, and the residual hash covers the entries.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n  \"schema\": \"");
        out.push_str(STORE_SCHEMA);
        out.push_str("\",\n  \"entry_count\": ");
        out.push_str(&self.entries.len().to_string());
        out.push_str(",\n  \"asserted\": \"every sealed port (leaf object + composition chain) loads from committed evidence without invoking the compiler or replaying an oracle\",\n  \"entries\": [\n");
        for (i, entry) in self.entries.iter().enumerate() {
            out.push_str(&entry.to_json());
            if i + 1 < self.entries.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ],\n  \"residual_hash\": \"");
        out.push_str(&self.residual_hash());
        out.push_str("\"\n}\n");
        out
    }

    /// Parse and structurally validate a store document (no filesystem access).
    pub fn parse(text: &str) -> Result<StoreDocument, StoreError> {
        let doc = Json::parse(text).map_err(StoreError::Parse)?;
        match doc.str_at("schema") {
            Some(STORE_SCHEMA) => {}
            Some(other) => {
                return Err(StoreError::Schema(format!(
                    "expected {}, found {}",
                    STORE_SCHEMA, other
                )))
            }
            None => return Err(StoreError::Schema(String::from("no schema field"))),
        }

        let raw = doc
            .get("entries")
            .and_then(|v| v.as_arr())
            .ok_or_else(|| StoreError::Malformed(String::from("no entries array")))?;

        let mut entries = Vec::new();
        for (i, e) in raw.iter().enumerate() {
            entries.push(parse_entry(e, i)?);
        }
        if entries.is_empty() {
            return Err(StoreError::Malformed(String::from("store has no entries")));
        }

        if let Some(count) = doc.get("entry_count").and_then(|v| v.as_i64()) {
            if count as usize != entries.len() {
                return Err(StoreError::Malformed(format!(
                    "entry_count {} does not match {} entries",
                    count,
                    entries.len()
                )));
            }
        }

        let document = StoreDocument { entries };

        // The recorded residual must cover exactly these entries.
        if let Some(recorded) = doc.str_at("residual_hash") {
            let computed = document.residual_hash();
            if recorded != computed {
                return Err(StoreError::ResidualMismatch {
                    expected: recorded.to_string(),
                    found: computed,
                });
            }
        } else {
            return Err(StoreError::Malformed(String::from(
                "no residual_hash field",
            )));
        }

        Ok(document)
    }

    /// Verify every entry and resolve it into a sealed index.
    ///
    /// `base` is the directory repo-relative paths are resolved against (the
    /// workspace root). Leaf object files are read and hashed; a leaf whose bytes
    /// do not match its seal is rejected. The returned index carries **absolute**
    /// object paths, so dispatch works regardless of the caller's cwd.
    pub fn verify_into_index(&self, base: &Path) -> Result<SealedPortIndex, StoreError> {
        // Target uniqueness first: a duplicate would make lookups ambiguous.
        let mut targets: BTreeSet<String> = BTreeSet::new();
        for e in &self.entries {
            if e.target().is_empty() {
                return Err(StoreError::Malformed(String::from(
                    "entry with empty target",
                )));
            }
            if !targets.insert(e.target().to_string()) {
                return Err(StoreError::DuplicateTarget(e.target().to_string()));
            }
        }

        let mut index = SealedPortIndex::new();

        // A composition graph must be acyclic. The runtime resolves a chain by
        // recursing through the index, so a cycle is a store that can never
        // finish — reject it as data, not as a hang.
        let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &self.entries {
            if let StoreEntry::Composition { target, leaves, .. } = entry {
                edges.insert(target.clone(), leaves.clone());
            }
        }
        if let Some(path) = composition_cycle(&edges) {
            return Err(StoreError::CompositionCycle(path.join(" -> ")));
        }

        for entry in &self.entries {
            match entry {
                StoreEntry::Leaf {
                    target,
                    object_path,
                    object_hash,
                    oracle_hash,
                    candidate_behavior_hash,
                    candidate_source_hash,
                    sealed_package,
                } => {
                    for (label, digest) in [
                        ("object_hash", object_hash),
                        ("oracle_hash", oracle_hash),
                        ("candidate_behavior_hash", candidate_behavior_hash),
                        ("candidate_source_hash", candidate_source_hash),
                    ] {
                        if !is_hex64(digest) {
                            return Err(StoreError::Malformed(format!(
                                "{}: {} is not a 64-char lowercase hex digest",
                                target, label
                            )));
                        }
                    }
                    if object_path.is_empty() {
                        return Err(StoreError::Malformed(format!(
                            "{}: empty object_path",
                            target
                        )));
                    }

                    // The seal is the object's bytes: read them and hash them.
                    let abs = resolve(base, object_path);
                    let bytes = fs::read(&abs).map_err(|_| {
                        StoreError::ObjectMissing(format!("{} ({})", object_path, abs.display()))
                    })?;
                    let found = sha256_hex(&bytes);
                    if &found != object_hash {
                        return Err(StoreError::ObjectHashMismatch {
                            path: object_path.clone(),
                            expected: object_hash.clone(),
                            found,
                        });
                    }

                    index.insert(SealedPortEntry {
                        target: target.clone(),
                        trust: TrustState::Sealed,
                        artifact: SealedArtifact::leaf_object(
                            object_hash.clone(),
                            abs.display().to_string(),
                        ),
                        oracle_hash: oracle_hash.clone(),
                        candidate_behavior_hash: candidate_behavior_hash.clone(),
                        candidate_source_hash: candidate_source_hash.clone(),
                        sealed_package: sealed_package.clone(),
                    });
                }
                StoreEntry::Composition {
                    target,
                    composition_id,
                    chain_hash,
                    leaves,
                    sealed_package,
                } => {
                    if composition_id != target {
                        return Err(StoreError::Malformed(format!(
                            "{}: composition_id does not match target",
                            target
                        )));
                    }
                    if !is_hex64(chain_hash) {
                        return Err(StoreError::Malformed(format!(
                            "{}: chain_hash is not a 64-char lowercase hex digest",
                            target
                        )));
                    }
                    if leaves.is_empty() {
                        return Err(StoreError::Malformed(format!(
                            "{}: composition has no stages",
                            target
                        )));
                    }
                    for leaf in leaves {
                        if !targets.contains(leaf) {
                            return Err(StoreError::DanglingLeaf {
                                composition: target.clone(),
                                leaf: leaf.clone(),
                            });
                        }
                    }
                    index.insert(SealedPortEntry {
                        target: target.clone(),
                        trust: TrustState::Sealed,
                        artifact: SealedArtifact::composition(
                            composition_id.clone(),
                            chain_hash.clone(),
                            leaves.clone(),
                        ),
                        oracle_hash: String::new(),
                        candidate_behavior_hash: String::new(),
                        candidate_source_hash: String::new(),
                        sealed_package: sealed_package.clone(),
                    });
                }
            }
        }

        Ok(index)
    }
}

/// Load and verify the committed store at `path` (repo-relative), returning the
/// parsed document alongside the resolved index (for inspection/CLI output).
///
/// Fails closed: any unreadable file, malformed entry, missing object, hash
/// mismatch or dangling stage is an error, never a partial index.
pub fn load_with_document(path: &str) -> Result<(StoreDocument, SealedPortIndex), StoreError> {
    let root = workspace_root();
    let abs = resolve(&root, path);
    let text = fs::read_to_string(&abs)
        .map_err(|e| StoreError::Io(format!("{}: {}", abs.display(), e)))?;
    let doc = StoreDocument::parse(&text)?;
    let index = doc.verify_into_index(&root)?;
    Ok((doc, index))
}

/// Load and verify the committed store at `path` (repo-relative).
pub fn load(path: &str) -> Result<SealedPortIndex, StoreError> {
    load_with_document(path).map(|(_, index)| index)
}

/// Load and verify the committed store at [`STORE_PATH`].
pub fn load_default() -> Result<SealedPortIndex, StoreError> {
    load(STORE_PATH)
}

/// Build the store document from the **committed evidence**, without invoking the
/// compiler and without replaying an oracle.
///
/// Leaves are taken from each target's checked-in `candidate_signature.json`
/// (object/behavior/source hashes) and `sealed_package.json` (trust + oracle
/// hash); compositions from each chain's checked-in `composition_verdict.json`
/// (chain hash + stages). The result is self-checked with `verify_into_index`, so
/// a stale or inconsistent evidence tree cannot produce a loadable store.
pub fn document_from_evidence(base: &Path) -> Result<StoreDocument, StoreError> {
    let mut entries: Vec<StoreEntry> = Vec::new();

    for symbol in LEAF_SYMBOLS {
        let dir = format!("phost/evidence/porting/{}", symbol);
        let sig = read_json(base, &format!("{}/candidate_signature.json", dir))?;
        let pkg = read_json(base, &format!("{}/sealed_package.json", dir))?;

        let target = field(&sig, "target", &dir)?;
        if pkg.str_at("trust") != Some("sealed") {
            return Err(StoreError::UnsealedEntry(target));
        }
        if pkg.str_at("court_verdict") != Some("consistent") {
            return Err(StoreError::Evidence(format!(
                "{}: court_verdict is not consistent",
                dir
            )));
        }

        entries.push(StoreEntry::Leaf {
            target,
            object_path: format!("{}/candidate.o", dir),
            object_hash: field(&sig, "candidate_object_hash", &dir)?,
            oracle_hash: field(&pkg, "oracle_hash", &dir)?,
            candidate_behavior_hash: field(&sig, "candidate_behavior_hash", &dir)?,
            candidate_source_hash: field(&sig, "candidate_source_hash", &dir)?,
            sealed_package: format!("{}/sealed_package.json", dir),
        });
    }

    for name in COMPOSITION_NAMES {
        let kind = CompositionKind::parse(name)
            .ok_or_else(|| StoreError::Evidence(format!("unknown composition {}", name)))?;
        let dir = kind.evidence_dir();
        let v = read_json(base, &format!("{}/composition_verdict.json", dir))?;
        if v.str_at("verdict") != Some("consistent") {
            return Err(StoreError::Evidence(format!(
                "{}: verdict is not consistent",
                dir
            )));
        }
        let target = field(&v, "target", dir)?;
        let leaves = v.strings_at("stages");
        if leaves.is_empty() {
            return Err(StoreError::Evidence(format!("{}: no stages", dir)));
        }
        entries.push(StoreEntry::Composition {
            composition_id: target.clone(),
            target,
            chain_hash: field(&v, "chain_hash", dir)?,
            leaves,
            sealed_package: format!("{}/composition_verdict.json", dir),
        });
    }

    let doc = StoreDocument::new(entries);
    // A generated store must itself verify (object files present and hashing to
    // their seals, every stage present) — otherwise it is not a store.
    doc.verify_into_index(base)?;
    Ok(doc)
}

/// Regenerate the committed store from committed evidence and write it.
pub fn regenerate(path: &str) -> Result<StoreDocument, StoreError> {
    let root = workspace_root();
    let doc = document_from_evidence(&root)?;
    let abs = resolve(&root, path);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| StoreError::Io(format!("{}: {}", parent.display(), e)))?;
    }
    fs::write(&abs, doc.render())
        .map_err(|e| StoreError::Io(format!("{}: {}", abs.display(), e)))?;
    Ok(doc)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Resolve a possibly-repo-relative path against `base` (absolute paths win).
fn resolve(base: &Path, path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// A 64-char lowercase hex digest (SHA-256), the only hash form court evidence
/// uses. Weak or truncated hashes are rejected rather than accepted loosely.
fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Detect a cycle in the composition dependency graph (three-colour DFS).
///
/// Only composition → composition edges matter: a leaf object terminates a chain.
/// Returns the cycle as a path (`a -> b -> a`) for the error message.
fn composition_cycle(edges: &BTreeMap<String, Vec<String>>) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        edges: &BTreeMap<String, Vec<String>>,
        color: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        match color.get(node).copied() {
            Some(2) => return None,
            Some(1) => {
                let start = stack.iter().position(|n| n == node).unwrap_or(0);
                let mut path = stack[start..].to_vec();
                path.push(node.to_string());
                return Some(path);
            }
            _ => {}
        }
        color.insert(node.to_string(), 1);
        stack.push(node.to_string());
        if let Some(children) = edges.get(node) {
            for child in children {
                if let Some(path) = visit(child, edges, color, stack) {
                    return Some(path);
                }
            }
        }
        stack.pop();
        color.insert(node.to_string(), 2);
        None
    }

    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    let mut stack: Vec<String> = Vec::new();
    for node in edges.keys() {
        if let Some(path) = visit(node, edges, &mut color, &mut stack) {
            return Some(path);
        }
    }
    None
}

fn read_json(base: &Path, rel: &str) -> Result<Json, StoreError> {
    let abs = base.join(rel);
    let text = fs::read_to_string(&abs)
        .map_err(|e| StoreError::Io(format!("{}: {}", abs.display(), e)))?;
    Json::parse(&text).map_err(|e| StoreError::Parse(format!("{}: {}", rel, e)))
}

/// A required non-empty string field.
fn field(doc: &Json, key: &str, ctx: &str) -> Result<String, StoreError> {
    match doc.str_at(key) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(StoreError::Evidence(format!("{}: missing {}", ctx, key))),
    }
}

fn parse_entry(value: &Json, index: usize) -> Result<StoreEntry, StoreError> {
    let target = value
        .str_at("target")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| StoreError::Malformed(format!("entry {} has no target", index)))?
        .to_string();

    if value.str_at("trust") != Some("sealed") {
        return Err(StoreError::UnsealedEntry(target));
    }

    let kind = value.str_at("kind").ok_or_else(|| {
        StoreError::Malformed(format!("entry {} ({}) has no kind", index, target))
    })?;

    // Every field is required and non-empty — a store entry is a claim about a
    // seal, and an absent field is a missing claim, not an optional detail.
    let req = |key: &str| -> Result<String, StoreError> {
        value
            .str_at(key)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                StoreError::Malformed(format!("entry {} ({}) missing {}", index, target, key))
            })
    };

    match kind {
        "leaf-object" => Ok(StoreEntry::Leaf {
            object_path: req("object_path")?,
            object_hash: req("object_hash")?,
            oracle_hash: req("oracle_hash")?,
            candidate_behavior_hash: req("candidate_behavior_hash")?,
            candidate_source_hash: req("candidate_source_hash")?,
            sealed_package: req("sealed_package")?,
            target,
        }),
        "composition" => {
            let leaves = value.strings_at("leaves");
            if leaves.is_empty() {
                return Err(StoreError::Malformed(format!(
                    "entry {} ({}) composition has no leaves",
                    index, target
                )));
            }
            Ok(StoreEntry::Composition {
                composition_id: req("composition_id")?,
                chain_hash: req("chain_hash")?,
                leaves,
                sealed_package: req("sealed_package")?,
                target,
            })
        }
        other => Err(StoreError::Malformed(format!(
            "entry {} ({}) has unknown kind {}",
            index, target, other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn leaf(target: &str) -> StoreEntry {
        StoreEntry::Leaf {
            target: target.to_string(),
            object_path: format!("phost/evidence/porting/{}/candidate.o", target),
            object_hash: "a".repeat(64),
            oracle_hash: "b".repeat(64),
            candidate_behavior_hash: "c".repeat(64),
            candidate_source_hash: "d".repeat(64),
            sealed_package: "sealed_package.json".to_string(),
        }
    }

    fn composition(target: &str, leaves: &[&str]) -> StoreEntry {
        StoreEntry::Composition {
            target: target.to_string(),
            composition_id: target.to_string(),
            chain_hash: "e".repeat(64),
            leaves: leaves.iter().map(|l| l.to_string()).collect(),
            sealed_package: "composition_verdict.json".to_string(),
        }
    }

    #[test]
    fn test_render_and_parse_round_trip() {
        let doc = StoreDocument::new(vec![
            leaf("libc:toupper:c-locale:u8:v1"),
            composition(
                "phor:compose:toupper_memchr:c-locale:index:v1",
                &["libc:toupper:c-locale:u8:v1"],
            ),
        ]);
        let text = doc.render();
        let parsed = StoreDocument::parse(&text).unwrap();
        assert_eq!(parsed, doc);
        assert_eq!(parsed.residual_hash(), doc.residual_hash());
    }

    #[test]
    fn test_residual_hash_changes_when_an_entry_changes() {
        let mut doc = StoreDocument::new(vec![leaf("libc:toupper:c-locale:u8:v1")]);
        let before = doc.residual_hash();
        if let StoreEntry::Leaf { object_hash, .. } = &mut doc.entries[0] {
            *object_hash = "f".repeat(64);
        }
        assert_ne!(doc.residual_hash(), before);
    }

    #[test]
    fn test_parse_rejects_a_tampered_residual() {
        let doc = StoreDocument::new(vec![leaf("libc:toupper:c-locale:u8:v1")]);
        let text = doc.render().replace(&doc.residual_hash(), &"0".repeat(64));
        assert!(matches!(
            StoreDocument::parse(&text),
            Err(StoreError::ResidualMismatch { .. })
        ));
    }

    #[test]
    fn test_parse_rejects_the_wrong_schema() {
        let text = r#"{"schema":"nope","entry_count":0,"entries":[],"residual_hash":"x"}"#;
        assert!(matches!(
            StoreDocument::parse(text),
            Err(StoreError::Schema(_))
        ));
    }

    #[test]
    fn test_parse_rejects_an_unsealed_entry() {
        let text = format!(
            "{{\"schema\":\"{}\",\"entries\":[{{\"target\":\"t\",\"kind\":\"leaf-object\",\"trust\":\"observed\",\"object_path\":\"p\",\"object_hash\":\"a\",\"oracle_hash\":\"b\",\"candidate_behavior_hash\":\"c\",\"candidate_source_hash\":\"d\",\"sealed_package\":\"s\"}}],\"residual_hash\":\"0\"}}",
            STORE_SCHEMA
        );
        assert!(matches!(
            StoreDocument::parse(&text),
            Err(StoreError::UnsealedEntry(_))
        ));
    }

    #[test]
    fn test_verify_rejects_a_dangling_composition_stage() {
        let doc = StoreDocument::new(vec![composition(
            "phor:compose:toupper_memchr:c-locale:index:v1",
            &["libc:toupper:c-locale:u8:v1"],
        )]);
        let err = doc.verify_into_index(Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, StoreError::DanglingLeaf { .. }));
    }

    #[test]
    fn test_verify_rejects_a_missing_object() {
        let doc = StoreDocument::new(vec![leaf("libc:toupper:c-locale:u8:v1")]);
        let err = doc
            .verify_into_index(Path::new("/nonexistent-root"))
            .unwrap_err();
        assert!(matches!(err, StoreError::ObjectMissing(_)));
    }

    #[test]
    fn test_verify_rejects_a_duplicate_target() {
        let doc = StoreDocument::new(vec![
            leaf("libc:toupper:c-locale:u8:v1"),
            leaf("libc:toupper:c-locale:u8:v1"),
        ]);
        let err = doc.verify_into_index(Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, StoreError::DuplicateTarget(_)));
    }

    #[test]
    fn test_verify_rejects_a_composition_cycle() {
        // The runtime resolves a chain by recursing through the index, so a cycle
        // is rejected as data rather than followed.
        let doc = StoreDocument::new(vec![
            composition("phor:compose:a", &["phor:compose:b"]),
            composition("phor:compose:b", &["phor:compose:a"]),
        ]);
        let err = doc.verify_into_index(Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, StoreError::CompositionCycle(_)));
    }

    #[test]
    fn test_verify_rejects_a_self_referential_composition() {
        let doc = StoreDocument::new(vec![composition("phor:compose:a", &["phor:compose:a"])]);
        let err = doc.verify_into_index(Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, StoreError::CompositionCycle(_)));
    }

    #[test]
    fn test_verify_rejects_a_non_hex_digest() {
        let mut doc = StoreDocument::new(vec![leaf("libc:toupper:c-locale:u8:v1")]);
        if let StoreEntry::Leaf { object_hash, .. } = &mut doc.entries[0] {
            *object_hash = "deadbeef".to_string();
        }
        let err = doc.verify_into_index(Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, StoreError::Malformed(_)));
    }

    // ---- the committed store itself ----

    #[test]
    fn test_committed_store_loads_and_covers_every_port() {
        let root = workspace_root();
        let doc = document_from_evidence(&root).expect("committed evidence builds a store");
        let index = load(STORE_PATH).expect("committed store loads");

        assert_eq!(index.len(), LEAF_SYMBOLS.len() + COMPOSITION_NAMES.len());
        assert_eq!(doc.entries.len(), index.len());

        // Loading reproduces the generated document exactly (deterministic).
        let text = std::fs::read_to_string(root.join(STORE_PATH)).unwrap();
        assert_eq!(
            text,
            doc.render(),
            "committed store index is stale; run `phost port store --write`"
        );
    }

    #[test]
    fn test_loaded_store_reproduces_every_recorded_hash() {
        let root = workspace_root();
        let index = load(STORE_PATH).expect("committed store loads");
        let auth = crate::porting::PortingAuthority::granted();

        for symbol in LEAF_SYMBOLS {
            let sig = read_json(
                &root,
                &format!("phost/evidence/porting/{}/candidate_signature.json", symbol),
            )
            .unwrap();
            let target = sig.str_at("target").unwrap();
            let (path, hash) = index
                .native_artifact(target, &auth)
                .unwrap_or_else(|| panic!("{} is not a sealed leaf in the store", target));
            assert_eq!(hash, sig.str_at("candidate_object_hash").unwrap());
            assert!(
                Path::new(path).is_file(),
                "{} object is not on disk",
                target
            );
        }

        for name in COMPOSITION_NAMES {
            let kind = CompositionKind::parse(name).unwrap();
            let verdict = read_json(
                &root,
                &format!("{}/composition_verdict.json", kind.evidence_dir()),
            )
            .unwrap();
            let target = verdict.str_at("target").unwrap();
            let (id, chain_hash) = index
                .sealed_chain(target, &auth)
                .unwrap_or_else(|| panic!("{} is not a sealed composition in the store", target));
            assert_eq!(id, target);
            assert_eq!(chain_hash, verdict.str_at("chain_hash").unwrap());
        }
    }

    #[test]
    fn test_loaded_store_needs_no_capability_to_be_gated() {
        let index = load(STORE_PATH).expect("committed store loads");
        let denied = crate::porting::PortingAuthority::none();
        let target = crate::porting::target::LIBC_TOUPPER.id;
        assert!(index.lookup_gated(target, &denied).is_none());
        assert!(index.native_artifact(target, &denied).is_none());
    }
}
