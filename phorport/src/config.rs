// phorport/config.rs — the harness runtime configuration (from the environment)
//
// The generated fuzz target is built once and run many times; what it attacks is
// supplied at run time by the coordinator through the environment, so the same
// binary can be pointed at a different candidate without a rebuild.

use std::env;

/// What the harness attacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessConfig {
    /// The qualified port target id, e.g. `posix:strspn:c-locale:u64:v1`.
    pub target_id: String,
    /// The compiled `.phor` object under test.
    pub candidate_path: String,
    /// The object's SHA-256 (the seal the harness verifies before mapping).
    pub candidate_hash: String,
}

impl HarnessConfig {
    /// Read the configuration from the environment, or `None` when unset.
    pub fn from_env() -> Option<Self> {
        Some(HarnessConfig {
            target_id: env::var("PHOR_TARGET").ok()?,
            candidate_path: env::var("PHOR_CANDIDATE").ok()?,
            candidate_hash: env::var("PHOR_CANDIDATE_HASH").ok()?,
        })
    }

    /// The environment variables the coordinator must set for the harness.
    pub fn env_pairs(&self) -> [(&'static str, &str); 3] {
        [
            ("PHOR_TARGET", self.target_id.as_str()),
            ("PHOR_CANDIDATE", self.candidate_path.as_str()),
            ("PHOR_CANDIDATE_HASH", self.candidate_hash.as_str()),
        ]
    }
}
