// porting/cross_impl.rs — the cross-implementation court (std)
//
// The dialect cage observes the host C library. That observation is what a seal
// binds, and it is one implementation: "the POSIX contract for `strspn`" is really
// "the POSIX contract as this host implements it".
//
// This court closes that gap for a target by observing the *same sealed corpus*
// through a **second, independent implementation** and requiring agreement:
//
//   target -> cases_for(target)                      the sealed corpus
//          -> dialect_cage (host C library, in-process)      -> implementation A
//          -> musl probe (statically linked, out-of-process) -> implementation B
//          -> per-case comparison -> cross-implementation verdict
//
// Promotion of the port is unchanged; this is an additional, orthogonal residual.
// The verdict binds `primary_oracle_hash`, which is the *sealed* oracle hash, so the
// cross-implementation claim cannot be swapped in without the seal it refers to.
//
// Two honest limits, both load-bearing:
//
//   1. Agreement on a bounded corpus is **evidence, not proof**. Two implementations
//      agreeing on every case of a sealed corpus is strong evidence that the corpus
//      does not distinguish them; it is not a proof of equivalence.
//   2. The probe is an instrument, not part of the promoted artifact. Its per-symbol
//      decoding deliberately duplicates the Rust cage's normalization, because an
//      observer that shared the cage's code could only confirm the cage.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::process::{Command, Stdio};

use crate::porting::compiled::workspace_root;
use crate::porting::dialect_cage;
use crate::porting::oracle_trace::{combined_oracle_hash, OracleTrace};
use crate::porting::replay_court::CourtVerdict;
use crate::porting::target::{cases_for, PortTarget, TestCase};
use crate::porting::{json_escape, sha256_hex, PortError, PortingAuthority};

/// The probe source, repo-relative. Committed; its hash is bound into the verdict.
pub const PROBE_SOURCE: &str = "phost/foreign/musl_probe.c";

/// A second implementation of the same contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implementation {
    /// The process's own C library, observed in-process through the dialect cage.
    HostLibc,
    /// musl, observed out-of-process through the statically linked probe.
    Musl,
}

impl Implementation {
    pub fn id(&self) -> &'static str {
        match self {
            Implementation::HostLibc => "host-libc",
            Implementation::Musl => "musl",
        }
    }

    /// How the implementation is reached — recorded so a reader knows whether a
    /// difference would be a library difference or an observer difference.
    pub fn mechanism(&self) -> &'static str {
        match self {
            Implementation::HostLibc => "in-process FFI (the dialect cage)",
            Implementation::Musl => "out-of-process statically linked probe",
        }
    }
}

/// The observer for the host C library's own version, when the platform exposes it.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn host_libc_version() -> String {
    use core::ffi::{c_char, CStr};
    extern "C" {
        fn gnu_get_libc_version() -> *const c_char;
    }
    unsafe {
        let p = gnu_get_libc_version();
        if p.is_null() {
            return String::from("unknown");
        }
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn host_libc_version() -> String {
    String::from("unknown")
}

/// A compiled second-implementation probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeArtifact {
    pub path: String,
    pub source: String,
    pub source_hash: String,
    pub binary_hash: String,
    /// The probe's own `--which` report, e.g. `libc=musl`.
    pub identity: String,
}

impl ProbeArtifact {
    /// The library the probe was compiled against, parsed from its `--which` report.
    pub fn libc(&self) -> Option<&str> {
        self.identity
            .lines()
            .find_map(|l| l.strip_prefix("libc="))
            .map(|s| s.trim())
    }
}

/// Compile the probe with `musl-gcc`. Statically linked, so the functions it calls
/// are musl's and cannot be the host's.
pub fn compile_probe(out_path: &str) -> Result<ProbeArtifact, PortError> {
    let root = workspace_root();
    let source_abs = root.join(PROBE_SOURCE);
    if !source_abs.is_file() {
        return Err(PortError::Execution(format!(
            "cross-implementation probe source is missing: {}",
            PROBE_SOURCE
        )));
    }
    let out_abs = if std::path::Path::new(out_path).is_absolute() {
        std::path::PathBuf::from(out_path)
    } else {
        root.join(out_path)
    };
    if let Some(parent) = out_abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PortError::Io(format!("{}: {}", parent.display(), e)))?;
    }

    let output = Command::new("musl-gcc")
        .current_dir(&root)
        .args(["-static", "-O1", "-o"])
        .arg(&out_abs)
        .arg(PROBE_SOURCE)
        .output()
        .map_err(|e| {
            PortError::Execution(format!(
                "musl-gcc is required for the cross-implementation court: {}",
                e
            ))
        })?;
    if !output.status.success() {
        return Err(PortError::Execution(format!(
            "musl-gcc failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let source_bytes = std::fs::read(&source_abs).map_err(|e| PortError::Io(format!("{}", e)))?;
    let binary_bytes = std::fs::read(&out_abs).map_err(|e| PortError::Io(format!("{}", e)))?;
    let identity = probe_identity(&out_abs.display().to_string())?;

    Ok(ProbeArtifact {
        path: out_abs.display().to_string(),
        source: PROBE_SOURCE.to_string(),
        source_hash: sha256_hex(&source_bytes),
        binary_hash: sha256_hex(&binary_bytes),
        identity,
    })
}

/// Wrap an already-built probe: hash it, ask it what it is, and record its source.
///
/// The source hash is taken from the committed `PROBE_SOURCE` rather than from the
/// binary, because the source is the reproducible part (the binary is
/// toolchain-bound, like the kernel image).
pub fn probe_artifact(path: &str) -> Result<ProbeArtifact, PortError> {
    let root = workspace_root();
    let source_abs = root.join(PROBE_SOURCE);
    if !source_abs.is_file() {
        return Err(PortError::Execution(format!(
            "cross-implementation probe source is missing: {}",
            PROBE_SOURCE
        )));
    }
    let binary_bytes =
        std::fs::read(path).map_err(|e| PortError::Execution(format!("{}: {}", path, e)))?;
    let source_bytes = std::fs::read(&source_abs).map_err(|e| PortError::Io(format!("{}", e)))?;
    let identity = probe_identity(path)?;
    Ok(ProbeArtifact {
        path: path.to_string(),
        source: PROBE_SOURCE.to_string(),
        source_hash: sha256_hex(&source_bytes),
        binary_hash: sha256_hex(&binary_bytes),
        identity,
    })
}

/// Ask the probe which library it was compiled against.
fn probe_identity(probe: &str) -> Result<String, PortError> {
    let out = Command::new(probe)
        .arg("--which")
        .output()
        .map_err(|e| PortError::Execution(format!("running the probe failed: {}", e)))?;
    if !out.status.success() {
        return Err(PortError::Execution(format!(
            "the probe rejected --which ({})",
            out.status
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Render one corpus case in the probe's wire protocol: `case_id arg_hex …`, with
/// `-` for an explicitly empty argument.
fn case_line(case: &TestCase) -> String {
    let mut line = String::from(&case.case_id);
    for arg in &case.args {
        line.push(' ');
        if arg.is_empty() {
            line.push('-');
        } else {
            line.push_str(&hex::encode(arg));
        }
    }
    line.push('\n');
    line
}

/// Observe the whole corpus through the probe, in one process, in corpus order.
fn observe_with_probe(
    probe: &str,
    symbol: &str,
    cases: &[TestCase],
) -> Result<BTreeMap<String, String>, PortError> {
    let mut child = Command::new(probe)
        .arg(symbol)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PortError::Execution(format!("spawning the probe failed: {}", e)))?;

    {
        use std::io::Write;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| PortError::Execution(String::from("probe stdin unavailable")))?;
        for case in cases {
            stdin
                .write_all(case_line(case).as_bytes())
                .map_err(|e| PortError::Execution(format!("writing to the probe failed: {}", e)))?;
        }
    }

    let out = child
        .wait_with_output()
        .map_err(|e| PortError::Execution(format!("waiting for the probe failed: {}", e)))?;
    if !out.status.success() {
        return Err(PortError::Execution(format!(
            "the probe exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut observed: BTreeMap<String, String> = BTreeMap::new();
    for line in stdout.lines() {
        let mut parts = line.split(' ');
        let (Some(case_id), Some(output)) = (parts.next(), parts.next()) else {
            continue;
        };
        observed.insert(case_id.to_string(), output.to_string());
    }
    Ok(observed)
}

/// One case the two implementations did not agree on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossMismatch {
    pub case_id: String,
    pub primary_hex: String,
    pub secondary_hex: String,
}

/// The cross-implementation residual for one target.
///
/// Claim hygiene: the **asserted** fields are reproducible anywhere the toolchain
/// can build the probe, and the residual hash covers exactly those. Two facts are
/// environment-bound and are recorded as **observed, not asserted** (the same split
/// the boot manifest uses for the kernel image):
///
///   * the host C library's version string — it is whatever this host has;
///   * the compiled probe's hash — it depends on the musl-gcc version.
///
/// Neither is part of the claim ("two named implementations agree over the sealed
/// corpus"); both are provenance for the run that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossVerdict {
    pub target: String,
    pub dialect: String,
    pub locale_contract: String,
    /// The implementation the seal was built from.
    pub primary: String,
    pub primary_mechanism: String,
    /// The independent implementation this court adds.
    pub secondary: String,
    pub secondary_mechanism: String,
    /// The probe's own identity report — the evidence that the second implementation
    /// really is musl, produced by the probe's own preprocessor check.
    pub secondary_identity: String,
    pub probe_source: String,
    pub probe_source_hash: String,
    pub cases_run: u64,
    pub agreements: u64,
    pub disagreements: u64,
    /// The **sealed** oracle hash. Binding it here is what ties this claim to a seal.
    pub primary_oracle_hash: String,
    pub secondary_oracle_hash: String,
    pub verdict: CourtVerdict,
    /// Observed only: the host library's version (e.g. `gnu libc 2.44`).
    pub primary_version_observed: String,
    /// Observed only: the compiled probe's hash (musl-gcc-version-bound).
    pub probe_binary_hash: String,
}

impl CrossVerdict {
    /// Consistent only if the corpus is non-empty and every case agreed.
    pub fn is_consistent(&self) -> bool {
        self.cases_run > 0
            && self.disagreements == 0
            && self.agreements == self.cases_run
            && !self.primary_oracle_hash.is_empty()
            && !self.secondary_oracle_hash.is_empty()
            && !self.probe_source_hash.is_empty()
            && !self.probe_binary_hash.is_empty()
    }

    /// The asserted (reproducible) claim. The residual hash covers exactly this.
    pub fn canonical(&self) -> String {
        format!(
            "target={};dialect={};locale={};primary={};primary_mechanism={};secondary={};secondary_mechanism={};secondary_identity={};probe_source={};probe_source_hash={};cases_run={};agreements={};disagreements={};primary_oracle_hash={};secondary_oracle_hash={};verdict={}",
            self.target,
            self.dialect,
            self.locale_contract,
            self.primary,
            self.primary_mechanism,
            self.secondary,
            self.secondary_mechanism,
            self.secondary_identity,
            self.probe_source,
            self.probe_source_hash,
            self.cases_run,
            self.agreements,
            self.disagreements,
            self.primary_oracle_hash,
            self.secondary_oracle_hash,
            self.verdict.as_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self, mismatches: &[CrossMismatch]) -> String {
        let body: Vec<String> = mismatches
            .iter()
            .map(|m| {
                format!(
                    "    {{\n      \"case_id\": \"{}\",\n      \"primary_hex\": \"{}\",\n      \"secondary_hex\": \"{}\"\n    }}",
                    json_escape(&m.case_id),
                    json_escape(&m.primary_hex),
                    json_escape(&m.secondary_hex)
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.cross_implementation_verdict.v1\",\n  \"asserted\": {{\n    \"target\": \"{}\",\n    \"dialect\": \"{}\",\n    \"locale_contract\": \"{}\",\n    \"primary\": \"{}\",\n    \"primary_mechanism\": \"{}\",\n    \"secondary\": \"{}\",\n    \"secondary_mechanism\": \"{}\",\n    \"secondary_identity\": \"{}\",\n    \"probe_source\": \"{}\",\n    \"probe_source_hash\": \"{}\",\n    \"cases_run\": {},\n    \"agreements\": {},\n    \"disagreements\": {},\n    \"primary_oracle_hash\": \"{}\",\n    \"secondary_oracle_hash\": \"{}\",\n    \"verdict\": \"{}\"\n  }},\n  \"observed_not_asserted\": {{\n    \"primary_version\": \"{}\",\n    \"probe_binary_hash\": \"{}\",\n    \"reason\": \"the host library version is whatever this host has, and the probe binary depends on the musl-gcc version; neither is part of the claim\"\n  }},\n  \"claim\": \"the two named implementations agree on every case of the sealed corpus; agreement on a bounded corpus is evidence, not proof of equivalence\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(&self.target),
            json_escape(&self.dialect),
            json_escape(&self.locale_contract),
            json_escape(&self.primary),
            json_escape(&self.primary_mechanism),
            json_escape(&self.secondary),
            json_escape(&self.secondary_mechanism),
            json_escape(&self.secondary_identity),
            json_escape(&self.probe_source),
            self.probe_source_hash,
            self.cases_run,
            self.agreements,
            self.disagreements,
            self.primary_oracle_hash,
            self.secondary_oracle_hash,
            self.verdict.as_str(),
            json_escape(&self.primary_version_observed),
            self.probe_binary_hash,
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

/// Run the cross-implementation court for `target` against a compiled probe.
///
/// The primary observation goes through exactly the code path the seal uses, so the
/// recorded `primary_oracle_hash` is the sealed oracle hash. Fails closed: a probe
/// built against anything other than musl, a missing case, or a probe error is a
/// hard error rather than a silently empty comparison.
pub fn run_cross_court(
    target: &PortTarget,
    probe: &ProbeArtifact,
    auth: &PortingAuthority,
) -> Result<(CrossVerdict, Vec<CrossMismatch>), PortError> {
    if !auth.can_observe() {
        return Err(PortError::CapabilityDenied);
    }
    if probe.libc() != Some("musl") {
        return Err(PortError::Execution(format!(
            "the probe must be musl-built; it reports {}",
            probe.identity.replace('\n', " ")
        )));
    }

    let cases = cases_for(target);
    if cases.is_empty() {
        return Err(PortError::UnknownTarget(target.id.to_string()));
    }

    // Implementation A: the host C library, through exactly the sealed path.
    let primary_traces = dialect_cage::observe_target(target, &cases, auth)?;

    // Implementation B: musl, out-of-process, one corpus in one invocation.
    let observed = observe_with_probe(&probe.path, target.symbol, &cases)?;

    let mut secondary_traces: Vec<OracleTrace> = Vec::with_capacity(cases.len());
    let mut mismatches: Vec<CrossMismatch> = Vec::new();
    let mut agreements: u64 = 0;

    for (case, primary) in cases.iter().zip(primary_traces.iter()) {
        let secondary_hex = observed.get(&case.case_id).cloned().ok_or_else(|| {
            PortError::Execution(format!(
                "the probe did not answer case {} of {}",
                case.case_id, target.id
            ))
        })?;

        if secondary_hex == primary.output_hex {
            agreements += 1;
        } else {
            mismatches.push(CrossMismatch {
                case_id: case.case_id.clone(),
                primary_hex: primary.output_hex.clone(),
                secondary_hex: secondary_hex.clone(),
            });
        }

        // The secondary observation is a first-class oracle trace of its own, so the
        // two sides are hashed by the same function over the same shape.
        secondary_traces.push(OracleTrace::new(
            target,
            &case.case_id,
            &case.args,
            &hex::decode(&secondary_hex).unwrap_or_default(),
            "ok",
            &["compute"],
        ));
    }

    let cases_run = cases.len() as u64;
    let disagreements = mismatches.len() as u64;
    let verdict = if disagreements == 0 {
        CourtVerdict::Consistent
    } else {
        CourtVerdict::Inconsistent
    };

    Ok((
        CrossVerdict {
            target: target.id.to_string(),
            dialect: target.dialect.to_string(),
            locale_contract: target.locale_contract.to_string(),
            primary: Implementation::HostLibc.id().to_string(),
            primary_mechanism: Implementation::HostLibc.mechanism().to_string(),
            secondary: Implementation::Musl.id().to_string(),
            secondary_mechanism: Implementation::Musl.mechanism().to_string(),
            secondary_identity: probe.identity.replace('\n', " "),
            probe_source: probe.source.clone(),
            probe_source_hash: probe.source_hash.clone(),
            cases_run,
            agreements,
            disagreements,
            primary_oracle_hash: combined_oracle_hash(&primary_traces),
            secondary_oracle_hash: combined_oracle_hash(&secondary_traces),
            verdict,
            primary_version_observed: format!("gnu libc {}", host_libc_version()),
            probe_binary_hash: probe.binary_hash.clone(),
        },
        mismatches,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::POSIX_STRSPN;

    fn granted() -> PortingAuthority {
        PortingAuthority::granted()
    }

    /// `musl-gcc`, or `None` when the toolchain is absent (the cross court is
    /// environment-gated, like the compiler-dependent tests).
    fn musl_gcc() -> Option<()> {
        Command::new("musl-gcc")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|_| ())
    }

    fn build_probe(tag: &str) -> Option<ProbeArtifact> {
        musl_gcc()?;
        let path = std::env::temp_dir().join(format!(
            "phorensic-cross-probe-{}-{}",
            tag,
            std::process::id()
        ));
        compile_probe(&path.display().to_string()).ok()
    }

    /// The committed sealed oracle hash for a target, so a test can prove the cross
    /// verdict is bound to the seal rather than to a fresh observation.
    fn committed_oracle_hash(symbol: &str) -> String {
        let root = workspace_root();
        let path = root.join(format!(
            "phost/evidence/porting/{}/behavior_signature.json",
            symbol
        ));
        let text = std::fs::read_to_string(&path).expect("committed behavior signature");
        let doc = crate::porting::json::Json::parse(&text).expect("valid JSON");
        doc.str_or_empty("combined_oracle_hash")
    }

    fn sample() -> CrossVerdict {
        CrossVerdict {
            target: String::from("posix:strspn:c-locale:u64:v1"),
            dialect: String::from("posix"),
            locale_contract: String::from("C"),
            primary: String::from("host-libc"),
            primary_mechanism: String::from("in-process FFI (the dialect cage)"),
            secondary: String::from("musl"),
            secondary_mechanism: String::from("out-of-process statically linked probe"),
            secondary_identity: String::from("libc=musl pointer_bytes=8 int_bytes=4"),
            probe_source: String::from(PROBE_SOURCE),
            probe_source_hash: "a".repeat(64),
            cases_run: 578,
            agreements: 578,
            disagreements: 0,
            primary_oracle_hash: "c".repeat(64),
            secondary_oracle_hash: "c".repeat(64),
            verdict: CourtVerdict::Consistent,
            primary_version_observed: String::from("gnu libc 2.44"),
            probe_binary_hash: "b".repeat(64),
        }
    }

    #[test]
    fn test_cross_verdict_is_consistent_only_on_full_agreement() {
        assert!(sample().is_consistent());

        let mut one_disagreement = sample();
        one_disagreement.agreements = 577;
        one_disagreement.disagreements = 1;
        one_disagreement.verdict = CourtVerdict::Inconsistent;
        assert!(!one_disagreement.is_consistent());
    }

    #[test]
    fn test_cross_verdict_residual_covers_the_claim() {
        let h = sample().residual_hash();
        assert_eq!(h.len(), 64);

        // The second implementation's identity is part of the claim.
        let mut other_impl = sample();
        other_impl.secondary = String::from("glibc-again");
        assert_ne!(other_impl.residual_hash(), h);

        // So is the seal this claim refers to.
        let mut other_seal = sample();
        other_seal.primary_oracle_hash = "d".repeat(64);
        assert_ne!(other_seal.residual_hash(), h);

        // And the probe that produced the second observation.
        let mut other_probe = sample();
        other_probe.probe_binary_hash = "e".repeat(64);
        assert_eq!(
            other_probe.residual_hash(),
            h,
            "the probe binary hash is observed-only and must not enter the residual"
        );
    }

    #[test]
    fn test_cross_court_denies_without_capability() {
        let probe = ProbeArtifact {
            path: String::from("/nonexistent/probe"),
            source: String::from(PROBE_SOURCE),
            source_hash: "a".repeat(64),
            binary_hash: "b".repeat(64),
            identity: String::from("libc=musl"),
        };
        assert_eq!(
            run_cross_court(&POSIX_STRSPN, &probe, &PortingAuthority::none()),
            Err(PortError::CapabilityDenied)
        );
    }

    #[test]
    fn test_cross_court_rejects_a_probe_that_is_not_musl() {
        // A probe compiled against the host library adds nothing: the whole point is
        // an independent implementation, so it is refused rather than compared.
        let probe = ProbeArtifact {
            path: String::from("/nonexistent/probe"),
            source: String::from(PROBE_SOURCE),
            source_hash: "a".repeat(64),
            binary_hash: "b".repeat(64),
            identity: String::from("libc=glibc"),
        };
        assert!(run_cross_court(&POSIX_STRSPN, &probe, &granted()).is_err());
    }

    #[test]
    fn test_cross_court_rejects_a_probe_with_no_identity() {
        // `/bin/true` exits 0 but reports nothing, so the identity is absent and the
        // court refuses rather than comparing against nothing.
        let probe = probe_artifact("/bin/true").expect("wrapping /bin/true");
        assert_eq!(probe.libc(), None);
        assert!(run_cross_court(&POSIX_STRSPN, &probe, &granted()).is_err());
    }

    /// The whole point, end to end on every sealed leaf: two independent
    /// implementations agree on the entire sealed corpus, and the verdict carries
    /// the committed seal's oracle hash.
    #[test]
    fn test_cross_court_agrees_on_every_sealed_leaf() {
        let Some(probe) = build_probe("leaves") else {
            return; // no musl-gcc on this host
        };
        assert_eq!(probe.libc(), Some("musl"));
        assert_eq!(
            probe.source_hash,
            sha256_hex(&std::fs::read(workspace_root().join(PROBE_SOURCE)).unwrap())
        );

        for symbol in ["toupper", "memcmp", "memchr", "strlen", "strrchr", "strspn"] {
            let target = crate::porting::target::resolve_target(symbol).unwrap();
            let (v, mismatches) = run_cross_court(&target, &probe, &granted()).unwrap();
            assert!(
                mismatches.is_empty(),
                "{} disagreed: {:?}",
                symbol,
                mismatches.first()
            );
            assert!(v.is_consistent(), "{}: {:?}", symbol, v);
            assert_eq!(v.secondary, "musl");
            // Bound to the committed seal, not to a fresh observation.
            assert_eq!(
                v.primary_oracle_hash,
                committed_oracle_hash(symbol),
                "{}: the cross verdict does not carry the sealed oracle hash",
                symbol
            );
            // Over a corpus that does not distinguish them, the two implementations
            // produce the *same* sealed trace set, not merely the same answers.
            assert_eq!(
                v.primary_oracle_hash, v.secondary_oracle_hash,
                "{}: the two implementations' trace sets differ",
                symbol
            );
        }
    }

    #[test]
    fn test_a_silent_probe_is_an_error_not_agreement() {
        let Some(probe) = build_probe("silent") else {
            return;
        };
        let target = crate::porting::target::resolve_target("toupper").unwrap();
        // A probe that answers nothing must fail closed rather than "agree".
        let mut silent = probe.clone();
        silent.path = String::from("/bin/true");
        silent.identity = String::from("libc=musl");
        assert!(run_cross_court(&target, &silent, &granted()).is_err());
        // The honest probe does agree, so the check above is not vacuous.
        let (v, _) = run_cross_court(&target, &probe, &granted()).unwrap();
        assert!(v.is_consistent());
    }
}
