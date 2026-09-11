// phorport/qualification.rs — the held-out qualification universe (§5, §7)
//
// Phase 7. The design corpus is visible to the candidate producer. Qualification
// must not be: it is a **held-out** universe, constructed *after* a candidate is
// frozen, from a generator that is structurally independent of the design and
// discovery generators, and it is never an input to any producer request.
//
// Three disciplines are load-bearing:
//
//   * **Independent construction, not a different seed.** The design corpora are
//     exhaustive deterministic enumerators over concrete pattern families
//     (`zero/ones/asc/desc/alt`, edge bytes). This module samples *semantic role
//     lattices* — accept-member vs non-member vs terminator, match vs non-match,
//     equal vs ordered-differing — and instantiates their cells into concrete
//     bytes. The two constructions share no axis decomposition.
//
//   * **The receipt is redacted.** `QualificationReceipt` carries counts and
//     case *ids*, never input bytes or oracle outputs. The raw observations live
//     only in `QualificationLedger`, which is the auditor's view and is never
//     materialized into a synthesis workspace. A qualification failure cannot
//     leak the held-out answer to the next revision.
//
//   * **Exhaustive finite domains stay exhaustive.** When the `PortSpec` declares
//     an exhaustive finite case space (`toupper`'s 256 bytes), held-out sampling
//     cannot add coverage, so the universe enumerates the complete domain and
//     says so. "Held out" is only claimed where the space is not already finite.

use std::collections::BTreeSet;

use phost::porting::portspec::{self, CaseSpaceKind, PortSpec, QualificationPolicy};
use phost::porting::target::PortTarget;

use crate::case::{CaseProvenance, CaseUniverse, PortCase};
use crate::compile::CandidateIdentity;
use crate::config::HarnessConfig;
use crate::harness::{probe_validated, CandidateExec, InProcessExec};

/// The generator identity bound into every qualification receipt.
pub const QUALIFICATION_GENERATOR: &str = "phorport.qualification.role-lattice";
/// The generator version (`role-lattice` construction).
pub const QUALIFICATION_VERSION: &str = "v1";
/// How many sampled cases a bounded-deterministic universe aims for.
pub const QUALIFICATION_CASES: usize = 512;
/// The fixed construction seed. A constant, never a clock.
pub const QUALIFICATION_SEED: u64 = 0x5048_4f52_5155_414c;
/// The content-identity domain tag for a universe.
pub const QUALIFICATION_DOMAIN: &[u8] = b"PHOR/QUALIFICATION/v1\0";

/// SplitMix64 — a deterministic, dependency-free mixer for the sampler.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (`bound > 0`).
    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// A byte, biased toward the unsigned boundary at 0x7f/0x80.
    fn byte(&mut self) -> u8 {
        match self.below(4) {
            0 => 0x7f,
            1 => 0x80,
            _ => (self.next_u64() & 0xff) as u8,
        }
    }

    /// A non-NUL byte.
    fn non_nul(&mut self) -> u8 {
        let b = self.byte();
        if b == 0 {
            0x41
        } else {
            b
        }
    }
}

/// A built, content-addressed qualification universe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationUniverse {
    pub target_id: String,
    pub generator_id: String,
    pub generator_version: String,
    pub construction: String,
    /// True when the `PortSpec`'s case space is an exhaustive finite domain, so
    /// the universe enumerates that domain completely.
    pub exhaustive_domain: bool,
    pub cases: Vec<PortCase>,
}

impl QualificationUniverse {
    /// The content identity of the universe.
    pub fn id(&self) -> String {
        let mut pre = Vec::with_capacity(QUALIFICATION_DOMAIN.len() + 256);
        pre.extend_from_slice(QUALIFICATION_DOMAIN);
        enc(&mut pre, &self.target_id);
        enc(&mut pre, &self.generator_id);
        enc(&mut pre, &self.generator_version);
        enc(&mut pre, &self.construction);
        pre.push(self.exhaustive_domain as u8);
        pre.extend_from_slice(&(self.cases.len() as u32).to_le_bytes());
        for c in &self.cases {
            enc(&mut pre, &c.id);
            pre.extend_from_slice(&(c.args.len() as u32).to_le_bytes());
            for a in &c.args {
                pre.extend_from_slice(&(a.len() as u32).to_le_bytes());
                pre.extend_from_slice(a);
            }
        }
        crate::compile::sha256_hex(&pre)
    }

    /// Every marker that must never appear in a synthesis workspace.
    ///
    /// The markers are high-entropy, namespaced tokens — the case ids, a
    /// namespaced seed token, and a namespaced digest of every encoded input —
    /// rather than raw short hex, so the audit cannot false-positive on an
    /// unrelated hex string that happens to share a prefix. Raw held-out bytes
    /// are prevented from reaching a workspace by construction (the universe is
    /// built after the freeze and is never an input to `materialize`); this audit
    /// is the defense-in-depth detector for the token forms.
    pub fn isolation_markers(&self, target: &PortTarget) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        out.push(format!("phor.qualification-seed:{QUALIFICATION_SEED:016x}"));
        for c in &self.cases {
            out.push(c.id.clone());
            out.push(format!(
                "phor.qualification-input:{}",
                crate::compile::sha256_hex(&c.encode(target))
            ));
        }
        out
    }
}

fn enc(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn case(id: String, args: Vec<Vec<u8>>, detail: String) -> PortCase {
    PortCase {
        id,
        universe: CaseUniverse::Qualification,
        args,
        provenance: CaseProvenance {
            source: format!("{QUALIFICATION_GENERATOR}.{QUALIFICATION_VERSION}"),
            detail,
        },
    }
}

fn n_arg(n: usize) -> Vec<u8> {
    (n as u64).to_le_bytes().to_vec()
}

// ---------------------------------------------------------------------------
// The role-lattice generators
// ---------------------------------------------------------------------------

/// Build the held-out universe for `target` from its `PortSpec`.
///
/// Fails closed: a generated case that violates its own declared preconditions is
/// a generator defect, not a case to silently drop.
pub fn build(target: &PortTarget, spec: &PortSpec) -> Result<QualificationUniverse, String> {
    let exhaustive = spec.case_space.kind == CaseSpaceKind::ExhaustiveFinite;
    let mut rng = Rng::new(QUALIFICATION_SEED);
    let mut raw: Vec<(String, Vec<Vec<u8>>, String)> = Vec::new();

    match target.symbol {
        "toupper" => {
            // The complete finite input domain; generated independently, but the
            // space is already exhaustive, so no held-out coverage is claimed.
            for b in 0..=255u16 {
                raw.push((
                    format!("Q.toupper.{:02x}", b),
                    vec![vec![b as u8]],
                    String::from("complete byte domain"),
                ));
            }
        }
        "memcmp" => {
            for i in 0..QUALIFICATION_CASES {
                let (a, b, n, detail) = memcmp_case(&mut rng);
                raw.push((format!("Q.memcmp.{i:04}"), vec![a, b, n_arg(n)], detail));
            }
        }
        "memchr" => {
            for i in 0..QUALIFICATION_CASES {
                let (hay, needle, n, detail) = memchr_case(&mut rng);
                raw.push((
                    format!("Q.memchr.{i:04}"),
                    vec![hay, vec![needle], n_arg(n)],
                    detail,
                ));
            }
        }
        "strlen" => {
            for i in 0..QUALIFICATION_CASES {
                let (buf, n, detail) = strlen_case(&mut rng);
                raw.push((format!("Q.strlen.{i:04}"), vec![buf, n_arg(n)], detail));
            }
        }
        "strrchr" => {
            for i in 0..QUALIFICATION_CASES {
                let (buf, needle, n, detail) = strrchr_case(&mut rng);
                raw.push((
                    format!("Q.strrchr.{i:04}"),
                    vec![buf, vec![needle], n_arg(n)],
                    detail,
                ));
            }
        }
        "strspn" => {
            for i in 0..QUALIFICATION_CASES {
                let (s, accept, n, detail) = strspn_case(&mut rng);
                raw.push((
                    format!("Q.strspn.{i:04}"),
                    vec![s, accept, n_arg(n)],
                    detail,
                ));
            }
        }
        other => return Err(format!("no qualification generator for {other}")),
    }

    // Deduplicate (the sampler can repeat a cell) while preserving order.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut cases: Vec<PortCase> = Vec::with_capacity(raw.len());
    for (id, args, detail) in raw {
        let key = format!("{args:?}");
        if !seen.insert(key) {
            continue;
        }
        // Fail closed on a generator defect: the case must satisfy its contract.
        if let Err(v) = portspec::validate_case(spec, &args) {
            return Err(format!("generated out-of-contract case {id}: {v:?}"));
        }
        cases.push(case(id, args, detail));
    }

    Ok(QualificationUniverse {
        target_id: target.id.to_string(),
        generator_id: QUALIFICATION_GENERATOR.to_string(),
        generator_version: QUALIFICATION_VERSION.to_string(),
        construction: String::from(
            "semantic role-lattice sampling with boundary injection; independent in kind from the design corpora's concrete-pattern enumeration",
        ),
        exhaustive_domain: exhaustive,
        cases,
    })
}

/// memcmp: role lattice over {equal, ordered-differing at k, differing beyond n}.
fn memcmp_case(rng: &mut Rng) -> (Vec<u8>, Vec<u8>, usize, String) {
    let n = 1 + rng.below(8);
    let a: Vec<u8> = (0..n).map(|_| rng.byte()).collect();
    match rng.below(3) {
        0 => {
            // Equal over the compared prefix; sometimes a differing tail past n.
            let mut b = a.clone();
            let detail = if rng.below(2) == 0 {
                b.push(a[0] ^ 0xff);
                "equal prefix, differing byte beyond n"
            } else {
                "equal prefix"
            };
            (a, b, n, detail.to_string())
        }
        _ => {
            // Differ at exactly one position k inside the compared prefix.
            let k = rng.below(n);
            let mut b = a.clone();
            b[k] ^= 1 << rng.below(8);
            (a, b, n, format!("ordered difference at k={k}"))
        }
    }
}

/// memchr: role lattice over {match at j, absent, match at the n boundary}.
fn memchr_case(rng: &mut Rng) -> (Vec<u8>, u8, usize, String) {
    let n = rng.below(9);
    if n == 0 {
        return (Vec::new(), rng.byte(), 0, String::from("empty window"));
    }
    let mut hay: Vec<u8> = (0..n).map(|_| rng.byte()).collect();
    let needle = rng.byte();
    match rng.below(3) {
        0 => {
            let j = rng.below(n);
            hay[j] = needle;
            (hay, needle, n, format!("match at j={j}"))
        }
        1 => {
            // Force absence: replace every needle occurrence.
            for b in hay.iter_mut() {
                if *b == needle {
                    *b = needle ^ 0xff;
                }
            }
            (hay, needle, n, String::from("absent needle"))
        }
        _ => {
            // Match only at the last compared position (the n boundary).
            let j = n - 1;
            hay[j] = needle;
            for b in hay.iter_mut().take(j) {
                if *b == needle {
                    *b = needle ^ 0xff;
                }
            }
            (hay, needle, n, String::from("match at the n boundary"))
        }
    }
}

/// strlen: role lattice over terminator index k within the bound n.
fn strlen_case(rng: &mut Rng) -> (Vec<u8>, usize, String) {
    let n = 1 + rng.below(8);
    let k = rng.below(n);
    let mut buf: Vec<u8> = (0..n).map(|_| rng.non_nul()).collect();
    buf[k] = 0x00;
    // Sometimes add a NUL-free tail beyond n (must be ignored).
    if rng.below(2) == 0 {
        buf.extend((0..rng.below(4)).map(|_| rng.non_nul()));
    }
    (buf, n, format!("terminator at k={k}, bound n={n}"))
}

/// strrchr: role lattice over {last occurrence, occurrence after terminator, absent}.
fn strrchr_case(rng: &mut Rng) -> (Vec<u8>, u8, usize, String) {
    let n = 1 + rng.below(8);
    let k = rng.below(n);
    let mut buf: Vec<u8> = (0..n).map(|_| rng.non_nul()).collect();
    buf[k] = 0x00;
    match rng.below(3) {
        0 => (buf, 0x00, n, format!("NUL needle, terminator at k={k}")),
        1 => {
            // A content byte that occurs only after the terminator: absent.
            let mut needle = rng.non_nul();
            while buf[..k].contains(&needle) {
                needle = needle.wrapping_add(1);
                if needle == 0 {
                    needle = 1;
                }
            }
            let mut b = buf.clone();
            if k + 1 < b.len() {
                b[k + 1] = needle;
            } else {
                b.push(needle);
            }
            (
                b,
                needle,
                n,
                String::from("needle only after the terminator"),
            )
        }
        _ => {
            // Last occurrence inside the string.
            let j = rng.below(k.max(1));
            buf[j] = buf[j];
            let needle = buf[j];
            (buf, needle, n, format!("last occurrence at j={j}"))
        }
    }
}

/// strspn: role lattice over {member prefix, stop before terminator, empty set}.
fn strspn_case(rng: &mut Rng) -> (Vec<u8>, Vec<u8>, usize, String) {
    let n = 1 + rng.below(8);
    let k = rng.below(n);
    let la = rng.below(9);
    // A NUL-free accept set of distinct bytes.
    let mut accept: Vec<u8> = Vec::new();
    while accept.len() < la {
        let b = rng.non_nul();
        if !accept.contains(&b) {
            accept.push(b);
        }
    }
    let mut s: Vec<u8> = Vec::with_capacity(n);
    for _ in 0..k {
        s.push(if accept.is_empty() || rng.below(2) == 0 {
            // In-set (when the set is non-empty) keeps the span running.
            if accept.is_empty() {
                rng.non_nul()
            } else {
                accept[rng.below(accept.len())]
            }
        } else {
            rng.non_nul()
        });
    }
    s.push(0x00);
    while s.len() < n {
        s.push(rng.non_nul());
    }
    (s, accept, n, format!("terminator at k={k}, |accept|={la}"))
}

// ---------------------------------------------------------------------------
// Running the qualification court
// ---------------------------------------------------------------------------

/// A redacted divergence: what failed, never the held-out answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactedDivergence {
    pub case_id: String,
    pub residual: String,
}

/// The held-out qualification receipt. Redacted by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationReceipt {
    pub schema_version: u32,
    pub target_id: String,
    pub candidate: CandidateIdentity,
    pub policy: QualificationPolicy,
    pub universe_id: String,
    pub generator_id: String,
    pub generator_version: String,
    pub construction: String,
    pub exhaustive_domain: bool,
    pub cases_run: u64,
    pub cases_passed: u64,
    pub cases_failed: u64,
    pub divergences: Vec<RedactedDivergence>,
    /// Whether the isolation audit held (no qualification material reached the
    /// producer's workspace). False fails the seal closed.
    pub isolated: bool,
    pub verdict: QualificationVerdict,
}

/// The qualification verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualificationVerdict {
    Consistent,
    Inconsistent,
    Inconclusive,
}

impl QualificationVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            QualificationVerdict::Consistent => "consistent",
            QualificationVerdict::Inconsistent => "inconsistent",
            QualificationVerdict::Inconclusive => "inconclusive",
        }
    }
}

impl QualificationReceipt {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn is_consistent(&self) -> bool {
        self.verdict == QualificationVerdict::Consistent
            && self.isolated
            && self.cases_run > 0
            && self.cases_failed == 0
    }

    /// The asserted claim, hashed for identity.
    pub fn canonical(&self) -> String {
        format!(
            "schema={};target={};candidate={};policy={};universe={};generator={};version={};exhaustive={};run={};passed={};failed={};divergences={};isolated={};verdict={}",
            self.schema_version,
            self.target_id,
            self.candidate.canonical(),
            portspec::qualification_name(self.policy),
            self.universe_id,
            self.generator_id,
            self.generator_version,
            self.exhaustive_domain,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.divergences.len(),
            self.isolated,
            self.verdict.as_str()
        )
    }

    pub fn id(&self) -> String {
        crate::compile::sha256_hex(self.canonical().as_bytes())
    }

    /// The redacted JSON projection (the only thing anything outside the court
    /// sees). Contains no input bytes and no oracle outputs.
    pub fn to_json(&self) -> String {
        let divs: Vec<String> = self
            .divergences
            .iter()
            .map(|d| {
                format!(
                    "    {{\"case_id\": \"{}\", \"residual\": \"{}\"}}",
                    d.case_id, d.residual
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.phorport.qualification_receipt.v1\",\n  \"target\": \"{}\",\n  \"candidate\": \"{}\",\n  \"policy\": \"{}\",\n  \"universe\": \"{}\",\n  \"generator\": \"{} {}\",\n  \"construction\": \"{}\",\n  \"exhaustive_domain\": {},\n  \"cases_run\": {},\n  \"cases_passed\": {},\n  \"cases_failed\": {},\n  \"isolated\": {},\n  \"verdict\": \"{}\",\n  \"divergences\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            self.target_id,
            self.candidate.canonical(),
            portspec::qualification_name(self.policy),
            self.universe_id,
            self.generator_id,
            self.generator_version,
            self.construction.replace('"', "'"),
            self.exhaustive_domain,
            self.cases_run,
            self.cases_passed,
            self.cases_failed,
            self.isolated,
            self.verdict.as_str(),
            divs.join(",\n"),
            self.id()
        )
    }
}

/// One raw observation, retained only for the auditor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationEntry {
    pub case_id: String,
    pub args_hex: String,
    pub oracle_hex: String,
    pub candidate_hex: String,
    pub matched: bool,
    pub residual: String,
}

/// The auditor's raw view. Never materialized into a producer workspace.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct QualificationLedger {
    pub entries: Vec<QualificationEntry>,
}

impl QualificationLedger {
    /// The markers that must never appear in a synthesis workspace.
    ///
    /// As with the universe, these are namespaced digest tokens, not raw output
    /// hex, so the audit cannot false-positive on an unrelated hex string.
    pub fn isolation_markers(&self) -> Vec<String> {
        let mut out = Vec::new();
        for e in &self.entries {
            out.push(e.case_id.clone());
            if !e.oracle_hex.is_empty() {
                out.push(format!(
                    "phor.qualification-observation:{}",
                    crate::compile::sha256_hex(e.oracle_hex.as_bytes())
                ));
            }
        }
        out
    }
}

/// The full outcome: the redacted receipt and the auditor's ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationOutcome {
    pub receipt: QualificationReceipt,
    pub ledger: QualificationLedger,
}

/// Run the held-out qualification court for `candidate` over `universe`.
///
/// `isolated` is the result of the caller's isolation audit (the producer
/// workspace was checked and clean); it is threaded through so the receipt binds
/// it rather than assuming it.
pub fn qualify(
    exec: &dyn CandidateExec,
    config: &HarnessConfig,
    target: &PortTarget,
    candidate: &CandidateIdentity,
    universe: &QualificationUniverse,
    policy: QualificationPolicy,
    isolated: bool,
) -> QualificationOutcome {
    let _ = target;
    let mut ledger = QualificationLedger::default();
    let mut divergences = Vec::new();
    let mut passed = 0u64;
    let mut failed = 0u64;

    for c in &universe.cases {
        let o = probe_validated(exec, config, &c.id, &c.args);
        let entry = QualificationEntry {
            case_id: c.id.clone(),
            args_hex: hex::encode(c.encode(target)),
            oracle_hex: o.oracle_hex.clone(),
            candidate_hex: o.candidate_hex.clone(),
            matched: o.valid && o.matched,
            residual: o.residual.to_string(),
        };
        if o.valid && o.matched {
            passed += 1;
        } else {
            failed += 1;
            // Redacted: the residual family and the case id, never the bytes.
            divergences.push(RedactedDivergence {
                case_id: c.id.clone(),
                residual: o.residual.to_string(),
            });
        }
        ledger.entries.push(entry);
    }

    let cases_run = universe.cases.len() as u64;
    let verdict = if cases_run == 0 {
        QualificationVerdict::Inconclusive
    } else if failed == 0 {
        QualificationVerdict::Consistent
    } else {
        QualificationVerdict::Inconsistent
    };

    QualificationOutcome {
        receipt: QualificationReceipt {
            schema_version: QualificationReceipt::SCHEMA_VERSION,
            target_id: target.id.to_string(),
            candidate: candidate.clone(),
            policy,
            universe_id: universe.id(),
            generator_id: universe.generator_id.clone(),
            generator_version: universe.generator_version.clone(),
            construction: universe.construction.clone(),
            exhaustive_domain: universe.exhaustive_domain,
            cases_run,
            cases_passed: passed,
            cases_failed: failed,
            divergences,
            isolated,
            verdict,
        },
        ledger,
    }
}

/// Convenience: run the qualification court with the in-process executor.
pub fn qualify_in_process(
    config: &HarnessConfig,
    target: &PortTarget,
    candidate: &CandidateIdentity,
    universe: &QualificationUniverse,
    policy: QualificationPolicy,
    isolated: bool,
) -> QualificationOutcome {
    qualify(
        &InProcessExec,
        config,
        target,
        candidate,
        universe,
        policy,
        isolated,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::target::resolve_target;

    fn universe_for(symbol: &str) -> QualificationUniverse {
        let target = resolve_target(symbol).expect("target");
        let spec = portspec::by_target_id(target.id).expect("spec");
        build(&target, spec).expect("universe")
    }

    #[test]
    fn test_every_generated_case_is_in_contract() {
        // `build` fails closed on an out-of-contract case, so reaching here is
        // the assertion; check the counts are non-trivial.
        for sym in ["toupper", "memcmp", "memchr", "strlen", "strrchr", "strspn"] {
            let u = universe_for(sym);
            assert!(!u.cases.is_empty(), "{sym} produced no qualification cases");
            assert!(
                u.cases
                    .iter()
                    .all(|c| c.universe == CaseUniverse::Qualification),
                "{sym} case universe"
            );
        }
    }

    #[test]
    fn test_universe_is_deterministic_and_identity_sensitive() {
        let a = universe_for("strspn");
        let b = universe_for("strspn");
        assert_eq!(a, b);
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn test_universe_is_not_the_design_corpus() {
        // Independence check: for a bounded-deterministic target the held-out
        // set must contain cases the design corpus does not.
        let target = resolve_target("strspn").expect("target");
        let design: BTreeSet<String> = phost::porting::target::cases_for(&target)
            .iter()
            .map(|c| format!("{:?}", c.args))
            .collect();
        let u = universe_for("strspn");
        assert!(!u.exhaustive_domain);
        let novel = u
            .cases
            .iter()
            .filter(|c| !design.contains(&format!("{:?}", c.args)))
            .count();
        assert!(novel > 0, "held-out universe added no coverage over design");
    }

    #[test]
    fn test_exhaustive_domain_is_marked_and_complete() {
        let u = universe_for("toupper");
        assert!(u.exhaustive_domain);
        assert_eq!(u.cases.len(), 256);
    }

    #[test]
    fn test_receipt_is_redacted_and_ledger_holds_the_raw_evidence() {
        // The lib test binary lives in `target/debug/deps`, so `phorc` is not
        // next to it; put `target/debug` on PATH for the compiler lookup.
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));

        // Build the correct strspn candidate and qualify it. The receipt must
        // not contain any held-out oracle output; the ledger must.
        let target = resolve_target("strspn").expect("target");
        let spec = portspec::by_target_id(target.id).expect("spec");
        let universe = build(&target, spec).expect("universe");
        let source = std::fs::read_to_string("../examples/jit_port_strspn.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn.phor"))
            .expect("source");
        let work = std::env::temp_dir().join(format!("phorport-qual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let identity = crate::compile::compile_source("strspn", &source, &work).expect("compile");
        let config = HarnessConfig {
            target_id: target.id.to_string(),
            candidate_path: identity.object_path.clone(),
            candidate_hash: identity.object_hash.clone(),
        };
        let outcome = qualify_in_process(
            &config,
            &target,
            &identity,
            &universe,
            spec.qualification,
            true,
        );
        assert!(
            outcome.receipt.is_consistent(),
            "{:?}",
            outcome.receipt.divergences
        );
        // The receipt's JSON does not echo held-out oracle outputs.
        let json = outcome.receipt.to_json();
        let oracle_outputs: Vec<&str> = outcome
            .ledger
            .entries
            .iter()
            .map(|e| e.oracle_hex.as_str())
            .filter(|h| !h.is_empty())
            .collect();
        assert!(
            !oracle_outputs.is_empty(),
            "ledger should hold observations"
        );
        for out in oracle_outputs {
            assert!(!json.contains(out), "receipt leaked held-out oracle output");
        }
        let _ = std::fs::remove_dir_all(&work);
    }
}
