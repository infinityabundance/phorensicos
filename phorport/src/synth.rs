// phorport/synth.rs — a bounded enumerative Phor synthesizer (§12, Phase 9)
//
// The `CandidateProducer` in `docs/CEGIS.md` was a scripted catalogue. This is a
// real, if deliberately small, **synthesizer**: given only the public `PortSpec`
// (its ABI symbol and the target's declared observable), it enumerates candidate
// `phor` programs from a declared program family and proposes them one at a time.
// It never reads the withheld candidate source, the held-out corpus or the
// challenge expectations — the finding is produced by the CEGIS loop falsifying
// the wrong members of the family against the design corpus.
//
// The family is the **branchless lane scan** the lowerer requires (a single basic
// block, so no branchy loop): for each byte lane, a predicate computes a hit and
// a subtract-update moves the accumulated index toward the matching lane. The
// enumeration axes are genuinely algorithmic:
//
//   * lane order   high→low (first match wins) vs low→high (last match wins)
//   * masking      whether the lane byte is masked to 8 bits before comparison
//
// The bound guard (`n > i`) is contract-derived: the observable is defined only
// up to `n`, so every candidate respects it.
//
// The family is an extension point keyed by symbol, not a branch in generic
// machinery (`docs/AUTONOMOUS_PORTING_ARCHITECTURE.md` §30).

use phost::porting::portspec::PortSpec;

use crate::producer::{
    CandidateProducer, CandidateProducerError, CandidateProposal, CandidateRequest,
};

/// The declared program family version. Changing the family is a versioned act.
pub const SYNTH_VERSION: &str = "phorport.lane-scan.v1";

/// A shape's ABI parameters (public: the symbol and arity come from the spec).
struct Shape {
    symbol: &'static str,
    params: &'static str,
    /// The lane word parameter.
    word: &'static str,
    /// The bound parameter.
    bound: &'static str,
    /// The observable mode.
    mode: Mode,
}

enum Mode {
    /// A length: the first NUL.
    FirstNul,
    /// An index: the first byte equal to the needle, or -1.
    FirstMatch,
}

fn shape_for(spec: &PortSpec) -> Option<Shape> {
    match spec.symbol {
        "strlen" => Some(Shape {
            symbol: "phor_strlen_len",
            params: "w: u64, n: u64",
            word: "w",
            bound: "n",
            mode: Mode::FirstNul,
        }),
        "memchr" => Some(Shape {
            symbol: "phor_memchr_index",
            params: "wh: u64, needle: u64, n: u64",
            word: "wh",
            bound: "n",
            mode: Mode::FirstMatch,
        }),
        _ => None,
    }
}

/// The enumeration axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Variant {
    high_to_low: bool,
    masked: bool,
}

/// Emit one candidate source for a variant.
fn emit(shape: &Shape, v: Variant) -> String {
    let (init, predicate) = match shape.mode {
        Mode::FirstNul => ("let mut idx: u64 = n;".to_string(), "== 0".to_string()),
        Mode::FirstMatch => (
            "let mut idx: u64 = 0 - 1;".to_string(),
            "== (needle & 255)".to_string(),
        ),
    };
    let mut body = String::new();
    body.push_str("    ");
    body.push_str(&init);
    body.push('\n');
    let lanes: Vec<usize> = if v.high_to_low {
        (0..8).rev().collect()
    } else {
        (0..8).collect()
    };
    for i in lanes {
        let shift = 8 * i;
        let extract = if v.masked {
            format!("(({word} >> {shift}) & 255)", word = shape.word)
        } else {
            format!("({word} >> {shift})", word = shape.word)
        };
        let guard = format!(" * (({bound} > {i}) as u64)", bound = shape.bound);
        body.push_str(&format!(
            "    let b{i}: u64 = {extract};\n    let hit{i}: u64 = ((b{i} {predicate}) as u64){guard};\n    idx = idx - hit{i} * (idx - {i});\n"
        ));
    }
    body.push_str("    return idx;\n");

    format!(
        "package \"phorensic:synth_lane_scan:v1.0\";\n\nfn {symbol}({params}) -> u64 effect [compute] {{\n{body}}}\n",
        symbol = shape.symbol,
        params = shape.params,
    )
}

/// Enumerate the family in a deterministic order. The correct variant is not
/// privileged: a deliberately wrong order (low→high, last match wins) is offered
/// first, so the CEGIS loop must falsify it and revise.
fn family(shape: &Shape) -> Vec<String> {
    let mut out = Vec::new();
    for high_to_low in [false, true] {
        for masked in [true, false] {
            out.push(emit(
                shape,
                Variant {
                    high_to_low,
                    masked,
                },
            ));
        }
    }
    out
}

/// A bounded enumerative synthesizer for the lane-scan family.
pub struct EnumerativeProducer {
    candidates: Vec<String>,
    next: usize,
}

impl EnumerativeProducer {
    /// Build a synthesizer for `spec`, or `None` when no family is declared.
    pub fn for_spec(spec: &PortSpec) -> Option<Self> {
        let shape = shape_for(spec)?;
        Some(EnumerativeProducer {
            candidates: family(&shape),
            next: 0,
        })
    }

    /// How many candidates the family declares.
    pub fn size(&self) -> usize {
        self.candidates.len()
    }
}

impl CandidateProducer for EnumerativeProducer {
    fn name(&self) -> &'static str {
        "enumerative-lane-scan"
    }

    fn propose(
        &mut self,
        _request: &CandidateRequest,
    ) -> Result<CandidateProposal, CandidateProducerError> {
        match self.candidates.get(self.next) {
            Some(s) => {
                self.next += 1;
                Ok(CandidateProposal {
                    source: s.clone(),
                    note: format!("lane-scan variant {}", self.next - 1),
                })
            }
            None => Err(CandidateProducerError::Refused(format!(
                "{} family exhausted",
                SYNTH_VERSION
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::portspec;

    fn spec(symbol: &str) -> &'static PortSpec {
        portspec::by_target_id(&format!(
            "{}:{}:{}:{}:v1",
            if symbol == "strlen" { "libc" } else { "libc" },
            symbol,
            "c-locale",
            if symbol == "strlen" { "u64" } else { "index" }
        ))
        .expect("spec")
    }

    #[test]
    fn test_family_contains_the_correct_first_nul_scan() {
        let s = spec("strlen");
        let p = EnumerativeProducer::for_spec(s).expect("family");
        assert_eq!(p.size(), 4);
        // The correct variant is present, but not first (the first is a
        // deliberately wrong last-match scan).
        let correct = p
            .candidates
            .iter()
            .find(|c| c.contains("(w >> 56) & 255"))
            .expect("correct variant");
        assert!(correct.contains("let mut idx: u64 = n;"));
        assert!(correct.contains("(n > 7)"));
        assert!(correct.contains("idx = idx - hit7 * (idx - 7);"));
        // The first offered variant is the wrong low→high (last-match) scan.
        assert!(p.candidates[0].contains("(w >> 0) & 255"));
    }

    #[test]
    fn test_family_contains_the_correct_first_match_scan() {
        let s = spec("memchr");
        let p = EnumerativeProducer::for_spec(s).expect("family");
        let correct = p
            .candidates
            .iter()
            .find(|c| c.contains("(wh >> 56) & 255"))
            .expect("correct variant");
        assert!(correct.contains("let mut idx: u64 = 0 - 1;"));
        assert!(correct.contains("== (needle & 255)"));
    }

    #[test]
    fn test_unknown_shape_has_no_family() {
        let s = portspec::by_target_id("libc:toupper:c-locale:u8:v1").expect("spec");
        assert!(EnumerativeProducer::for_spec(s).is_none());
    }

    /// Blind regeneration: the synthesizer sees only the `PortSpec`, falsifies the
    /// wrong members of its family against the design corpus, and the survivor is
    /// qualified, challenged, verified, executed, dispatched and sealed — without
    /// ever reading `examples/jit_port_strlen.phor`.
    #[test]
    fn test_blind_regeneration_of_strlen_seals() {
        use crate::cegis::CampaignManifest;
        use crate::config::HarnessConfig;
        use crate::pipeline::{run_autonomous, AutonomyInputs};
        use phost::porting::target::resolve_target;

        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));

        let target = resolve_target("strlen").expect("target");
        let s = spec("strlen");
        let mut producer = EnumerativeProducer::for_spec(s).expect("family");
        let work = std::env::temp_dir().join(format!("phorport-synth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let inputs = AutonomyInputs {
            spec: s,
            target: target.clone(),
            base_config: HarnessConfig {
                target_id: target.id.to_string(),
                candidate_path: String::new(),
                candidate_hash: String::new(),
            },
            work_root: work.clone(),
            frf_store_root: work.join("frf"),
            program: debug_dir.join("phorport"),
            manifest: CampaignManifest {
                target_id: target.id.to_string(),
                port_spec_id: s.id(),
                design_generator: String::from("registry"),
                qualification_policy: String::from("host-observed"),
                discovery_budget: 4,
                challenge_required: true,
                seal_profile: String::from("autonomous-v1"),
            },
            memory: None,
        };
        let report = run_autonomous(&inputs, &mut producer, None).expect("pipeline runs");
        assert!(
            report.sealed(),
            "blind regeneration did not seal; log: {:#?}; refusal: {:?}",
            report.log,
            report.seal_refusal
        );
        // The first family member (a wrong last-match scan) was falsified.
        assert!(
            !report.cegis.rejected.is_empty(),
            "the search falsified nothing"
        );
        let _ = std::fs::remove_dir_all(&work);
    }
}
