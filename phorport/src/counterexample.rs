// phorport/counterexample.rs — durable, content-addressed counterexamples
//
// A discovered mismatch is durable evidence, not a transient log line. The
// original input is **never discarded** when a smaller reproducer is found: both
// are retained, with their residual lineages and the identities (candidate object
// hash, oracle output, candidate output) that make the record replayable.

use std::fs;
use std::io;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::minimize::Minimization;

/// A durable differential counterexample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counterexample {
    pub target: String,
    pub candidate_path: String,
    pub candidate_hash: String,
    pub original_hex: String,
    pub original_residual: String,
    pub minimal_hex: String,
    pub minimal_residual: String,
    pub oracle_hex: String,
    pub candidate_output_hex: String,
    pub accepted_reductions: u64,
    pub refused_reductions: u64,
}

impl Counterexample {
    pub fn from_minimization(
        target: &str,
        candidate_path: &str,
        candidate_hash: &str,
        oracle_hex: &str,
        candidate_output_hex: &str,
        m: &Minimization,
    ) -> Self {
        Counterexample {
            target: target.to_string(),
            candidate_path: candidate_path.to_string(),
            candidate_hash: candidate_hash.to_string(),
            original_hex: hex::encode(&m.original),
            original_residual: m.original_residual.clone(),
            minimal_hex: hex::encode(&m.minimal),
            minimal_residual: m.minimal_residual.clone(),
            oracle_hex: oracle_hex.to_string(),
            candidate_output_hex: candidate_output_hex.to_string(),
            accepted_reductions: m.accepted,
            refused_reductions: m.refused,
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "target={};candidate_hash={};original_hex={};original_residual={};minimal_hex={};minimal_residual={};oracle_hex={};candidate_output_hex={};accepted={};refused={}",
            self.target,
            self.candidate_hash,
            self.original_hex,
            self.original_residual,
            self.minimal_hex,
            self.minimal_residual,
            self.oracle_hex,
            self.candidate_output_hex,
            self.accepted_reductions,
            self.refused_reductions
        )
    }

    /// The content identity of the counterexample record.
    pub fn content_id(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"PHOR/PHORPORT/COUNTEREXAMPLE/v1\0");
        h.update(self.canonical().as_bytes());
        hex::encode(h.finalize())
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.phorport.counterexample.v1\",\n  \"target\": \"{}\",\n  \"candidate_path\": \"{}\",\n  \"candidate_hash\": \"{}\",\n  \"original_hex\": \"{}\",\n  \"original_residual\": \"{}\",\n  \"minimal_hex\": \"{}\",\n  \"minimal_residual\": \"{}\",\n  \"oracle_hex\": \"{}\",\n  \"candidate_output_hex\": \"{}\",\n  \"accepted_reductions\": {},\n  \"refused_reductions\": {},\n  \"content_id\": \"{}\"\n}}\n",
            self.target,
            self.candidate_path,
            self.candidate_hash,
            self.original_hex,
            self.original_residual,
            self.minimal_hex,
            self.minimal_residual,
            self.oracle_hex,
            self.candidate_output_hex,
            self.accepted_reductions,
            self.refused_reductions,
            self.content_id()
        )
    }
}

/// Write a counterexample record, returning its path.
pub fn write(dir: &str, cx: &Counterexample) -> io::Result<String> {
    let base = PathBuf::from(dir);
    fs::create_dir_all(&base)?;
    let path = base.join(format!("{}.json", cx.content_id()));
    fs::write(&path, cx.to_json())?;
    Ok(path.display().to_string())
}
