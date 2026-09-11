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
            "usage: phorport <probe|corpus|fuzz> <symbol> --candidate-src <path> …",
        ));
    };
    let symbol = args
        .get(1)
        .cloned()
        .ok_or_else(|| String::from("missing <symbol>"))?;
    let candidate_src = arg_value(args, "--candidate-src")
        .ok_or_else(|| String::from("missing --candidate-src"))?;
    let workspace =
        PathBuf::from(arg_value(args, "--workspace").unwrap_or_else(|| String::from(".")));
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

            let outcome = explore::campaign(
                &config,
                &target,
                &symbol,
                &fuzz_root,
                Path::new(&frf_fuzz_root),
                &phorport_root,
                Path::new(&frf_fuzz_bin),
                max_time,
                &out,
            )
            .map_err(|e| e.to_string())?;

            println!("target:      {}", outcome.target);
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
        other => Err(format!("unknown command {other}")),
    }
}

/// Locate this crate's directory (for the generated project's path dependency).
fn phorport_root(workspace: &Path) -> PathBuf {
    workspace.join("phorport")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match command(&args) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("phorport: {e}");
            ExitCode::from(2)
        }
    }
}
