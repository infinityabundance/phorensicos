// phorport/compile.rs — compiling a proposed source into a candidate identity
//
// A producer proposes *source bytes*; the foundry compiles them with `phorc` and
// derives a **candidate identity**. The identity is a function of the source
// bytes (and the compiler), so any source-byte change produces a new identity and
// invalidates every downstream piece of evidence — that is the whole point.

use std::path::Path;

use sha2::{Digest, Sha256};

use phost::porting::compiled::{compile_candidate, workspace_root};
use phost::porting::target::{resolve_target, PortTarget};

/// The identity of one candidate revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateIdentity {
    /// The revision ordinal within the campaign (0-based).
    pub revision: u32,
    /// SHA-256 of the candidate source bytes.
    pub source_hash: String,
    /// SHA-256 of the compiled `.phor` object (`phorc`-deterministic).
    pub object_hash: String,
    /// The compiled object's path (canonicalized).
    pub object_path: String,
}

impl CandidateIdentity {
    /// The canonical identity string bound into campaign evidence.
    pub fn canonical(&self) -> String {
        format!(
            "revision={};source_hash={};object_hash={}",
            self.revision, self.source_hash, self.object_hash
        )
    }
}

/// SHA-256 (lowercase hex) of a byte string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Compile `source` as `symbol`'s ABI under `work_dir`, returning the candidate
/// identity (revision is left at 0 for the caller to set).
pub fn compile_source(
    symbol: &str,
    source: &str,
    work_dir: &Path,
) -> Result<CandidateIdentity, String> {
    let base = resolve_target(symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
    std::fs::create_dir_all(work_dir).map_err(|e| e.to_string())?;
    let src = work_dir.join("candidate.phor");
    std::fs::write(&src, source).map_err(|e| e.to_string())?;

    // Compile from a path **relative to the workspace root** whenever the working
    // directory is inside it. `phorc` embeds the source path in the object, so a
    // relative path makes the emitted bytes — and therefore the autonomous seal —
    // independent of the absolute checkout location (`/work` in a container, any
    // path on a host). This is the canonical invocation for byte-reproducible
    // rebuilds (Phase 7/§22).
    let root = workspace_root();
    let source_path = match src.strip_prefix(&root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => src.to_string_lossy().into_owned(),
    };

    // `PortTarget` carries a `&'static str` source path. The foundry compiles a
    // bounded number of revisions, so leaking the path is bounded and honest.
    let src_static: &'static str = Box::leak(source_path.into_boxed_str());
    let target = PortTarget {
        candidate_source: src_static,
        ..base
    };
    let compiled = compile_candidate(&target, work_dir.to_str().unwrap_or("."), None)
        .map_err(|e| e.message())?;
    let object_path = std::fs::canonicalize(&compiled.object_path)
        .map_err(|e| e.to_string())?
        .display()
        .to_string();

    Ok(CandidateIdentity {
        revision: 0,
        source_hash: sha256_hex(source.as_bytes()),
        object_hash: compiled.object_hash,
        object_path,
    })
}
