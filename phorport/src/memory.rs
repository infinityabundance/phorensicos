// phorport/memory.rs — longitudinal porting memory (Gemel)
//
// Phase 5. A campaign must become better informed because previous work happened,
// and it must remember **wrong** work as first-class knowledge: a candidate that
// already failed, and why. Gemel is the durable engineering-memory layer; this
// module publishes phorport's durable boundaries into it and reads them back.
//
// It is deliberately small and does not reimplement Gemel: it uses Gemel's own
// object model (`Family`, `Field`, `Value`), its content-addressed store
// (`insert_object`, `scan_canonical`) and its opaque Gids, retained verbatim and
// never merged with Phorensicos hashes or FRF ids (three identity namespaces).
//
// Gemel is optional. When no repository is discoverable, `PortMemory::open`
// returns `None` and every campaign stays standalone: a missing memory layer is
// never a campaign failure.

use std::path::Path;

use gemel::family::Family as GFamily;
use gemel::gid::Gid;
use gemel::store::{now_ms, InitOptions, Repo};
use gemel::value::{Field, Object, Value};

/// Marks a Gemel object as a phorport memory record, so a scan can find them
/// without a separate index.
pub const MEMORY_MARKER: &str = "phorport.port.memory.v1";

const TAG_SUMMARY: u8 = 0x02;
const TAG_CLASSIFICATION: u8 = 0x03;
const TAG_SCOPE: u8 = 0x05;
const TAG_FIRST_OBSERVED: u8 = 0x09;
const TAG_CREATED_AT: u8 = 0x0c;

/// The boundary kinds phorport publishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryKind {
    /// A candidate was proposed and rejected by the court.
    CandidateRejected,
    /// A counterexample was discovered (and minimized).
    Counterexample,
    /// A campaign completed.
    CampaignCompleted,
}

impl MemoryKind {
    fn as_str(self) -> &'static str {
        match self {
            MemoryKind::CandidateRejected => "candidate-rejected",
            MemoryKind::Counterexample => "counterexample",
            MemoryKind::CampaignCompleted => "campaign-completed",
        }
    }
}

/// One durable memory record read back from Gemel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRecord {
    pub gid: String,
    pub kind: String,
    pub target: String,
    pub candidate_hash: String,
    pub residual: String,
    pub detail: String,
}

/// The durable porting memory, backed by a Gemel repository.
pub struct PortMemory {
    repo: Repo,
}

impl PortMemory {
    /// Discover a repository by walking up from `start`; `None` when absent.
    pub fn open(start: &Path) -> Option<PortMemory> {
        Repo::find(start).ok().map(|repo| PortMemory { repo })
    }

    /// Open or create a repository rooted exactly at `root`.
    pub fn at(root: &Path) -> Result<PortMemory, String> {
        match Repo::open(root) {
            Ok(repo) => Ok(PortMemory { repo }),
            Err(_) => {
                std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
                Repo::init(
                    root,
                    &InitOptions {
                        author_name: Some(String::from("phorport")),
                        author_email: None,
                    },
                )
                .map(|repo| PortMemory { repo })
                .map_err(|e| e.to_string())
            }
        }
    }

    /// The repository root.
    pub fn root(&self) -> &Path {
        self.repo.root()
    }

    fn insert(
        &self,
        kind: MemoryKind,
        target: &str,
        candidate_hash: &str,
        residual: &str,
        detail: &str,
    ) -> Result<Gid, String> {
        // Gemel validates its families' schemas, so phorport does not invent
        // field tags: the record rides Gemel's `Residual` schema (the negative-
        // knowledge family) — a summary, a classification, a timestamp, and the
        // structured detail in the residual `scope.paths` array.
        let paths = Value::Array(vec![
            Value::Str(MEMORY_MARKER.to_string()),
            Value::Str(kind.as_str().to_string()),
            Value::Str(target.to_string()),
            Value::Str(candidate_hash.to_string()),
            Value::Str(residual.to_string()),
            Value::Str(detail.to_string()),
        ]);
        let scope = Value::Record(vec![Field::new(0x03, paths)]);
        let now = now_ms();
        let obj = Object::fields(
            GFamily::Residual,
            vec![
                Field::new(TAG_SUMMARY, Value::Str(detail.to_string())),
                Field::new(
                    TAG_CLASSIFICATION,
                    Value::Str("contract_mismatch".to_string()),
                ),
                Field::new(TAG_SCOPE, scope),
                Field::new(TAG_FIRST_OBSERVED, Value::I(now)),
                Field::new(TAG_CREATED_AT, Value::I(now)),
            ],
        );
        self.repo.insert_object(&obj).map_err(|e| e.to_string())
    }

    /// Record a rejected candidate: durable **negative knowledge**.
    pub fn record_candidate_rejected(
        &self,
        target: &str,
        candidate_hash: &str,
        residual: &str,
        detail: &str,
    ) -> Result<Gid, String> {
        self.insert(
            MemoryKind::CandidateRejected,
            target,
            candidate_hash,
            residual,
            detail,
        )
    }

    /// Record a discovered counterexample.
    pub fn record_counterexample(
        &self,
        target: &str,
        candidate_hash: &str,
        residual: &str,
        detail: &str,
    ) -> Result<Gid, String> {
        self.insert(
            MemoryKind::Counterexample,
            target,
            candidate_hash,
            residual,
            detail,
        )
    }

    /// Every phorport record, in the repository's canonical scan order.
    pub fn records(&self) -> Vec<MemoryRecord> {
        let mut out = Vec::new();
        for (gid, obj) in self.repo.scan_canonical() {
            if obj.family != GFamily::Residual {
                continue;
            }
            let Some(fields) = obj.field_sequence() else {
                continue;
            };
            // The marker and the structured detail ride `scope.paths`.
            let paths = fields
                .iter()
                .find(|f| f.tag == TAG_SCOPE)
                .and_then(|f| match &f.value {
                    Value::Record(rec) => rec.iter().find(|r| r.tag == 0x03).and_then(|r| match &r
                        .value
                    {
                        Value::Array(a) => Some(a.clone()),
                        _ => None,
                    }),
                    _ => None,
                });
            let Some(paths) = paths else { continue };
            let s = |i: usize| match paths.get(i) {
                Some(Value::Str(v)) => Some(v.clone()),
                _ => None,
            };
            if s(0).as_deref() != Some(MEMORY_MARKER) {
                continue;
            }
            out.push(MemoryRecord {
                gid: gid.to_string(),
                kind: s(1).unwrap_or_default(),
                target: s(2).unwrap_or_default(),
                candidate_hash: s(3).unwrap_or_default(),
                residual: s(4).unwrap_or_default(),
                detail: s(5).unwrap_or_default(),
            });
        }
        out
    }

    /// Prior rejections of an identical candidate (by source identity).
    ///
    /// This is how a later campaign avoids treating already-failed work as new:
    /// the reason is retrieved without re-running the search.
    pub fn prior_rejections(&self, candidate_hash: &str) -> Vec<MemoryRecord> {
        self.records()
            .into_iter()
            .filter(|r| {
                r.kind == MemoryKind::CandidateRejected.as_str()
                    && r.candidate_hash == candidate_hash
            })
            .collect()
    }

    /// All records for a target, in scan order.
    pub fn history(&self, target: &str) -> Vec<MemoryRecord> {
        self.records()
            .into_iter()
            .filter(|r| r.target == target)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_round_trips_negative_knowledge() {
        let dir = std::env::temp_dir().join(format!("phorport-mem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mem = PortMemory::at(&dir).expect("init");
        assert!(mem.records().is_empty());

        mem.record_candidate_rejected(
            "posix:strspn:x:v1",
            "hashA",
            "PORT.LENGTH",
            "duplicate lane",
        )
        .expect("record");
        mem.record_counterexample(
            "posix:strspn:x:v1",
            "hashA",
            "PORT.LENGTH",
            "minimal 08047f",
        )
        .expect("record");

        let prior = mem.prior_rejections("hashA");
        assert_eq!(prior.len(), 1);
        assert_eq!(prior[0].residual, "PORT.LENGTH");
        assert_eq!(prior[0].detail, "duplicate lane");
        assert_eq!(mem.history("posix:strspn:x:v1").len(), 2);
        assert!(mem.prior_rejections("hashB").is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
