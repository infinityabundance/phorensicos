// phorport/ablation.rs — controlled ablation of the search mechanisms (§25, Phase 9)
//
// The question is not "does the foundry port one function" but "do the declared
// mechanisms contribute". This module runs the same declared defects against
// different experiment-selection arms with a shared deterministic seed and a
// fixed budget, and measures how many executions each arm needs to find a
// **distinguishing input** for each declared defect family.
//
// The arms:
//
//   A  design corpus only
//   B  + uniform random inputs (seeded, deterministic)
//   C  + role-lattice guided inputs (boundary-heavy)
//   D  C with precedent suppression (an input whose coarse signature was already
//      observed is not re-run — "already-failed work is not new work")
//   E  D with disagreement selection: greedily choose the input that separates
//      the most currently-live defect hypotheses
//
// It measures executions-to-first-distinguishing-input per family, the families
// distinguished within the budget, and the distinct residual classes found. It
// reports medians and interquartile ranges. It never converts a measurement into
// a correctness claim.

use std::collections::BTreeSet;

use phost::porting::challenge::{leaf_mutants, LeafMutant};
use phost::porting::target::{cases_for, resolve_target, PortTarget, TestCase};
use phost::porting::{dialect_cage, portspec, PortingAuthority};

use crate::case::{decode_args, encode_args};

/// The experiment-selection arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    Design,
    Random,
    Guided,
    Precedent,
    Disagreement,
}

impl Arm {
    pub fn id(self) -> &'static str {
        match self {
            Arm::Design => "A-design",
            Arm::Random => "B-random",
            Arm::Guided => "C-guided",
            Arm::Precedent => "D-precedent",
            Arm::Disagreement => "E-disagreement",
        }
    }

    pub fn all() -> [Arm; 5] {
        [
            Arm::Design,
            Arm::Random,
            Arm::Guided,
            Arm::Precedent,
            Arm::Disagreement,
        ]
    }
}

/// The declared seed. A constant (no wall clock).
pub const ABLATION_SEED: u64 = 0x4142_4c41_5449_4f4e;

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One measured trial: how many executions an arm needed for a family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyTrial {
    pub family: String,
    /// Executions to the first distinguishing input, censored at the budget.
    pub executions: Option<u64>,
}

/// The per-arm result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmReport {
    pub arm: &'static str,
    pub budget: u64,
    pub trials: Vec<FamilyTrial>,
    pub families_distinguished: u64,
    pub families_total: u64,
    /// Median executions-to-first over the distinguished families.
    pub median_executions: Option<u64>,
    /// The interquartile range (Q1, Q3).
    pub iqr: Option<(u64, u64)>,
    /// Distinct residual classes observed on the distinguishing inputs.
    pub distinct_residuals: u64,
}

fn quantile(sorted: &[u64], q: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

fn summarize(
    arm: Arm,
    budget: u64,
    trials: Vec<FamilyTrial>,
    total: u64,
    residuals: BTreeSet<String>,
) -> ArmReport {
    let mut distinguished: Vec<u64> = trials.iter().filter_map(|t| t.executions).collect();
    distinguished.sort_unstable();
    let median = quantile(&distinguished, 0.5);
    let iqr = match (
        quantile(&distinguished, 0.25),
        quantile(&distinguished, 0.75),
    ) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    ArmReport {
        arm: arm.id(),
        budget,
        families_distinguished: distinguished.len() as u64,
        families_total: total,
        median_executions: median,
        iqr,
        distinct_residuals: residuals.len() as u64,
        trials,
    }
}

/// Encode a case index/key into deterministic pseudo-random bytes.
fn random_input(seed: u64, i: u64) -> Vec<u8> {
    let mut state = seed ^ (i.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let len = 1 + (splitmix64(&mut state) % 12) as usize;
    (0..len)
        .map(|_| (splitmix64(&mut state) & 0xff) as u8)
        .collect()
}

/// The candidate input pools, as decoded arguments.
fn pools(
    target: &PortTarget,
    seed: u64,
) -> (Vec<Vec<Vec<u8>>>, Vec<Vec<Vec<u8>>>, Vec<Vec<Vec<u8>>>) {
    let spec = portspec::by_target_id(target.id).expect("spec");
    let valid = |args: &Vec<Vec<u8>>| portspec::validate_case(spec, args).is_ok();

    // Design: the registered corpus arguments.
    let design: Vec<Vec<Vec<u8>>> = cases_for(target)
        .iter()
        .map(|c| c.args.clone())
        .filter(valid)
        .collect();

    // Random: deterministic pseudo-random fuzz inputs decoded into cases.
    let mut random: Vec<Vec<Vec<u8>>> = Vec::new();
    for i in 0..512u64 {
        if let Some(args) = decode_args(target, &random_input(seed, i)) {
            if valid(&args) && !random.contains(&args) {
                random.push(args);
            }
        }
    }

    // Guided: boundary-heavy role-lattice cases (reusing the qualification
    // generator's construction).
    let guided: Vec<Vec<Vec<u8>>> = crate::qualification::build(target, spec)
        .map(|u| u.cases.iter().map(|c| c.args.clone()).collect())
        .unwrap_or_default();

    (design, random, guided)
}

/// The coarse signature of an input (for precedent suppression).
fn signature(args: &[Vec<u8>]) -> String {
    let mut parts: Vec<String> = args
        .iter()
        .map(|a| format!("{}:{}", a.len(), a.first().copied().unwrap_or(0)))
        .collect();
    parts.sort();
    parts.join("|")
}

/// The number of distinct observable partitions the live hypotheses fall into
/// on this input — the deterministic disagreement score of §11. Higher means the
/// input separates more surviving hypotheses. It is cheap (no oracle call).
fn score_partitions(mutants: &[LeafMutant], live: &BTreeSet<String>, args: &[Vec<u8>]) -> usize {
    let mut distinct: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut defined = 0usize;
    for m in mutants.iter().filter(|m| live.contains(m.id)) {
        if let Some(o) = (m.behavior)(args) {
            distinct.insert(o);
            defined += 1;
        }
    }
    // Reward separation (distinct partitions) and penalize undetermined members.
    distinct.len() * 2 + defined
}

/// Does `mutant` disagree with the oracle on `args`?
fn distinguishes(
    target: &PortTarget,
    mutant: &LeafMutant,
    args: &[Vec<u8>],
    auth: &PortingAuthority,
) -> bool {
    let Some(wrong) = (mutant.behavior)(args) else {
        return false;
    };
    let case = TestCase::new(format!("abl:{}", mutant.id), args.to_vec());
    match dialect_cage::observe_target(target, core::slice::from_ref(&case), auth) {
        Ok(traces) => traces
            .first()
            .map(|t| t.output_hex != hex::encode(&wrong))
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// The residual class of a distinguishing input (how it diverges).
fn residual_of(mutant: &LeafMutant, args: &[Vec<u8>]) -> String {
    // A deterministic, structural label from the mutant family and the input
    // shape (byte-equality, terminator position, length). Not a correctness
    // claim; a coarse class for counting.
    let shapes: Vec<String> = args
        .iter()
        .map(|a| {
            if a.contains(&0) {
                String::from("terminated")
            } else if a.is_empty() {
                String::from("empty")
            } else {
                String::from("raw")
            }
        })
        .collect();
    format!("{}:{}", mutant.family, shapes.join("+"))
}

/// Run one arm and measure.executions-to-first per family.
fn run_arm(
    target: &PortTarget,
    mutants: &[LeafMutant],
    budget: u64,
    seed: u64,
    arm: Arm,
) -> ArmReport {
    let auth = PortingAuthority::granted();
    let (design, random, guided) = pools(target, seed);

    // The input order for this arm.
    let inputs: Vec<Vec<Vec<u8>>> = match arm {
        Arm::Design => design.clone(),
        Arm::Random => {
            let mut v = design.clone();
            v.extend(random.clone());
            v
        }
        Arm::Guided => {
            let mut v = design.clone();
            v.extend(guided.clone());
            v
        }
        Arm::Precedent => {
            // Guided order, but suppress inputs whose coarse signature repeats.
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut v = Vec::new();
            for args in design.iter().chain(guided.iter()) {
                if seen.insert(signature(args)) {
                    v.push(args.clone());
                }
            }
            v
        }
        Arm::Disagreement => {
            // Greedy: from a pool, repeatedly take the input that separates the
            // most currently-live hypotheses.
            let mut pool: Vec<Vec<Vec<u8>>> = Vec::new();
            pool.extend(random.iter().cloned());
            pool.extend(guided.iter().cloned());
            let mut live: BTreeSet<String> = mutants.iter().map(|m| m.id.to_string()).collect();
            let mut chosen: Vec<Vec<Vec<u8>>> = Vec::new();
            let mut used: BTreeSet<usize> = BTreeSet::new();
            while chosen.len() < budget as usize && !live.is_empty() && used.len() < pool.len() {
                let mut best: Option<(usize, usize)> = None; // (index, score)
                for (i, args) in pool.iter().enumerate() {
                    if used.contains(&i) {
                        continue;
                    }
                    let score = score_partitions(mutants, &live, args);
                    if best.map(|(_, s)| score > s).unwrap_or(true) {
                        best = Some((i, score));
                    }
                }
                let Some((i, _)) = best else { break };
                used.insert(i);
                let args = pool[i].clone();
                // Retire the hypotheses this input actually separates.
                let retired: Vec<String> = mutants
                    .iter()
                    .filter(|m| live.contains(m.id))
                    .filter(|m| distinguishes(target, m, &args, &auth))
                    .map(|m| m.id.to_string())
                    .collect();
                for id in retired {
                    live.remove(&id);
                }
                chosen.push(args);
            }
            // Append the design corpus first (it is public knowledge), then the
            // selected inputs.
            let mut v = design.clone();
            v.extend(chosen);
            v
        }
    };

    // Measure executions-to-first for each family over the arm's input order.
    let mut trials = Vec::new();
    let mut residuals: BTreeSet<String> = BTreeSet::new();
    for m in mutants {
        let mut found = None;
        for (n, args) in inputs.iter().enumerate() {
            if (n as u64) >= budget {
                break;
            }
            if distinguishes(target, m, args, &auth) {
                found = Some(n as u64 + 1);
                residuals.insert(residual_of(m, args));
                break;
            }
        }
        trials.push(FamilyTrial {
            family: m.id.to_string(),
            executions: found,
        });
    }
    summarize(arm, budget, trials, mutants.len() as u64, residuals)
}

/// Run every arm for `symbol` with a shared seed and budget.
pub fn run(symbol: &str, budget: u64) -> Result<Vec<ArmReport>, String> {
    let target = resolve_target(symbol).ok_or_else(|| format!("unknown symbol {symbol}"))?;
    let mutants = leaf_mutants(target.id);
    let mut out = Vec::new();
    for arm in Arm::all() {
        out.push(run_arm(&target, &mutants, budget, ABLATION_SEED, arm));
    }
    Ok(out)
}

impl ArmReport {
    pub fn to_json(&self) -> String {
        let trials: Vec<String> = self
            .trials
            .iter()
            .map(|t| {
                format!(
                    "      {{\"family\": \"{}\", \"executions\": {}}}",
                    t.family,
                    t.executions
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| String::from("null"))
                )
            })
            .collect();
        let iqr = self
            .iqr
            .map(|(a, b)| format!("[{a}, {b}]"))
            .unwrap_or_else(|| String::from("null"));
        format!(
            "  {{\n    \"arm\": \"{}\",\n    \"budget\": {},\n    \"families_distinguished\": {},\n    \"families_total\": {},\n    \"median_executions\": {},\n    \"iqr\": {},\n    \"distinct_residuals\": {},\n    \"trials\": [\n{}\n    ]\n  }}",
            self.arm,
            self.budget,
            self.families_distinguished,
            self.families_total,
            self.median_executions
                .map(|m| m.to_string())
                .unwrap_or_else(|| String::from("null")),
            iqr,
            self.distinct_residuals,
            trials.join(",\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ablation_is_deterministic_and_measures_every_arm() {
        let a = run("strspn", 64).expect("ablation");
        let b = run("strspn", 64).expect("ablation");
        assert_eq!(a, b, "the ablation must be deterministic");
        assert_eq!(a.len(), 5);
        for r in &a {
            assert_eq!(r.families_total, r.trials.len() as u64);
            assert!(r.families_distinguished <= r.families_total);
        }
    }

    #[test]
    fn test_a_larger_budget_distinguishes_at_least_as_many_families() {
        let small = run("strspn", 16).expect("ablation");
        let large = run("strspn", 256).expect("ablation");
        for (s, l) in small.iter().zip(large.iter()) {
            assert!(
                l.families_distinguished >= s.families_distinguished,
                "arm {} regressed with more budget",
                s.arm
            );
        }
    }
}
