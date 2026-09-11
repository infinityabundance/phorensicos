// phorport/frf.rs — the FRF epistemic outer court (§17)
//
// Phase 7. FRF is the authority for durable, re-verifiable external evidence. A
// phorport counterexample is a HYPOTHESIS until FRF's court observes it as a
// differential and emits a receipt. This module does not reimplement FRF's
// object model: it uses FRF's own store, admission, court, receipt and claim
// commands, and retains the resulting run/receipt/claim ids **verbatim** in a
// namespace distinct from Phorensicos hashes, frf-fuzz ContentIds and Gemel Gids.
//
// The court is a differential court, exactly as frf-fuzz's is:
//
//   * the **authority** is the foreign oracle for the target, reached through a
//     generated executable (`__oracle`) that prints the oracle observable;
//   * the **candidate** is the same binary in differential mode (`__differential`)
//     with the candidate object bound, which prints the *identical* line on
//     parity and diverges (exit + stderr) when the candidate disagrees.
//
// A receipt is evidence that the court ran. A counterexample is VERIFIED only
// when the court observed a residual (the candidate diverged). A parity run still
// emits a receipt; that receipt is evidence of NON-reproduction and the outcome
// is `Failed`, preserved rather than deleted.
//
// Two honest limits, both load-bearing:
//
//   1. The authority executable is a helper around the host dialect cage. Its
//      *bytes* are what FRF admits; the implementation behind it (the host libc)
//      is provenance, recorded as observed, not asserted — the same split the
//      cross-implementation court uses.
//   2. Agreement or divergence on one fixture is evidence about that fixture.

use std::path::{Path, PathBuf};

/// The FRF store root, relative to the foundry's store root.
pub const FRF_ROOT_DIR: &str = "frf-root";
/// Bound on a retained FRF id (run/receipt/claim), verbatim.
pub const MAX_FRF_ID_LEN: usize = 512;
/// Bound on a deterministic note.
pub const MAX_NOTE_LEN: usize = 2048;

/// The outcome of one court verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// FRF emitted a receipt and the court observed a divergence.
    Verified,
    /// The court ran but did not reproduce the counterexample as a differential,
    /// or FRF refused. The note preserves why.
    Failed,
}

impl VerificationOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationOutcome::Verified => "verified",
            VerificationOutcome::Failed => "failed",
        }
    }
}

/// The authority (reference) configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoritySpec {
    pub name: String,
    pub version: String,
    /// The executable FRF admits and executes as the reference.
    pub path: PathBuf,
}

/// The court question binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CourtQuestion {
    pub id: String,
    pub question: String,
    pub falsifier: String,
    pub fixture_family: String,
}

/// The result of one FRF court.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CourtOutcome {
    pub outcome: VerificationOutcome,
    /// The FRF run id, verbatim, when the court executed.
    pub run: Option<String>,
    /// The FRF receipt id, verbatim, when emitted.
    pub receipt: Option<String>,
    /// The FRF claim id, verbatim, when compiled.
    pub claim: Option<String>,
    /// Why the outcome is not `Verified`, when it is not.
    pub note: Option<String>,
}

impl CourtOutcome {
    /// Is the counterexample verified by a receipt with an observed residual?
    pub fn is_verified(&self) -> bool {
        self.outcome == VerificationOutcome::Verified && self.receipt.is_some()
    }
}

/// Bound a free text deterministically, keeping a content tail so distinct long
/// texts never collapse.
pub fn bound_text(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let tail = crate::compile::sha256_hex(s.as_bytes());
    let tail = &tail[..16];
    let cut = limit.saturating_sub(tail.len() + 3);
    let mut head = s.as_bytes()[..cut.min(s.len())].to_vec();
    while !head.is_empty() && std::str::from_utf8(&head).is_err() {
        head.pop();
    }
    let mut out = String::from_utf8(head).unwrap_or_default();
    out.push('…');
    out.push_str(tail);
    out
}

/// Quote a string as a YAML double-quoted scalar with the documented escapes.
pub fn yaml_q(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The platform string FRF expects.
pub fn platform_string() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

fn validate_id_component(s: &str) -> Result<(), String> {
    if s.is_empty() || s == "." || s == ".." {
        return Err(String::from("authority name/version must be non-empty"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(String::from(
            "authority name/version must match [A-Za-z0-9._-]",
        ));
    }
    Ok(())
}

fn validate_question(q: &CourtQuestion) -> Result<(), String> {
    if q.id.is_empty()
        || q.id.len() > 128
        || q.id == "."
        || q.id == ".."
        || q.id.contains("::")
        || !q
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') || c == ':')
    {
        return Err(String::from(
            "question id must be 1..=128 chars of [A-Za-z0-9._:-] and must not contain '::'",
        ));
    }
    if q.question.len() > 8192 || q.falsifier.len() > 8192 {
        return Err(String::from("question/falsifier too long (max 8192)"));
    }
    if q.fixture_family.is_empty() || q.fixture_family.len() > 512 {
        return Err(String::from("fixture family must be 1..=512 chars"));
    }
    Ok(())
}

/// Emit the court manifest YAML. Both sides receive the fixture arguments; the
/// role-specific configuration (target, object, hash) is baked into the
/// generated executable scripts, so there is no environment to trust.
pub fn emit_manifest(
    question: &CourtQuestion,
    authority_id: &str,
    candidate_path: &Path,
    fixture_path: &Path,
    platform: &str,
) -> String {
    let mut out = String::new();
    out.push_str("court:\n");
    out.push_str(&format!("  id: {}\n", yaml_q(&question.id)));
    out.push_str(&format!("  question: {}\n", yaml_q(&question.question)));
    out.push_str(&format!("  falsifier: {}\n", yaml_q(&question.falsifier)));
    out.push_str(&format!("  authority: {}\n", yaml_q(authority_id)));
    out.push_str("  candidate:\n");
    out.push_str("    name: phorport-candidate\n");
    out.push_str("    version_or_commit: \"1.0\"\n");
    out.push_str("    build_profile: release\n");
    out.push_str(&format!(
        "    path: {}\n",
        yaml_q(&candidate_path.to_string_lossy())
    ));
    out.push_str("  fixture:\n");
    out.push_str("    id: phorport-input\n");
    out.push_str(&format!(
        "    path: {}\n",
        yaml_q(&fixture_path.to_string_lossy())
    ));
    out.push_str("    arguments:\n");
    out.push_str(&format!("      - {}\n", yaml_q("--frf-fuzz-fixture")));
    out.push_str("      - \"{fixture}\"\n");
    out.push_str("  admissibility_envelope:\n");
    out.push_str(&format!(
        "    fixture_family: {}\n",
        yaml_q(&question.fixture_family)
    ));
    out.push_str("    platforms:\n");
    out.push_str(&format!("      - {}\n", yaml_q(platform)));
    out.push_str("    observables:\n");
    out.push_str("      - exit\n");
    out.push_str("      - stderr\n");
    out.push_str("    normalizers: []\n");
    out.push_str("    replay_scope: single-run\n");
    out
}

fn open_frf_store(store_root: &Path) -> Result<frf::store::Store, String> {
    let root = std::path::absolute(store_root.join(FRF_ROOT_DIR)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let store = frf::store::Store::new(root);
    store.ensure_tree().map_err(|e| e.to_string())?;
    Ok(store)
}

fn admit_or_verify(
    store: &frf::store::Store,
    authority: &AuthoritySpec,
    authority_id: &str,
) -> Result<String, String> {
    let record_path = store
        .authority_path(authority_id)
        .map_err(|e| format!("FRF authority path error: {e}"))?;
    if !record_path.exists() {
        return frf::commands::admit::run(
            store,
            &authority.path,
            &authority.name,
            &authority.version,
            "executable_reference",
        )
        .map_err(|e| format!("FRF admission failed: {e}"));
    }
    let rec = store
        .load_authority(authority_id)
        .map_err(|e| format!("FRF authority load failed: {e}"))?;
    let sha = frf::host::sha256_file(&authority.path)
        .map_err(|e| format!("cannot hash authority: {e}"))?;
    if rec.executable_sha256 != sha {
        return Err(String::from(
            "authority bytes changed since admission; admission is once — admit the changed oracle as a new version",
        ));
    }
    if rec.path != authority.path.to_string_lossy() {
        return Err(String::from(
            "authority path changed since admission; re-admit with the original path or bump the version",
        ));
    }
    Ok(authority_id.to_string())
}

fn compile_baseline_claim(
    store: &frf::store::Store,
    run: &str,
    receipt: &str,
) -> Result<Option<String>, String> {
    use frf::cli::ClosureArg;
    use frf::commands::{claim, dispose};
    let capture = store
        .load_capture(run)
        .map_err(|e| format!("cannot read FRF capture: {e}"))?
        .into_inner();
    for rid in &capture.residuals {
        dispose::run(
            store,
            rid,
            ClosureArg::Intentional,
            "phorport: the divergence is the verified counterexample (candidate diverges where the reference does not); disposed to compile the baseline claim",
            None,
            None,
            None,
            None,
        )
        .map_err(|e| format!("FRF disposition failed for {rid}: {e}"))?;
    }
    claim::run(
        store,
        std::slice::from_ref(&receipt.to_string()),
        false,
        frf::model::CLAIM_POLICY_BASELINE,
        "",
        &[],
    )
    .map_err(|e| format!("FRF claim refused: {e}"))?;
    Ok(store
        .claim_ids_for_receipt(receipt)
        .map_err(|e| format!("FRF claim index error: {e}"))?
        .first()
        .cloned())
}

/// A generated executable bound to a role (candidate or authority).
fn write_script(path: &Path, program: &Path, args: &[String]) -> Result<(), String> {
    let mut body = String::from("#!/bin/sh\n");
    body.push_str("exec ");
    body.push_str(&shell_quote(&program.to_string_lossy()));
    for a in args {
        body.push(' ');
        body.push_str(&shell_quote(a));
    }
    body.push_str(" \"$@\"\n");
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Minimal POSIX shell quoting.
fn shell_quote(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Run an FRF differential court for one counterexample.
///
/// `program` is the phorport binary (the helper host); `candidate_args` are its
/// differential-mode arguments (target, object path, object hash); the authority
/// script is generated from `target_id` alone.
#[allow(clippy::too_many_arguments)]
pub fn run_counterexample_court(
    store_root: &Path,
    program: &Path,
    target_id: &str,
    symbol: &str,
    candidate_args: &[String],
    fixture_bytes: &[u8],
    with_claim: bool,
) -> Result<CourtOutcome, String> {
    let store = open_frf_store(store_root)?;
    let frf_root = std::path::absolute(store_root.join(FRF_ROOT_DIR)).map_err(|e| e.to_string())?;

    // The court id must be path-safe and stable per target.
    let court_symbol: String = symbol
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let court_dir = frf_root.join("phorport-courts").join(&court_symbol);
    std::fs::create_dir_all(&court_dir).map_err(|e| e.to_string())?;

    // Generated executables.
    let candidate_script = court_dir.join("candidate.sh");
    let authority_script = court_dir.join("authority.sh");
    write_script(&candidate_script, program, candidate_args)?;
    write_script(
        &authority_script,
        program,
        &[String::from("__oracle"), target_id.to_string()],
    )?;

    // Admit the authority (the oracle helper).
    let authority = AuthoritySpec {
        name: String::from("phorensic-oracle"),
        version: court_symbol.clone(),
        path: authority_script.clone(),
    };
    validate_id_component(&authority.name)?;
    validate_id_component(&authority.version)?;
    let authority_id = format!("{}-{}", authority.name, authority.version);
    let _admitted = admit_or_verify(&store, &authority, &authority_id)?;

    // Stage the fixture and manifest.
    let fixture_path = court_dir.join("fixture.bin");
    std::fs::write(&fixture_path, fixture_bytes).map_err(|e| e.to_string())?;
    let question = CourtQuestion {
        id: format!("phorport-{court_symbol}"),
        question: format!(
            "For the same input, does the candidate port of {symbol} terminate exactly as the reference oracle terminates, over the declared observables?"
        ),
        falsifier: format!(
            "The candidate's observable behavior for {symbol} diverges from the reference on this input."
        ),
        fixture_family: String::from("phorport-input"),
    };
    validate_question(&question)?;
    let manifest_path = court_dir.join("manifest.yaml");
    let manifest = emit_manifest(
        &question,
        &authority_id,
        &candidate_script,
        &fixture_path,
        &platform_string(),
    );
    std::fs::write(&manifest_path, manifest).map_err(|e| e.to_string())?;

    // Run the court. The manifest uses absolute paths everywhere, so no process
    // cwd mutation is needed; FRF's environment digest then records the caller's
    // (stable) cwd rather than a globally-pinned one — which would race with
    // concurrent work in the same process.
    let run = match frf::commands::court::run_once(&store, &manifest_path, None, None, true, None) {
        Ok(run) => run,
        Err(e) => {
            return Ok(CourtOutcome {
                outcome: VerificationOutcome::Failed,
                run: None,
                receipt: None,
                claim: None,
                note: Some(bound_text(&format!("FRF court refused: {e}"), MAX_NOTE_LEN)),
            })
        }
    };

    // Emit the receipt.
    let receipt = match frf::commands::receipt::run(&store, &run) {
        Ok(r) => r,
        Err(e) => {
            return Ok(CourtOutcome {
                outcome: VerificationOutcome::Failed,
                run: Some(run),
                receipt: None,
                claim: None,
                note: Some(bound_text(
                    &format!("FRF receipt refused: {e}"),
                    MAX_NOTE_LEN,
                )),
            })
        }
    };

    // A receipt alone is not verification: a parity run (zero residuals) emits a
    // receipt but did not reproduce the counterexample.
    let capture = store
        .load_capture(&run)
        .map_err(|e| format!("cannot read FRF capture: {e}"))?
        .into_inner();
    if capture.residuals.is_empty() {
        return Ok(CourtOutcome {
            outcome: VerificationOutcome::Failed,
            run: Some(run),
            receipt: Some(receipt),
            claim: None,
            note: Some(bound_text(
                "no divergence under the FRF harness: the candidate behaved like the reference on this input; the parity receipt is preserved as evidence of non-reproduction",
                MAX_NOTE_LEN,
            )),
        });
    }

    // Optional claim.
    let mut claim = None;
    if with_claim {
        match compile_baseline_claim(&store, &run, &receipt) {
            Ok(Some(c)) => claim = Some(c),
            Ok(None) => {}
            Err(e) => {
                return Ok(CourtOutcome {
                    outcome: VerificationOutcome::Verified,
                    run: Some(run),
                    receipt: Some(receipt),
                    claim: None,
                    note: Some(bound_text(
                        &format!("claim not compiled: {e}"),
                        MAX_NOTE_LEN,
                    )),
                })
            }
        }
    }

    Ok(CourtOutcome {
        outcome: VerificationOutcome::Verified,
        run: Some(run),
        receipt: Some(receipt),
        claim,
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_quoting_escapes_hostile_text() {
        assert_eq!(yaml_q("plain"), "\"plain\"");
        assert_eq!(yaml_q("a\"b\\c\nd\te\rf"), "\"a\\\"b\\\\c\\nd\\te\\rf\"");
        assert_eq!(yaml_q("\u{1}"), "\"\\u0001\"");
        assert_eq!(yaml_q("{fixture}"), "\"{fixture}\"");
    }

    #[test]
    fn test_manifest_parses_with_frfs_own_deserializer() {
        let q = CourtQuestion {
            id: String::from("phorport-strspn"),
            question: String::from("line one\nline two: with \"quotes\" and \\ backslashes"),
            falsifier: String::from("diverges \u{1b}"),
            fixture_family: String::from("phorport-input"),
        };
        let yaml = emit_manifest(
            &q,
            "phorensic-oracle-strspn",
            Path::new("/abs/candidate.sh"),
            Path::new("/abs/fixture.bin"),
            "x86_64-linux",
        );
        let dir =
            std::env::temp_dir().join(format!("phorport-frf-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fs = frf::store::Store::new(dir.clone());
        fs.ensure_tree().unwrap();
        let path = dir.join("manifest.yaml");
        std::fs::write(&path, &yaml).unwrap();
        let m: frf::model::CourtManifest = fs.parse_yaml(&path).unwrap();
        assert_eq!(m.court.id, "phorport-strspn");
        assert_eq!(m.court.authority, "phorensic-oracle-strspn");
        assert_eq!(m.court.candidate.name, "phorport-candidate");
        assert_eq!(
            m.court.fixture.arguments,
            vec!["--frf-fuzz-fixture", "{fixture}"]
        );
        assert_eq!(m.court.admissibility_envelope.replay_scope, "single-run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bound_text_is_deterministic_and_distinct() {
        let a = "x".repeat(5000);
        let b = format!("{}y", "x".repeat(4999));
        assert_ne!(bound_text(&a, 1000), bound_text(&b, 1000));
        assert_eq!(bound_text(&a, 1000), bound_text(&a, 1000));
        assert_eq!(bound_text("short", 1000), "short");
    }

    #[test]
    fn test_shell_quote_handles_quotes() {
        assert_eq!(shell_quote("/a/b"), "'/a/b'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    fn prep_path() {
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));
    }

    use crate::config::HarnessConfig;

    fn phorport_bin() -> PathBuf {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug/phorport");
        assert!(p.is_file(), "build phorport first: {}", p.display());
        p
    }

    /// End-to-end: FRF verifies a real counterexample (defective candidate)
    /// and preserves the parity receipt when the candidate matches.
    #[test]
    fn test_frf_court_verifies_a_counterexample_and_preserves_parity() {
        prep_path();
        use phost::porting::target::resolve_target;
        let target = resolve_target("strspn").expect("target");
        let correct = std::fs::read_to_string("../examples/jit_port_strspn.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn.phor"))
            .expect("correct source");
        let defective = std::fs::read_to_string("../examples/jit_port_strspn_xor_lane.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn_xor_lane.phor"))
            .expect("defective source");
        let work = std::env::temp_dir().join(format!("phorport-frf-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let bad = crate::compile::compile_source("strspn", &defective, &work.join("bad"))
            .expect("compile bad");
        let good = crate::compile::compile_source("strspn", &correct, &work.join("good"))
            .expect("compile good");
        let bad_config = HarnessConfig {
            target_id: target.id.to_string(),
            candidate_path: bad.object_path.clone(),
            candidate_hash: bad.object_hash.clone(),
        };

        // Find a deterministic fixture on which the XOR-lane defect diverges.
        let mut fixture: Option<Vec<u8>> = None;
        'outer: for (s, accept) in [
            (vec![0x61u8, 0x61, 0x00], vec![0x61u8, 0x61]),
            (vec![0x62u8, 0x61, 0x00], vec![0x61u8, 0x61]),
            (vec![0x61u8, 0x62, 0x00], vec![0x61u8, 0x62, 0x61]),
            (vec![0x61u8, 0x61, 0x61, 0x00], vec![0x61u8, 0x61]),
        ] {
            let n = s.len();
            let args = vec![s, accept, (n as u64).to_le_bytes().to_vec()];
            if !crate::harness::probe_args(&bad_config, &args).matched {
                let data = crate::case::encode_args(&target, &args);
                // Confirm the encoded fixture decodes back to a divergence.
                if !crate::harness::probe(&bad_config, &data).matched {
                    fixture = Some(data);
                    break 'outer;
                }
            }
        }
        let fixture = fixture.expect("a divergent fixture for the XOR-lane defect");

        let store = work.join("store");
        let program = phorport_bin();
        let bad_args = vec![
            String::from("__differential"),
            target.id.to_string(),
            bad.object_path.clone(),
            bad.object_hash.clone(),
        ];
        let out = run_counterexample_court(
            &store, &program, target.id, "strspn", &bad_args, &fixture, false,
        )
        .expect("court runs");
        assert!(
            out.is_verified(),
            "defective candidate must verify: {out:?}"
        );
        assert!(out.receipt.as_deref().unwrap_or("").starts_with("receipt-"));

        // Parity: the correct candidate reproduces the oracle exactly, so the
        // court emits a receipt but the outcome is Failed (non-reproduction).
        let good_args = vec![
            String::from("__differential"),
            target.id.to_string(),
            good.object_path.clone(),
            good.object_hash.clone(),
        ];
        let out2 = run_counterexample_court(
            &store, &program, target.id, "strspn", &good_args, &fixture, false,
        )
        .expect("court runs");
        assert_eq!(out2.outcome, VerificationOutcome::Failed, "{out2:?}");
        assert!(out2.receipt.is_some(), "the parity receipt is preserved");

        let _ = std::fs::remove_dir_all(&work);
    }
}
