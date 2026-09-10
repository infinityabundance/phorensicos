// porting/compiled.rs — Compiled candidate authority (std)
//
// The promoted implementation is the *compiled* clean-room candidate, not just a
// Rust mirror of it. This module invokes `phorc` on the target's `.phor` source,
// hashes the emitted ELF64 object and its receipt file, and records the compiler
// provenance. Those hashes are bound into the candidate signature and the seal.
//
// Reproducibility: the compiler is invoked from the workspace root with the
// repo-relative source path, so the ELF `FILE` symbol is environment-independent
// and the object hash is stable across hosts and containers. (This depends on
// `phorc` emitting deterministic objects — register allocation was made
// deterministic for exactly this reason.)

use alloc::format;
use alloc::string::{String, ToString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::porting::candidate::CandidateArtifacts;
use crate::porting::sha256_hex;
use crate::porting::target::PortTarget;

/// Why a candidate could not be compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// The `phorc` binary could not be located.
    CompilerNotFound(String),
    /// The `.phor` source is missing from the workspace.
    MissingSource(String),
    /// The compiler exited non-zero.
    Failed { status: String, stderr: String },
    /// The compiler succeeded but an artifact is missing.
    MissingArtifacts(String),
    /// `phorc --version` did not succeed.
    VersionUnavailable,
    /// I/O failure.
    Io(String),
}

impl CompileError {
    pub fn message(&self) -> String {
        match self {
            CompileError::CompilerNotFound(p) => format!("phorc not found: {}", p),
            CompileError::MissingSource(s) => format!("candidate source not found: {}", s),
            CompileError::Failed { status, stderr } => {
                format!("phorc failed ({}) {}", status, stderr.trim())
            }
            CompileError::MissingArtifacts(p) => format!("compiler artifact missing: {}", p),
            CompileError::VersionUnavailable => String::from("phorc --version failed"),
            CompileError::Io(m) => format!("I/O error: {}", m),
        }
    }
}

/// The compiled artifact bound into the seal, plus its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledCandidate {
    pub compiler: String,
    pub compiler_version: String,
    /// Repo-relative `.phor` source.
    pub source: String,
    pub source_hash: String,
    pub object_path: String,
    pub object_hash: String,
    pub receipt_path: String,
    pub receipt_hash: String,
}

impl CompiledCandidate {
    /// The hash-only view bound into signatures and the seal.
    pub fn artifacts(&self) -> CandidateArtifacts {
        CandidateArtifacts {
            source_hash: self.source_hash.clone(),
            object_hash: self.object_hash.clone(),
            receipt_hash: self.receipt_hash.clone(),
            compiler_version: self.compiler_version.clone(),
        }
    }
}

/// Walk up from the current directory to the workspace root (a directory with a
/// `Cargo.toml` and an `examples/` tree).
fn workspace_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut dir: PathBuf = cwd.clone();
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("examples").is_dir() {
            return dir;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return cwd,
        }
    }
}

/// Locate the `phorc` compiler: an explicit override, else a sibling of the
/// running executable (cargo target dir), else `PATH`.
fn resolve_phorc(override_path: Option<&str>) -> Result<PathBuf, CompileError> {
    if let Some(p) = override_path {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Ok(pb);
        }
        return Err(CompileError::CompilerNotFound(p.to_string()));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("phorc");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Ok(paths) = std::env::var("PATH") {
        for entry in paths.split(':') {
            if entry.is_empty() {
                continue;
            }
            let candidate = Path::new(entry).join("phorc");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(CompileError::CompilerNotFound(String::from("phorc")))
}

fn phorc_version(phorc: &Path) -> Result<String, CompileError> {
    let out = Command::new(phorc)
        .arg("--version")
        .output()
        .map_err(|e| CompileError::Io(format!("{}", e)))?;
    if !out.status.success() {
        return Err(CompileError::VersionUnavailable);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Compile `target`'s clean-room candidate and hash the artifacts.
///
/// Artifacts are written into `out_dir` as `candidate.o` and
/// `candidate.receipts.json`.
pub fn compile_candidate(
    target: &PortTarget,
    out_dir: &str,
    phorc_override: Option<&str>,
) -> Result<CompiledCandidate, CompileError> {
    let root = workspace_root();
    let source_rel = target.candidate_source;
    let source_abs = root.join(source_rel);
    if !source_abs.is_file() {
        return Err(CompileError::MissingSource(source_rel.to_string()));
    }

    let phorc = resolve_phorc(phorc_override)?;

    let out_dir_abs = if Path::new(out_dir).is_absolute() {
        PathBuf::from(out_dir)
    } else {
        root.join(out_dir)
    };
    fs::create_dir_all(&out_dir_abs).map_err(|e| CompileError::Io(format!("{}", e)))?;

    let obj_path = out_dir_abs.join("candidate.o");
    let receipt_path = out_dir_abs.join("candidate.receipts.json");

    // Compile from the workspace root with the repo-relative source path so the
    // embedded FILE symbol is host-independent.
    let output = Command::new(&phorc)
        .current_dir(&root)
        .arg(source_rel)
        .arg(&obj_path)
        .arg("--emit-receipts")
        .output()
        .map_err(|e| CompileError::Io(format!("{}", e)))?;

    if !output.status.success() {
        return Err(CompileError::Failed {
            status: format!("{:?}", output.status),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let source_bytes = fs::read(&source_abs).map_err(|e| CompileError::Io(format!("{}", e)))?;
    let object_bytes = fs::read(&obj_path)
        .map_err(|_| CompileError::MissingArtifacts(obj_path.display().to_string()))?;
    let receipt_bytes = fs::read(&receipt_path)
        .map_err(|_| CompileError::MissingArtifacts(receipt_path.display().to_string()))?;

    let compiler_version = phorc_version(&phorc)?;

    Ok(CompiledCandidate {
        compiler: String::from("phorc"),
        compiler_version,
        source: source_rel.to_string(),
        source_hash: sha256_hex(&source_bytes),
        object_path: obj_path.display().to_string(),
        object_hash: sha256_hex(&object_bytes),
        receipt_path: receipt_path.display().to_string(),
        receipt_hash: sha256_hex(&receipt_bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::{LIBC_MEMCMP, LIBC_TOUPPER};

    #[test]
    fn test_artifacts_view_carries_all_hashes() {
        let c = CompiledCandidate {
            compiler: "phorc".to_string(),
            compiler_version: "phorc 0.1.0".to_string(),
            source: "examples/jit_port_memcmp.phor".to_string(),
            source_hash: "s".to_string(),
            object_path: "/tmp/candidate.o".to_string(),
            object_hash: "o".to_string(),
            receipt_path: "/tmp/candidate.receipts.json".to_string(),
            receipt_hash: "r".to_string(),
        };
        let a = c.artifacts();
        assert_eq!(a.source_hash, "s");
        assert_eq!(a.object_hash, "o");
        assert_eq!(a.receipt_hash, "r");
        assert_eq!(a.compiler_version, "phorc 0.1.0");
    }

    #[test]
    fn test_workspace_root_has_examples() {
        // Tests run from the crate dir; the walk must find the workspace root.
        let root = workspace_root();
        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("examples").is_dir());
    }

    #[test]
    fn test_source_paths_are_repo_relative() {
        assert!(LIBC_TOUPPER.candidate_source.starts_with("examples/"));
        assert!(LIBC_MEMCMP.candidate_source.starts_with("examples/"));
        assert!(!LIBC_MEMCMP.candidate_source.starts_with('/'));
    }
}
