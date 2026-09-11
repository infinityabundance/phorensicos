// phorport/demand.rs — deterministic demand prioritization (§20)
//
// A runtime miss emits a `PortDemand` (data). When several demands exist, the
// foundry prioritizes them with a **recorded, deterministic integer score**:
// fixed weights, an explicit contribution per factor, no probability of success.
// The breakdown is the record of *why* a surface was chosen.

use phost::porting::demand::PortDemand;
use phost::porting::portspec;
use phost::porting::target::resolve_target;

/// The ranking scheme version. Changing a weight is a versioned act.
pub const RANKING_VERSION: &str = "phorport.demand-ranking.v1";

/// One factor's contribution to a score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub factor: &'static str,
    pub weight: i64,
    pub value: i64,
    pub product: i64,
}

/// The deterministic score of one demanded surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemandScore {
    pub surface: String,
    pub contributions: Vec<Contribution>,
    pub total: i64,
}

/// Context a ranking may consult. Every input is measurable; nothing is a
/// probabilistic guess.
pub struct RankContext<'a> {
    /// How many sealed ports depend on (consume) a given surface, if known.
    pub sealed_fanin: &'a dyn Fn(&str) -> u64,
    /// The sealed ports available as dependencies.
    pub sealed_dependencies: &'a [String],
    /// How many prior failed attempts exist for a surface (Gemel), if known.
    pub historical_failures: &'a dyn Fn(&str) -> u64,
}

impl Default for RankContext<'_> {
    fn default() -> Self {
        fn zero(_: &str) -> u64 {
            0
        }
        static EMPTY: [String; 0] = [];
        // A `Default` cannot borrow a temporary closure, so callers should build
        // a context explicitly; this exists for convenience only.
        RankContext {
            sealed_fanin: &zero,
            sealed_dependencies: &EMPTY,
            historical_failures: &zero,
        }
    }
}

fn symbol_of(surface: &str) -> Option<&str> {
    surface.split(':').nth(1)
}

/// Score one demand.
pub fn score(demand: &PortDemand, ctx: &RankContext<'_>) -> DemandScore {
    let fanin = (ctx.sealed_fanin)(&demand.requested_surface) as i64;
    let oracle = if portspec::by_target_id(&demand.requested_surface).is_some() {
        1
    } else {
        0
    };
    let expressible = match symbol_of(&demand.requested_surface) {
        Some(sym) if resolve_target(sym).is_some() => 1,
        _ => 0,
    };
    let sealed_deps = demand
        .available_dependencies
        .iter()
        .filter(|d| ctx.sealed_dependencies.contains(d))
        .count() as i64;
    let callers = if sealed_deps > 0 { 1 } else { 0 };
    let qualification_cost = if oracle == 1 { 1 } else { 0 };
    let failures = (ctx.historical_failures)(&demand.requested_surface) as i64;

    let factors: [(&'static str, i64, i64); 7] = [
        ("runtime_demand_count", 100, demand.demand_count as i64),
        ("existing_sealed_fanin", 40, fanin),
        ("callers_helped", 30, callers),
        ("sealed_dependency_count", 5, sealed_deps),
        ("oracle_available", 20, oracle),
        ("compiler_expressible", 15, expressible),
        ("historical_failed_attempt_burden", -25, failures),
    ];
    let mut contributions = Vec::with_capacity(factors.len() + 1);
    for (factor, weight, value) in factors {
        contributions.push(Contribution {
            factor,
            weight,
            value,
            product: weight * value,
        });
    }
    // Qualification cost is a *cost*: more cases is lower priority.
    contributions.push(Contribution {
        factor: "qualification_cost",
        weight: -10,
        value: qualification_cost,
        product: -10 * qualification_cost,
    });

    let total = contributions.iter().map(|c| c.product).sum();
    DemandScore {
        surface: demand.requested_surface.clone(),
        contributions,
        total,
    }
}

/// Rank demands deterministically: highest score first, ties broken by surface.
pub fn rank(demands: &[PortDemand], ctx: &RankContext<'_>) -> Vec<DemandScore> {
    let mut scores: Vec<DemandScore> = demands.iter().map(|d| score(d, ctx)).collect();
    scores.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.surface.cmp(&b.surface))
    });
    scores
}

impl DemandScore {
    pub fn contribution(&self, factor: &str) -> i64 {
        self.contributions
            .iter()
            .find(|c| c.factor == factor)
            .map(|c| c.product)
            .unwrap_or(0)
    }

    /// A stable JSON projection with the full breakdown.
    pub fn to_json(&self) -> String {
        let c: Vec<String> = self
            .contributions
            .iter()
            .map(|c| {
                format!(
                    "      {{\"factor\": \"{}\", \"weight\": {}, \"value\": {}, \"product\": {}}}",
                    c.factor, c.weight, c.value, c.product
                )
            })
            .collect();
        format!(
            "  {{\n    \"surface\": \"{}\",\n    \"total\": {},\n    \"contributions\": [\n{}\n    ]\n  }}",
            self.surface,
            self.total,
            c.join(",\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demand(surface: &str, count: u64, deps: Vec<String>) -> PortDemand {
        PortDemand {
            requested_surface: surface.to_string(),
            callsite_family: String::from("dispatch"),
            demand_count: count,
            store_generation: String::from("gen-1"),
            available_dependencies: deps,
        }
    }

    #[test]
    fn test_ranking_is_deterministic_and_orders_by_total() {
        fn zero(_: &str) -> u64 {
            0
        }
        let sealed = vec![String::from("libc:toupper:c-locale:u8:v1")];
        let ctx = RankContext {
            sealed_fanin: &zero,
            sealed_dependencies: &sealed,
            historical_failures: &zero,
        };
        let demands = vec![
            demand("libc:strspn:c-locale:u64:v1", 1, vec![]),
            demand("libc:toupper:c-locale:u8:v1", 5, vec![]),
        ];
        let a = rank(&demands, &ctx);
        let b = rank(&demands, &ctx);
        assert_eq!(a, b);
        assert_eq!(a[0].surface, "libc:toupper:c-locale:u8:v1");
    }

    #[test]
    fn test_failed_attempt_burden_lowers_priority() {
        fn zero(_: &str) -> u64 {
            0
        }
        fn five(_: &str) -> u64 {
            5
        }
        let empty: Vec<String> = vec![];
        let ctx_clean = RankContext {
            sealed_fanin: &zero,
            sealed_dependencies: &empty,
            historical_failures: &zero,
        };
        let ctx_burdened = RankContext {
            sealed_fanin: &zero,
            sealed_dependencies: &empty,
            historical_failures: &five,
        };
        let d = demand("libc:strspn:c-locale:u64:v1", 1, vec![]);
        let clean = score(&d, &ctx_clean);
        let burdened = score(&d, &ctx_burdened);
        assert!(burdened.total < clean.total);
        assert_eq!(
            burdened.contribution("historical_failed_attempt_burden"),
            -125
        );
    }

    #[test]
    fn test_recorded_breakdown_is_complete() {
        let ctx = RankContext::default();
        let d = demand("libc:toupper:c-locale:u8:v1", 3, vec![]);
        let s = score(&d, &ctx);
        for factor in [
            "runtime_demand_count",
            "existing_sealed_fanin",
            "callers_helped",
            "sealed_dependency_count",
            "oracle_available",
            "compiler_expressible",
            "historical_failed_attempt_burden",
            "qualification_cost",
        ] {
            assert!(
                s.contributions.iter().any(|c| c.factor == factor),
                "missing factor {factor}"
            );
        }
    }
}
