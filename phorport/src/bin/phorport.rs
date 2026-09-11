// phorport — the host-only autonomous porting foundry CLI
//
//   phorport probe  <symbol> --candidate-src <path> --data <hex>
//   phorport corpus <symbol> --candidate-src <path>
//   phorport fuzz   <symbol> --candidate-src <path> --frf-fuzz-bin <path>
//                            --frf-fuzz-root <path> [--workspace <path>]
//                            [--max-time <secs>] [--out <dir>]
//
// `--candidate-src` is a `.phor` source compiled with `phorc` into a work object;
// `probe`/`corpus` compare it against the foreign oracle directly, and `fuzz`
// drives an FRF-Fuzz campaign around the same comparison.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use phost::porting::compiled::compile_candidate;
use phost::porting::target::{resolve_target, PortTarget};

use phorport::config::HarnessConfig;
use phorport::explore;
use phorport::harness::probe;

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Every value following `flag` (for repeatable options such as `--source`).
fn arg_values(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            if let Some(v) = args.get(i + 1) {
                out.push(v.clone());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn work_dir(workspace: &Path, symbol: &str) -> PathBuf {
    workspace.join(".phorport").join(symbol)
}

/// Compile `candidate_src` as `symbol`'s ABI and return `(path, hash)`.
fn compile(
    symbol: &str,
    candidate_src: &Path,
    workspace: &Path,
) -> Result<(String, String), String> {
    let base = resolve_target(symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
    // `PortTarget` carries a `&'static str` source; this is a one-shot CLI, so
    // leaking the path is the honest way to hand the compiler a custom source.
    let src_static: &'static str = Box::leak(
        candidate_src
            .to_string_lossy()
            .into_owned()
            .into_boxed_str(),
    );
    let target = PortTarget {
        candidate_source: src_static,
        ..base
    };
    let dir = work_dir(workspace, symbol);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let compiled = compile_candidate(&target, dir.to_str().unwrap_or(".phorport"), None)
        .map_err(|e| format!("{}", e.message()))?;
    let path = fs_canonical(&compiled.object_path);
    Ok((path, compiled.object_hash))
}

fn fs_canonical(p: &str) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| PathBuf::from(p))
        .display()
        .to_string()
}

fn command(args: &[String]) -> Result<i32, String> {
    let Some(cmd) = args.first() else {
        return Err(String::from(
            "usage: phorport <probe|corpus|fuzz|history|campaign|autonomy|store|demand> <symbol> --candidate-src <path> …",
        ));
    };

    // `store generations` — build the genesis generation from the committed
    // index and optionally publish a child generation.
    if cmd == "store" {
        use phost::porting::autonomous_seal::SealProfile;
        use phost::porting::ident::EvidenceClosureId;
        if args.get(1).map(String::as_str) != Some("generations") {
            return Err(String::from("usage: phorport store generations [--publish-target <id> --publish-artifact <hash> --closure <id>]"));
        }
        let index = phost::porting::store::load_default().map_err(|e| e.to_string())?;
        let g0 = phorport::generations::genesis_from_index(&index)?;
        println!("generation 0: {}", g0.generation_id.as_str());
        println!("  parent:  (genesis)");
        println!("  entries: {}", g0.entries.len());
        println!("  closure: {}", g0.evidence_closure.as_str());
        if let (Some(t), Some(a)) = (
            arg_value(args, "--publish-target"),
            arg_value(args, "--publish-artifact"),
        ) {
            let closure = EvidenceClosureId::new(arg_value(args, "--closure").unwrap_or_default());
            let g1 = phorport::generations::publish_port(
                &g0,
                &t,
                &a,
                SealProfile::AutonomousV1,
                "phor.autonomous:receipt",
                closure,
            )?;
            println!("generation 1: {}", g1.generation_id.as_str());
            println!("  parent:  {}", g0.generation_id.as_str());
            println!("  entries: {}", g1.entries.len());
            println!("  closure: {}", g1.evidence_closure.as_str());
        }
        return Ok(0);
    }

    // `demand rank <surface>…` — deterministic prioritization of runtime misses.
    if cmd == "demand" {
        use phost::porting::demand::PortDemand;
        if args.get(1).map(String::as_str) != Some("rank") {
            return Err(String::from("usage: phorport demand rank <surface>…"));
        }
        let surfaces: Vec<String> = args
            .iter()
            .skip(2)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        if surfaces.is_empty() {
            return Err(String::from("demand rank needs at least one surface id"));
        }
        fn zero(_: &str) -> u64 {
            0
        }
        let empty: Vec<String> = Vec::new();
        let ctx = phorport::demand::RankContext {
            sealed_fanin: &zero,
            sealed_dependencies: &empty,
            historical_failures: &zero,
        };
        let demands: Vec<PortDemand> = surfaces
            .iter()
            .map(|s| PortDemand {
                requested_surface: s.clone(),
                callsite_family: String::from("dispatch"),
                demand_count: 1,
                store_generation: String::from("current"),
                available_dependencies: Vec::new(),
            })
            .collect();
        let scores = phorport::demand::rank(&demands, &ctx);
        let body: Vec<String> = scores.iter().map(|s| s.to_json()).collect();
        println!("[\n{}\n]", body.join(",\n"));
        return Ok(0);
    }
    let symbol = args
        .get(1)
        .cloned()
        .ok_or_else(|| String::from("missing <symbol>"))?;
    let workspace =
        PathBuf::from(arg_value(args, "--workspace").unwrap_or_else(|| String::from(".")));

    // `campaign` runs the bounded CEGIS loop; it consumes producer *sources*.
    if cmd == "campaign" {
        use phorport::cegis::{run_cegis, CampaignManifest, CampaignState, DiscoveryHook};
        use phorport::producer::{KnownCounterexample, ScriptedProducer};

        let target = resolve_target(&symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
        let spec = phost::porting::portspec::by_target_id(target.id)
            .ok_or_else(|| format!("no PortSpec for {}", target.id))?;
        let source_paths = arg_values(args, "--source");
        if source_paths.is_empty() {
            return Err(String::from("campaign needs at least one --source <path>"));
        }
        let mut sources = Vec::new();
        for p in &source_paths {
            sources.push(std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?);
        }
        let mut producer = ScriptedProducer::new(sources);

        let frf_fuzz_bin = arg_value(args, "--frf-fuzz-bin");
        let frf_fuzz_root = arg_value(args, "--frf-fuzz-root");
        let max_time = arg_value(args, "--max-time")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
        let out = arg_value(args, "--out").unwrap_or_else(|| {
            workspace
                .join("phost/evidence/phorport")
                .display()
                .to_string()
        });
        let phorport_root = phorport_root(&workspace);
        let work = workspace.join(".phorport/campaign").join(&symbol);
        let fuzz_root = workspace.join(".phorport/fuzz");

        // The production discovery hook is Phase 4's bounded FRF-Fuzz campaign.
        let mut hook = |cfg: &HarnessConfig| -> Vec<KnownCounterexample> {
            let (Some(bin), Some(root)) = (&frf_fuzz_bin, &frf_fuzz_root) else {
                return Vec::new();
            };
            match explore::campaign(
                cfg,
                &target,
                &symbol,
                None,
                "campaign",
                &fuzz_root,
                Path::new(root),
                &phorport_root,
                Path::new(bin),
                max_time,
                &out,
            ) {
                Ok(o) => o
                    .counterexample
                    .map(|cx| {
                        vec![KnownCounterexample {
                            residual: cx.minimal_residual,
                            input_hex: cx.minimal_hex,
                            oracle_hex: cx.oracle_hex,
                            candidate_hex: cx.candidate_output_hex,
                        }]
                    })
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };
        let discovery: Option<DiscoveryHook> = if frf_fuzz_bin.is_some() && frf_fuzz_root.is_some()
        {
            Some(&mut hook)
        } else {
            None
        };

        let base = HarnessConfig {
            target_id: target.id.to_string(),
            candidate_path: String::new(),
            candidate_hash: String::new(),
        };
        let manifest = CampaignManifest {
            target_id: target.id.to_string(),
            port_spec_id: "PortSpec".to_string(),
            design_generator: "registry".to_string(),
            qualification_policy: "host-observed".to_string(),
            discovery_budget: arg_value(args, "--budget")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(4),
            challenge_required: true,
            seal_profile: "autonomous-v1".to_string(),
        };

        let report = run_cegis(
            &mut producer,
            spec,
            &target,
            &base,
            &work,
            &manifest,
            discovery,
        );
        println!("=== Candidate CEGIS Campaign ===");
        println!("manifest:  {}", report.manifest_id);
        println!("revisions: {}", report.revisions);
        for line in &report.revision_log {
            println!("  {line}");
        }
        match &report.frozen {
            Some(id) => {
                println!("frozen:    {}", id.canonical());
                println!("state:     {}", CampaignState::CandidateFrozen.as_str());
            }
            None => println!(
                "state:     {}",
                report
                    .states
                    .last()
                    .map(|s| s.as_str())
                    .unwrap_or("unknown")
            ),
        }
        return Ok(if report.frozen.is_some() { 0 } else { 1 });
    }

    // `autonomy` runs the full Phase 7 pipeline: CEGIS → qualification →
    // challenge → multi-oracle → FRF → execution → dispatch → AUTONOMOUS-SEAL.
    if cmd == "autonomy" {
        use phorport::cegis::{CampaignManifest, DiscoveryHook};
        use phorport::pipeline::{run_autonomous, AutonomyInputs};
        use phorport::producer::{KnownCounterexample, ScriptedProducer};

        let target = resolve_target(&symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
        let spec = phost::porting::portspec::by_target_id(target.id)
            .ok_or_else(|| format!("no PortSpec for {}", target.id))?;
        let source_paths = arg_values(args, "--source");
        if source_paths.is_empty() {
            return Err(String::from("autonomy needs at least one --source <path>"));
        }
        let mut sources = Vec::new();
        for p in &source_paths {
            sources.push(std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?);
        }
        let mut producer = ScriptedProducer::new(sources);

        let frf_fuzz_bin = arg_value(args, "--frf-fuzz-bin");
        let frf_fuzz_root = arg_value(args, "--frf-fuzz-root");
        let max_time = arg_value(args, "--max-time")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
        let out_dir = PathBuf::from(arg_value(args, "--out").unwrap_or_else(|| {
            workspace
                .join("phost/evidence/phorport/autonomy")
                .join(&symbol)
                .display()
                .to_string()
        }));
        std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        let phorport_root = phorport_root(&workspace);
        let work = workspace.join(".phorport/autonomy").join(&symbol);
        let fuzz_root = workspace.join(".phorport/fuzz");
        let frf_store = arg_value(args, "--frf-store")
            .map(PathBuf::from)
            .unwrap_or_else(|| work.join("frf"));
        let program = std::env::current_exe().map_err(|e| e.to_string())?;

        let mut hook = |cfg: &HarnessConfig| -> Vec<KnownCounterexample> {
            let (Some(bin), Some(root)) = (&frf_fuzz_bin, &frf_fuzz_root) else {
                return Vec::new();
            };
            match explore::campaign(
                cfg,
                &target,
                &symbol,
                None,
                "autonomy",
                &fuzz_root,
                Path::new(root),
                &phorport_root,
                Path::new(bin),
                max_time,
                out_dir.to_str().unwrap_or("out"),
            ) {
                Ok(o) => o
                    .counterexample
                    .map(|cx| {
                        vec![KnownCounterexample {
                            residual: cx.minimal_residual,
                            input_hex: cx.minimal_hex,
                            oracle_hex: cx.oracle_hex,
                            candidate_hex: cx.candidate_output_hex,
                        }]
                    })
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };
        let discovery: Option<DiscoveryHook> = if frf_fuzz_bin.is_some() && frf_fuzz_root.is_some()
        {
            Some(&mut hook)
        } else {
            None
        };

        let inputs = AutonomyInputs {
            spec,
            target: target.clone(),
            base_config: HarnessConfig {
                target_id: target.id.to_string(),
                candidate_path: String::new(),
                candidate_hash: String::new(),
            },
            work_root: work.clone(),
            frf_store_root: frf_store,
            program,
            manifest: CampaignManifest {
                target_id: target.id.to_string(),
                port_spec_id: spec.id(),
                design_generator: String::from("registry"),
                qualification_policy: phost::porting::portspec::qualification_name(
                    spec.qualification,
                )
                .to_string(),
                discovery_budget: arg_value(args, "--budget")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(4),
                challenge_required: true,
                seal_profile: String::from("autonomous-v1"),
            },
            memory: None,
        };

        let report = run_autonomous(&inputs, &mut producer, discovery)
            .map_err(|e| format!("autonomy: {e}"))?;

        std::fs::write(out_dir.join("campaign_log.txt"), report.log.join("\n"))
            .map_err(|e| e.to_string())?;
        if let Some(q) = &report.qualification {
            std::fs::write(
                out_dir.join("qualification_receipt.json"),
                q.receipt.to_json(),
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(i) = &report.isolation {
            std::fs::write(
                out_dir.join("isolation_report.json"),
                format!(
                    "{{\n  \"schema\": \"phorensic.phorport.isolation_report.v1\",\n  \"workspace\": \"{}\",\n  \"markers_checked\": {},\n  \"universe_built_after_freeze\": {},\n  \"clean\": {},\n  \"leaks\": [{}]\n}}\n",
                    i.workspace_root,
                    i.markers_checked,
                    i.universe_built_after_freeze,
                    i.clean(),
                    i.leaks
                        .iter()
                        .map(|l| format!("\"{}\"", l.replace('"', "'")))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(m) = &report.multi_oracle {
            std::fs::write(
                out_dir.join("multi_oracle_verdict.json"),
                m.verdict.to_json(),
            )
            .map_err(|e| e.to_string())?;
        }

        println!("=== Autonomous Porting Campaign ===");
        println!("manifest: {}", report.manifest_id);
        for line in &report.log {
            println!("  {line}");
        }
        return match &report.seal {
            Some(seal) => {
                std::fs::write(
                    out_dir.join("autonomous_promotion_receipt.json"),
                    seal.to_json(),
                )
                .map_err(|e| e.to_string())?;
                println!("sealed: {}", seal.profile.as_str());
                println!("closure: {}", seal.closure.as_str());
                println!("receipt: {}", seal.residual_hash);
                println!("evidence: {}", out_dir.display());
                Ok(0)
            }
            None => {
                let refusal = report
                    .seal_refusal
                    .map(|r| r.describe())
                    .unwrap_or_else(|| String::from("no candidate frozen"));
                std::fs::write(
                    out_dir.join("seal_refusal.json"),
                    format!(
                        "{{\n  \"schema\": \"phorensic.phorport.seal_refusal.v1\",\n  \"reason\": \"{}\"\n}}\n",
                        refusal.replace('"', "'")
                    ),
                )
                .map_err(|e| e.to_string())?;
                println!("refused: {refusal}");
                Ok(1)
            }
        };
    }

    // `ablation` runs the controlled experiment arms and needs no candidate.
    if cmd == "ablation" {
        let budget = arg_value(args, "--budget")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(256);
        let reports =
            phorport::ablation::run(&symbol, budget).map_err(|e| format!("ablation: {e}"))?;
        let body: Vec<String> = reports.iter().map(|r| r.to_json()).collect();
        let json = format!(
            "{{\n  \"schema\": \"phorensic.phorport.ablation.v1\",\n  \"target\": \"{}\",\n  \"seed\": \"{:016x}\",\n  \"arms\": [\n{}\n  ]\n}}\n",
            symbol,
            phorport::ablation::ABLATION_SEED,
            body.join(",\n")
        );
        if let Some(out) = arg_value(args, "--out") {
            std::fs::write(&out, &json).map_err(|e| e.to_string())?;
        }
        print!("{json}");
        return Ok(0);
    }

    // `history` reads Gemel memory and needs no candidate object.
    if cmd == "history" {
        let target = resolve_target(&symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
        let memory = match arg_value(args, "--gemel") {
            Some(p) => phorport::memory::PortMemory::at(Path::new(&p))
                .map_err(|e| format!("gemel: {e}"))?,
            None => match phorport::memory::PortMemory::open(&workspace) {
                Some(m) => m,
                None => {
                    println!("no Gemel repository (standalone mode)");
                    return Ok(0);
                }
            },
        };
        let records = memory.history(target.id);
        println!("memory root: {}", memory.root().display());
        println!("records for {}: {}", target.id, records.len());
        for r in records {
            println!("  {} {} {} ({})", r.kind, r.residual, r.gid, r.detail);
        }
        return Ok(0);
    }

    let candidate_src = arg_value(args, "--candidate-src")
        .ok_or_else(|| String::from("missing --candidate-src"))?;
    let candidate_src = PathBuf::from(candidate_src);

    let (candidate_path, candidate_hash) = compile(&symbol, &candidate_src, &workspace)?;
    let target = resolve_target(&symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
    let config = HarnessConfig {
        target_id: target.id.to_string(),
        candidate_path,
        candidate_hash,
    };

    match cmd.as_str() {
        "probe" => {
            let data = arg_value(args, "--data").unwrap_or_default();
            let bytes = hex::decode(&data).unwrap_or_default();
            let o = probe(&config, &bytes);
            println!("target:      {}", config.target_id);
            println!("valid:       {}", o.valid);
            println!("matched:     {}", o.matched);
            println!("residual:    {}", o.residual);
            println!("distance:    {}", o.distance);
            println!("oracle:      {}", o.oracle_hex);
            println!("candidate:   {}", o.candidate_hex);
            Ok(if o.valid && !o.matched { 1 } else { 0 })
        }
        "corpus" => {
            let (ran, diverged) = explore::design_corpus_matches(&config, &target);
            println!("target:            {}", config.target_id);
            println!("design cases run:  {}", ran);
            println!("diverged:          {}", diverged);
            println!(
                "design corpus {} the defect",
                if diverged == 0 { "MISSES" } else { "catches" }
            );
            Ok(0)
        }
        "fuzz" => {
            let frf_fuzz_bin = arg_value(args, "--frf-fuzz-bin")
                .ok_or_else(|| String::from("missing --frf-fuzz-bin"))?;
            let frf_fuzz_root = arg_value(args, "--frf-fuzz-root")
                .ok_or_else(|| String::from("missing --frf-fuzz-root"))?;
            let max_time = arg_value(args, "--max-time")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30);
            let out = arg_value(args, "--out").unwrap_or_else(|| {
                workspace
                    .join("phost/evidence/phorport")
                    .display()
                    .to_string()
            });
            let phorport_root = phorport_root(&workspace);
            let fuzz_root = workspace.join(".phorport/fuzz");
            std::fs::create_dir_all(&fuzz_root).map_err(|e| e.to_string())?;

            // The candidate's **source** identity is the memory key: an identical
            // source that already failed is not new work.
            let source_hash = sha256_file(&candidate_src)?;

            // Long-term memory: an explicit `--gemel <path>` (created if absent) or
            // a repository discovered by walking up from the workspace.
            let memory = match arg_value(args, "--gemel") {
                Some(p) => Some(
                    phorport::memory::PortMemory::at(Path::new(&p))
                        .map_err(|e| format!("gemel: {e}"))?,
                ),
                None => phorport::memory::PortMemory::open(&workspace),
            };

            let outcome = explore::campaign(
                &config,
                &target,
                &symbol,
                memory.as_ref(),
                &source_hash,
                &fuzz_root,
                Path::new(&frf_fuzz_root),
                &phorport_root,
                Path::new(&frf_fuzz_bin),
                max_time,
                &out,
            )
            .map_err(|e| e.to_string())?;

            println!("target:      {}", outcome.target);
            println!("source hash: {source_hash}");
            if outcome.skipped_due_to_memory {
                println!("already known: this candidate's failure is in Gemel memory");
                for r in &outcome.prior_rejections {
                    println!("  prior rejection {}: {} ({})", r.gid, r.residual, r.detail);
                }
                return Ok(0);
            }
            println!("findings:    {}", outcome.findings);
            match &outcome.counterexample {
                Some(cx) => {
                    println!("counterexample:      {}", cx.content_id());
                    println!("original residual:   {}", cx.original_residual);
                    println!("minimal residual:    {}", cx.minimal_residual);
                    println!("original (hex):      {}", cx.original_hex);
                    println!("minimal (hex):       {}", cx.minimal_hex);
                    println!(
                        "reductions:          {} accepted / {} refused",
                        cx.accepted_reductions, cx.refused_reductions
                    );
                    if let Some(gid) = &outcome.memory_gid {
                        println!("memory:               {gid}");
                    }
                    println!("evidence:            {out}");
                    Ok(0)
                }
                None => {
                    for line in outcome
                        .campaign_log
                        .lines()
                        .rev()
                        .take(6)
                        .collect::<Vec<_>>()
                        .iter()
                        .rev()
                    {
                        println!("  | {line}");
                    }
                    println!("no counterexample extracted");
                    Ok(if outcome.findings > 0 { 1 } else { 0 })
                }
            }
        }
        "history" => {
            // Handled before compilation; unreachable here.
            Ok(0)
        }
        other => Err(format!("unknown command {other}")),
    }
}

/// SHA-256 of a file's bytes (the candidate source identity).
fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

/// Locate this crate's directory (for the generated project's path dependency).
fn phorport_root(workspace: &Path) -> PathBuf {
    workspace.join("phorport")
}

/// The value following `--frf-fuzz-fixture`.
fn fixture_path(args: &[String]) -> Option<String> {
    arg_value(args, "--frf-fuzz-fixture")
}

/// `phorport __oracle <target_id> --frf-fuzz-fixture <fixture>`.
///
/// The reference side of the FRF differential court: observe the foreign oracle
/// and print the observable to stderr. Exit 0 on success, 3 on a deterministic
/// error (which FRF records as a divergence, never as agreement).
fn oracle_mode(args: &[String]) -> i32 {
    let target_id = match args.get(1) {
        Some(t) => t.clone(),
        None => return 3,
    };
    let fixture = match fixture_path(args) {
        Some(f) => f,
        None => return 3,
    };
    let target = match resolve_target(&target_id) {
        Some(t) => t,
        None => {
            eprintln!("oracle-error unknown-target");
            return 3;
        }
    };
    let spec = match phost::porting::portspec::by_target_id(&target_id) {
        Some(s) => s,
        None => {
            eprintln!("oracle-error no-spec");
            return 3;
        }
    };
    let data = match std::fs::read(&fixture) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("oracle-error fixture-unreadable");
            return 3;
        }
    };
    let args_decoded = match phorport::case::decode_args(&target, &data) {
        Some(a) => a,
        None => {
            eprintln!("oracle-error undecodable");
            return 3;
        }
    };
    if phost::porting::portspec::validate_case(spec, &args_decoded).is_err() {
        eprintln!("oracle-error out-of-contract");
        return 3;
    }
    let case = phost::porting::target::TestCase::new(String::from("frf"), args_decoded);
    let auth = phost::porting::PortingAuthority::granted();
    match phost::porting::dialect_cage::observe_target(&target, core::slice::from_ref(&case), &auth)
    {
        Ok(traces) => match traces.first() {
            Some(t) => {
                eprintln!("oracle {}", t.output_hex);
                0
            }
            None => {
                eprintln!("oracle-error empty");
                3
            }
        },
        Err(_) => {
            eprintln!("oracle-error cage");
            3
        }
    }
}

/// `phorport __differential <target> <object> <hash> --frf-fuzz-fixture <fixture>`.
///
/// The candidate side of the FRF differential court. On parity it prints the
/// **same** line the authority prints (so FRF observes no residual); on a
/// divergence it prints the oracle line plus a divergence line and exits 2.
fn differential_mode(args: &[String]) -> i32 {
    let target_id = match args.get(1) {
        Some(t) => t.clone(),
        None => return 3,
    };
    let object_path = match args.get(2) {
        Some(p) => p.clone(),
        None => return 3,
    };
    let object_hash = match args.get(3) {
        Some(h) => h.clone(),
        None => return 3,
    };
    let fixture = match fixture_path(args) {
        Some(f) => f,
        None => return 3,
    };
    let data = match std::fs::read(&fixture) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("differential-error fixture-unreadable");
            return 3;
        }
    };
    let config = HarnessConfig {
        target_id,
        candidate_path: object_path,
        candidate_hash: object_hash,
    };
    let o = phorport::harness::probe(&config, &data);
    if !o.valid {
        eprintln!("differential-error invalid-case");
        return 3;
    }
    if o.matched {
        // Byte-identical to the authority's observation: parity.
        eprintln!("oracle {}", o.oracle_hex);
        0
    } else {
        eprintln!("oracle {}", o.oracle_hex);
        eprintln!("divergence {} {}", o.residual, o.candidate_hex);
        2
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The hidden containment subcommand: run the untrusted-candidate worker loop.
    if args.first().map(String::as_str) == Some("__worker") {
        return ExitCode::from(phorport::worker::run_worker() as u8);
    }
    // The hidden FRF court modes: the reference oracle and the differential
    // candidate. These print to stderr, which FRF observes.
    if args.first().map(String::as_str) == Some("__oracle") {
        return ExitCode::from(oracle_mode(&args) as u8);
    }
    if args.first().map(String::as_str) == Some("__differential") {
        return ExitCode::from(differential_mode(&args) as u8);
    }
    match command(&args) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("phorport: {e}");
            ExitCode::from(2)
        }
    }
}
