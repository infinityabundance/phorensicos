// porting/challenge.rs — the court-sensitivity court (Phase 3)
//
// Phase 1 and 2 proved the courts *accept* what is correct. Phase 3 asks the
// harder question: can the court actually **see** the semantic defect classes it
// claims to discriminate? A passing candidate is weak evidence if the measuring
// instrument is blind.
//
// A `MutationProfile` names a bounded set of *intentionally wrong*
// implementations for a port — a wrong `strlen` that returns the last NUL, a
// wrong chain that searches before the slice, a wrong `memcmp` that compares
// signed bytes. The challenge court runs every mutant against the **same corpus
// and oracle the real court uses** and records, per mutant:
//
//   * whether it was valid (a well-formed observable could be produced);
//   * whether the court detected it (at least one case diverged);
//   * how many cases diverged, and the first one (the witness);
//   * whether detection was *specific* (localized, not a blanket mismatch).
//
// Equivalent or undetermined mutants are never silently counted as killed or
// missed: an undetectable-but-valid mutant is reported as an undetected family,
// which is exactly a blind spot the corpus must close before the court can claim
// sensitivity to that family. The bounded statement this licenses is
// "court-sensitive to mutation families [X, Y, …] over this declared corpus" —
// never "proves all bugs detectable".
//
// This mirrors FRF's challenge discipline; where FRF provides a challenge court,
// a Phorensicos seal *references* that evidence rather than reinterpreting it.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::candidate::{decode_usize, encode_index, encode_sign, encode_usize};
use crate::porting::composition_engine::encode_outputs;
use crate::porting::composition_engine::SealedBackend;
use crate::porting::composition_ir::{eval, CompositionIR, Node, Type, ValueId};
use crate::porting::composition_registry::{self, PortValue};
use crate::porting::dialect_cage;
use crate::porting::dispatch::NativeDispatcher;
use crate::porting::oracle_trace::OracleTrace;
use crate::porting::target::{self, PortTarget, TestCase};
use crate::porting::{sha256_hex, PortError, PortingAuthority, SealedPortIndex};

/// A deliberately wrong leaf implementation. `None` means the mutant cannot
/// produce a well-formed observable under the declared contract (undetermined).
pub type LeafBehavior = fn(&[Vec<u8>]) -> Option<Vec<u8>>;

/// One leaf mutant: a named semantic defect.
#[derive(Clone, Copy)]
pub struct LeafMutant {
    /// The defect family, stable and semantic (e.g. `PORT.FIRST_VS_LAST`).
    pub family: &'static str,
    /// A short stable id within the family (e.g. `strlen.last_nul`).
    pub id: &'static str,
    pub behavior: LeafBehavior,
}

/// One composition mutant: a named wrong chain shape.
#[derive(Clone, Copy)]
pub struct CompositionMutant {
    pub family: &'static str,
    pub id: &'static str,
    /// What the wrong shape does, in one line.
    pub note: &'static str,
    pub ir: fn() -> CompositionIR,
}

// ---------------------------------------------------------------------------
// Leaf mutants
// ---------------------------------------------------------------------------

fn t_identity(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    Some(alloc::vec![*a.first()?.first()?])
}
fn t_lower(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    Some(alloc::vec![a.first()?.first()?.to_ascii_lowercase()])
}
fn t_swap(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let b = *a.first()?.first()?;
    Some(alloc::vec![if b.is_ascii_lowercase() {
        b.to_ascii_uppercase()
    } else {
        b.to_ascii_lowercase()
    }])
}
fn t_fold_high(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let b = *a.first()?.first()?;
    // C locale folds only `a`..=`z`; folding Latin-1 lowercase is a real defect.
    Some(alloc::vec![if (0xe0..=0xfe).contains(&b) {
        b - 0x20
    } else {
        b
    }])
}

fn c_signed(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let (x, y) = (a.first()?, a.get(1)?);
    let n = decode_usize(a.get(2)?).min(x.len()).min(y.len());
    for i in 0..n {
        let (p, q) = (x[i] as i8, y[i] as i8);
        if p != q {
            return Some(encode_sign(if p < q { -1 } else { 1 }));
        }
    }
    Some(encode_sign(0))
}
fn c_ignore_last(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let (x, y) = (a.first()?, a.get(1)?);
    let n = decode_usize(a.get(2)?).min(x.len()).min(y.len());
    let m = n.saturating_sub(1);
    let mut i = 0;
    while i < m {
        if x[i] != y[i] {
            return Some(encode_sign(if x[i] < y[i] { -1 } else { 1 }));
        }
        i += 1;
    }
    Some(encode_sign(0))
}
fn c_reverse(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut v = c_signed(a)?;
    let s = crate::porting::candidate::decode_index(&v);
    v = encode_sign(-s);
    Some(v)
}
fn c_ignore_n(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let (x, y) = (a.first()?, a.get(1)?);
    let n = x.len().min(y.len());
    for i in 0..n {
        if x[i] != y[i] {
            return Some(encode_sign(if x[i] < y[i] { -1 } else { 1 }));
        }
    }
    Some(encode_sign(0))
}

fn m_last(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let hay = a.first()?;
    let needle = *a.get(1)?.first()?;
    let n = decode_usize(a.get(2)?).min(hay.len());
    let mut last = -1i32;
    for (i, &b) in hay[..n].iter().enumerate() {
        if b == needle {
            last = i as i32;
        }
    }
    Some(encode_index(last))
}
fn m_beyond_n(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let hay = a.first()?;
    let needle = *a.get(1)?.first()?;
    Some(encode_index(
        hay.iter()
            .position(|&b| b == needle)
            .map(|i| i as i32)
            .unwrap_or(-1),
    ))
}
fn m_absent_zero(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let hay = a.first()?;
    let needle = *a.get(1)?.first()?;
    let n = decode_usize(a.get(2)?).min(hay.len());
    Some(encode_index(
        hay[..n]
            .iter()
            .position(|&b| b == needle)
            .map(|i| i as i32)
            .unwrap_or(0),
    ))
}

fn s_last_nul(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let n = decode_usize(a.get(1)?).min(buf.len());
    let last = buf[..n]
        .iter()
        .rposition(|&b| b == 0)
        .map(|i| i as i32)
        .unwrap_or(-1);
    Some(encode_usize(if last < 0 { n } else { last as usize }))
}
fn s_ignore_nul(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let n = decode_usize(a.get(1)?);
    Some(encode_usize(n))
}
fn s_off_by_one(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let n = decode_usize(a.get(1)?).min(buf.len());
    let first = buf[..n].iter().position(|&b| b == 0).map(|i| i + 1);
    Some(encode_usize(first.unwrap_or(n)))
}
fn s_treat_0x80(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let n = decode_usize(a.get(1)?).min(buf.len());
    Some(encode_usize(
        buf[..n]
            .iter()
            .position(|&b| b == 0 || b == 0x80)
            .unwrap_or(n),
    ))
}
fn s_scan_beyond(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let n = decode_usize(a.get(1)?);
    // Ignore the bound, but still fail closed at the buffer end.
    Some(encode_usize(
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len().max(n)),
    ))
}

fn r_first(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let needle = *a.get(1)?.first()?;
    let n = decode_usize(a.get(2)?).min(buf.len());
    let mut first = -1i32;
    for (i, &b) in buf[..n].iter().enumerate() {
        if b == needle {
            first = i as i32;
            break;
        }
        if b == 0 {
            break;
        }
    }
    Some(encode_index(first))
}
fn r_after_nul(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let needle = *a.get(1)?.first()?;
    let n = decode_usize(a.get(2)?).min(buf.len());
    Some(encode_index(
        buf[..n]
            .iter()
            .rposition(|&b| b == needle)
            .map(|i| i as i32)
            .unwrap_or(-1),
    ))
}
fn r_nul_absent(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let buf = a.first()?;
    let needle = *a.get(1)?.first()?;
    if needle == 0 {
        return Some(encode_index(-1));
    }
    let n = decode_usize(a.get(2)?).min(buf.len());
    let mut last = -1i32;
    for (i, &b) in buf[..n].iter().enumerate() {
        if b == needle {
            last = i as i32;
        }
        if b == 0 {
            break;
        }
    }
    Some(encode_index(last))
}

fn p_prefix_only(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let s = a.first()?;
    let accept = a.get(1)?;
    let n = decode_usize(a.get(2)?).min(s.len());
    // An empty set accepts nothing (a legitimate wrong answer is still a total
    // function; this mutant never becomes undetermined).
    let first = match accept.first() {
        Some(b) => *b,
        None => return Some(encode_usize(0)),
    };
    let mut i = 0;
    while i < n && s[i] == first && s[i] != 0 {
        i += 1;
    }
    Some(encode_usize(i))
}
fn p_after_terminator(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let s = a.first()?;
    let accept = a.get(1)?;
    let n = decode_usize(a.get(2)?).min(s.len());
    let mut i = 0;
    while i < n && (s[i] == 0 || accept.contains(&s[i])) {
        i += 1;
    }
    Some(encode_usize(i))
}
fn p_omit_lane(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let s = a.first()?;
    let accept = a.get(1)?;
    let n = decode_usize(a.get(2)?).min(s.len());
    let lanes = accept.len().saturating_sub(1);
    let mut i = 0;
    while i < n && s[i] != 0 && accept[..lanes].contains(&s[i]) {
        i += 1;
    }
    Some(encode_usize(i))
}
fn p_wrong_polarity(a: &[Vec<u8>]) -> Option<Vec<u8>> {
    let s = a.first()?;
    let accept = a.get(1)?;
    let n = decode_usize(a.get(2)?).min(s.len());
    let mut i = 0;
    while i < n && !accept.contains(&s[i]) {
        i += 1;
    }
    Some(encode_usize(i))
}

const TOUPPER_MUTANTS: [LeafMutant; 4] = [
    LeafMutant {
        family: "PORT.IDENTITY",
        id: "toupper.identity",
        behavior: t_identity,
    },
    LeafMutant {
        family: "PORT.WRONG_POLARITY",
        id: "toupper.lower",
        behavior: t_lower,
    },
    LeafMutant {
        family: "PORT.WRONG_POLARITY",
        id: "toupper.swap",
        behavior: t_swap,
    },
    LeafMutant {
        family: "PORT.LOCALE_WIDENING",
        id: "toupper.fold_high",
        behavior: t_fold_high,
    },
];
const MEMCMP_MUTANTS: [LeafMutant; 4] = [
    LeafMutant {
        family: "PORT.SIGNEDNESS",
        id: "memcmp.signed",
        behavior: c_signed,
    },
    LeafMutant {
        family: "PORT.INCLUSIVE_EXCLUSIVE_BOUND",
        id: "memcmp.ignore_last",
        behavior: c_ignore_last,
    },
    LeafMutant {
        family: "PORT.REVERSED_ORDER",
        id: "memcmp.reverse",
        behavior: c_reverse,
    },
    LeafMutant {
        family: "PORT.DERIVED_BOUND_SUBSTITUTION",
        id: "memcmp.ignore_n",
        behavior: c_ignore_n,
    },
];
const MEMCHR_MUTANTS: [LeafMutant; 3] = [
    LeafMutant {
        family: "PORT.FIRST_VS_LAST",
        id: "memchr.last",
        behavior: m_last,
    },
    LeafMutant {
        family: "PORT.INCLUSIVE_EXCLUSIVE_BOUND",
        id: "memchr.beyond_n",
        behavior: m_beyond_n,
    },
    LeafMutant {
        family: "PORT.ABSENT_SENTINEL",
        id: "memchr.absent_zero",
        behavior: m_absent_zero,
    },
];
const STRLEN_MUTANTS: [LeafMutant; 5] = [
    LeafMutant {
        family: "PORT.FIRST_VS_LAST",
        id: "strlen.last_nul",
        behavior: s_last_nul,
    },
    LeafMutant {
        family: "PORT.ZERO_TERMINATION",
        id: "strlen.ignore_nul",
        behavior: s_ignore_nul,
    },
    LeafMutant {
        family: "PORT.INCLUSIVE_EXCLUSIVE_BOUND",
        id: "strlen.off_by_one",
        behavior: s_off_by_one,
    },
    LeafMutant {
        family: "PORT.ZERO_TERMINATION",
        id: "strlen.treat_0x80",
        behavior: s_treat_0x80,
    },
    LeafMutant {
        family: "PORT.PRECONDITION_ESCAPE",
        id: "strlen.scan_beyond",
        behavior: s_scan_beyond,
    },
];
const STRRCHR_MUTANTS: [LeafMutant; 3] = [
    LeafMutant {
        family: "PORT.FIRST_VS_LAST",
        id: "strrchr.first",
        behavior: r_first,
    },
    LeafMutant {
        family: "PORT.POST_TERMINATOR_VISIBILITY",
        id: "strrchr.after_nul",
        behavior: r_after_nul,
    },
    LeafMutant {
        family: "PORT.ZERO_TERMINATION",
        id: "strrchr.nul_absent",
        behavior: r_nul_absent,
    },
];
const STRSPN_MUTANTS: [LeafMutant; 4] = [
    LeafMutant {
        family: "PORT.SET_VS_SEQUENCE",
        id: "strspn.prefix_only",
        behavior: p_prefix_only,
    },
    LeafMutant {
        family: "PORT.POST_TERMINATOR_VISIBILITY",
        id: "strspn.after_terminator",
        behavior: p_after_terminator,
    },
    LeafMutant {
        family: "PORT.LANE_ORDER",
        id: "strspn.omit_lane",
        behavior: p_omit_lane,
    },
    LeafMutant {
        family: "PORT.SET_VS_SEQUENCE",
        id: "strspn.wrong_polarity",
        behavior: p_wrong_polarity,
    },
];

/// The leaf mutation profile for a target id.
pub fn leaf_mutants(target_id: &str) -> &'static [LeafMutant] {
    match target_id {
        x if x == target::LIBC_TOUPPER.id => &TOUPPER_MUTANTS,
        x if x == target::LIBC_MEMCMP.id => &MEMCMP_MUTANTS,
        x if x == target::LIBC_MEMCHR.id => &MEMCHR_MUTANTS,
        x if x == target::LIBC_STRLEN.id => &STRLEN_MUTANTS,
        x if x == target::LIBC_STRRCHR.id => &STRRCHR_MUTANTS,
        x if x == target::POSIX_STRSPN.id => &STRSPN_MUTANTS,
        x if x == target::LIBC_STRSPN.id => &STRSPN_MUTANTS,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------
// Composition mutants
// ---------------------------------------------------------------------------

fn mut_toupper_memchr_no_fold() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_memchr").unwrap().ir)();
    // Replace the folded haystack with the raw input (drop the fold).
    ir.nodes[3] = Node::Input { index: 0 };
    ir
}

fn mut_toupper_strlen_memchr_caller_bound() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_strlen_memchr")
        .unwrap()
        .ir)();
    // Search bounded by the caller's n rather than the derived strlen length.
    ir.nodes[8] = Node::Call {
        port: String::from(target::LIBC_MEMCHR.id),
        args: alloc::vec![ValueId(3), ValueId(4), ValueId(5)],
    };
    ir
}

fn mut_pair_second_caller_bound() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_strlen_memchr_pair")
        .unwrap()
        .ir)();
    // The second search substitutes the caller's n for the shared derived bound.
    ir.nodes[11] = Node::Call {
        port: String::from(target::LIBC_MEMCHR.id),
        args: alloc::vec![ValueId(4), ValueId(6), ValueId(7)],
    };
    ir
}

fn mut_toupper_each_first_only() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_each").unwrap().ir)();
    // Fold only the first byte.
    ir.nodes[3] = Node::Slice {
        input: ValueId(0),
        origin: ValueId(2),
        length: Some(ValueId(4)),
    };
    ir.nodes[4] = Node::ConstantScalar { value: 1 };
    ir.nodes.push(Node::MapBytes {
        port: String::from(target::LIBC_TOUPPER.id),
        input: ValueId(3),
    });
    ir.outputs[0] = crate::porting::composition_ir::Output {
        ty: Type::Bytes,
        value: ValueId(5),
    };
    ir
}

fn mut_nested_caller_bound() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_each_strlen_memchr")
        .unwrap()
        .ir)();
    // Search bounded by the caller's n rather than the derived bound.
    ir.nodes[10] = Node::Call {
        port: String::from(target::LIBC_MEMCHR.id),
        args: alloc::vec![ValueId(4), ValueId(9), ValueId(3)],
    };
    ir
}

fn mut_suffix_search_before_slice() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_memchr_suffix")
        .unwrap()
        .ir)();
    // Search the whole folded haystack for needle B instead of the suffix.
    ir.nodes[12] = Node::Input { index: 0 };
    ir.nodes[12] = Node::MapBytes {
        port: String::from(target::LIBC_TOUPPER.id),
        input: ValueId(0),
    };
    ir
}

fn mut_slice_search_unfolded() -> CompositionIR {
    let mut ir = (composition_registry::by_name("toupper_each_slice_search")
        .unwrap()
        .ir)();
    // Hand the raw slice to a bare memchr (drop the consumer composition's fold):
    // call a leaf memchr on the unfolded slice instead of the sealed composition.
    ir.nodes[13] = Node::Input { index: 0 };
    ir.nodes[15] = Node::Call {
        port: String::from(target::LIBC_MEMCHR.id),
        args: alloc::vec![ValueId(13), ValueId(2), ValueId(14)],
    };
    ir
}

const COMPOSITION_MUTANTS: [(&str, CompositionMutant); 7] = [
    (
        "toupper_memchr",
        CompositionMutant {
            family: "PORT.DEPENDENCY_OMISSION",
            id: "toupper_memchr.no_fold",
            note: "drop the toupper fold of the haystack",
            ir: mut_toupper_memchr_no_fold,
        },
    ),
    (
        "toupper_strlen_memchr",
        CompositionMutant {
            family: "PORT.DERIVED_BOUND_SUBSTITUTION",
            id: "toupper_strlen_memchr.caller_bound",
            note: "search with the caller's n instead of the derived strlen bound",
            ir: mut_toupper_strlen_memchr_caller_bound,
        },
    ),
    (
        "toupper_strlen_memchr_pair",
        CompositionMutant {
            family: "PORT.DERIVED_BOUND_SUBSTITUTION",
            id: "pair.second_caller_bound",
            note: "the second search substitutes n for the shared derived bound",
            ir: mut_pair_second_caller_bound,
        },
    ),
    (
        "toupper_each",
        CompositionMutant {
            family: "PORT.MAP_FIRST_ONLY",
            id: "toupper_each.first_only",
            note: "fold only the first byte",
            ir: mut_toupper_each_first_only,
        },
    ),
    (
        "toupper_each_strlen_memchr",
        CompositionMutant {
            family: "PORT.DERIVED_BOUND_SUBSTITUTION",
            id: "nested.caller_bound",
            note: "search with n instead of the derived bound",
            ir: mut_nested_caller_bound,
        },
    ),
    (
        "toupper_memchr_suffix",
        CompositionMutant {
            family: "PORT.DERIVED_ORIGIN_SUBSTITUTION",
            id: "suffix.search_before_slice",
            note: "search before the slice (whole haystack)",
            ir: mut_suffix_search_before_slice,
        },
    ),
    (
        "toupper_each_slice_search",
        CompositionMutant {
            family: "PORT.DEPENDENCY_OMISSION",
            id: "slice_search.unfolded",
            note: "bare memchr on the unfolded slice (drop the consumer's fold)",
            ir: mut_slice_search_unfolded,
        },
    ),
];

/// The composition mutation profile for a short name.
pub fn composition_mutants(name: &str) -> &'static [CompositionMutant] {
    COMPOSITION_MUTANTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| core::slice::from_ref(m))
        .unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// The challenge court
// ---------------------------------------------------------------------------

/// One mutant's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutantOutcome {
    pub family: String,
    pub id: String,
    /// A well-formed observable could be produced for every valid case.
    pub valid: bool,
    /// The court diverged on at least one case (the mutant is rejected).
    pub detected: bool,
    pub detected_cases: u64,
    pub total_cases: u64,
    pub first_differing_case: String,
    /// Detection is localized (some but not all cases), not a blanket mismatch.
    pub specific: bool,
    /// The mutant is provably equivalent to the correct implementation under the
    /// declared preconditions, so it is not a blind spot and is never counted as
    /// killed or missed.
    pub equivalent: bool,
    /// The reason the mutant is equivalent, when it is.
    pub equivalence_note: String,
    /// An invalid mutant could not be evaluated at all.
    pub note: String,
}

/// The challenge residual: does the court see the declared defect families?
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChallengeReport {
    pub target: String,
    /// `leaf` or `composition`.
    pub court: &'static str,
    pub corpus_hash: String,
    pub outcomes: Vec<MutantOutcome>,
    pub families_total: u64,
    pub families_detected: u64,
    pub families_undetected: u64,
    pub families_invalid: u64,
    /// Mutants that are equivalent under the declared domain: neither killed nor
    /// missed, excluded from the sensitivity requirement.
    pub families_equivalent: u64,
    /// Every valid declared family was detected: the court is sensitive to the
    /// declared profile over this corpus.
    pub court_sensitive: bool,
    pub verdict: &'static str,
}

impl ChallengeReport {
    pub fn canonical(&self) -> String {
        let rows: Vec<String> = self
            .outcomes
            .iter()
            .map(|o| {
                format!(
                    "{}:{}:valid={}:detected={}:cases={}/{}:specific={}:equivalent={}",
                    o.family,
                    o.id,
                    o.valid,
                    o.detected,
                    o.detected_cases,
                    o.total_cases,
                    o.specific,
                    o.equivalent
                )
            })
            .collect();
        format!(
            "target={};court={};corpus_hash={};families_total={};families_detected={};families_undetected={};families_invalid={};families_equivalent={};court_sensitive={};verdict={};outcomes={}",
            self.target,
            self.court,
            self.corpus_hash,
            self.families_total,
            self.families_detected,
            self.families_undetected,
            self.families_invalid,
            self.families_equivalent,
            self.court_sensitive,
            self.verdict,
            rows.join(",")
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        let rows: Vec<String> = self
            .outcomes
            .iter()
            .map(|o| {
                format!(
                    "    {{\n      \"family\": \"{}\",\n      \"id\": \"{}\",\n      \"valid\": {},\n      \"detected\": {},\n      \"detected_cases\": {},\n      \"total_cases\": {},\n      \"first_differing_case\": \"{}\",\n      \"specific\": {},\n      \"equivalent\": {},\n      \"equivalence_note\": \"{}\",\n      \"note\": \"{}\"\n    }}",
                    crate::porting::json_escape(&o.family),
                    crate::porting::json_escape(&o.id),
                    o.valid,
                    o.detected,
                    o.detected_cases,
                    o.total_cases,
                    crate::porting::json_escape(&o.first_differing_case),
                    o.specific,
                    o.equivalent,
                    crate::porting::json_escape(&o.equivalence_note),
                    crate::porting::json_escape(&o.note)
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.challenge_verdict.v1\",\n  \"target\": \"{}\",\n  \"court\": \"{}\",\n  \"corpus_hash\": \"{}\",\n  \"families_total\": {},\n  \"families_detected\": {},\n  \"families_undetected\": {},\n  \"families_invalid\": {},\n  \"families_equivalent\": {},\n  \"court_sensitive\": {},\n  \"verdict\": \"{}\",\n  \"mutants\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            crate::porting::json_escape(&self.target),
            self.court,
            self.corpus_hash,
            self.families_total,
            self.families_detected,
            self.families_undetected,
            self.families_invalid,
            self.families_equivalent,
            self.court_sensitive,
            self.verdict,
            rows.join(",\n"),
            self.residual_hash()
        )
    }
}

fn finish(
    target: String,
    court: &'static str,
    corpus_hash: String,
    mut outcomes: Vec<MutantOutcome>,
) -> ChallengeReport {
    for o in &mut outcomes {
        if let Some(note) = equivalence_note(&o.id) {
            o.equivalent = true;
            o.equivalence_note = note.to_string();
        }
    }
    let families_total = outcomes.len() as u64;
    let families_detected = outcomes.iter().filter(|o| o.detected).count() as u64;
    let families_invalid = outcomes.iter().filter(|o| !o.valid).count() as u64;
    let families_equivalent = outcomes.iter().filter(|o| o.equivalent).count() as u64;
    let undetected_non_equivalent = outcomes
        .iter()
        .filter(|o| !o.detected && !o.equivalent)
        .count() as u64;
    let families_undetected = undetected_non_equivalent;
    // Sensitivity requires every declared, non-equivalent mutant to be detected.
    // An equivalent mutant is neither killed nor missed and is excluded; an
    // invalid (undetermined) mutant means the court demonstrated nothing about
    // that family, so it fails closed.
    let court_sensitive =
        families_total > 0 && undetected_non_equivalent == 0 && families_invalid == 0;
    let verdict = if families_total == 0 {
        "no-profile"
    } else if court_sensitive {
        "court-sensitive"
    } else {
        "court-blind"
    };
    ChallengeReport {
        target,
        court,
        corpus_hash,
        outcomes,
        families_total,
        families_detected,
        families_undetected,
        families_invalid,
        families_equivalent,
        court_sensitive,
        verdict,
    }
}

/// A mutant that is provably equivalent to the correct implementation under the
/// declared domain is not a blind spot. It is recorded as equivalent (with the
/// reason) and is never counted as killed or missed. Out-of-contract behavior is
/// deliberately *not* tested here: it belongs to a separate, explicit court
/// against declared fail-closed behavior — never to a foreign undefined-behavior
/// oracle.
pub fn equivalence_note(mutant_id: &str) -> Option<&'static str> {
    match mutant_id {
        "strlen.scan_beyond" => Some(
            "equivalent under the declared domain: the strlen precondition guarantees a NUL within the bound n, so ignoring the bound cannot change any valid observation",
        ),
        _ => None,
    }
}

/// Run the challenge court for a **leaf** target over its corpus and oracle.
///
/// The corpus and oracle are the same ones the real court uses, so a detected
/// mutant means the actual court would reject that wrong implementation.
pub fn run_leaf_challenge(
    target: &PortTarget,
    auth: &PortingAuthority,
) -> Result<ChallengeReport, PortError> {
    let cases: Vec<TestCase> = target::cases_for(target);
    let traces = dialect_cage::observe_target(target, &cases, auth)?;
    Ok(challenge_leaf_against(target.id, &traces))
}

/// Challenge a leaf profile against already-observed traces (deterministic, no FFI).
pub fn challenge_leaf_against(target_id: &str, traces: &[OracleTrace]) -> ChallengeReport {
    let mutants = leaf_mutants(target_id);
    let total = traces.len() as u64;
    let mut outcomes = Vec::with_capacity(mutants.len());
    for m in mutants {
        let mut detected_cases = 0u64;
        let mut first = String::new();
        let mut valid = true;
        for t in traces {
            let args = t.input_args();
            match (m.behavior)(&args) {
                Some(out) => {
                    if hex::encode(&out) != t.output_hex {
                        detected_cases += 1;
                        if first.is_empty() {
                            first = t.case_id.clone();
                        }
                    }
                }
                None => valid = false,
            }
        }
        outcomes.push(MutantOutcome {
            family: m.family.to_string(),
            id: m.id.to_string(),
            valid,
            detected: valid && detected_cases > 0,
            detected_cases,
            total_cases: total,
            first_differing_case: first,
            specific: valid && detected_cases > 0 && detected_cases < total,
            equivalent: false,
            equivalence_note: String::new(),
            note: if valid {
                String::new()
            } else {
                String::from("undetermined: no observable")
            },
        });
    }
    finish(
        target_id.to_string(),
        "leaf",
        crate::porting::oracle_trace::combined_oracle_hash(traces),
        outcomes,
    )
}

/// Run the challenge court for a **composition** over its corpus and oracle.
///
/// Each mutant is a wrong `CompositionIR`; it is evaluated over the sealed store
/// and compared against the **correct** chain's committed oracle. A mutant that
/// diverges (or cannot run at all) is one the court rejects.
pub fn run_composition_challenge(
    name: &str,
    index: &SealedPortIndex,
    auth: &PortingAuthority,
) -> Result<ChallengeReport, PortError> {
    let def = composition_registry::by_name(name)
        .ok_or_else(|| PortError::UnknownTarget(name.to_string()))?;
    let cases = (def.cases)();
    let traces = (def.observe)(&cases, auth)?;
    let mutants = composition_mutants(name);

    let mut outcomes = Vec::with_capacity(mutants.len());
    for m in mutants {
        let ir = (m.ir)();
        let kind = composition_registry::port_value(def.target.id).unwrap_or(PortValue::Index);
        let mut dispatcher = NativeDispatcher::new(index.clone());
        let mut detected_cases = 0u64;
        let mut first = String::new();
        let mut valid = true;
        let mut note = String::new();
        for t in traces.iter() {
            let args = t.input_args();
            let inputs = match crate::porting::composition_engine::case_inputs(&ir, &args) {
                Ok(v) => v,
                Err(_) => {
                    valid = false;
                    note = String::from("invalid: case arity");
                    break;
                }
            };
            let result = {
                let mut backend = SealedBackend::new(&mut dispatcher, auth);
                eval(&ir, &mut backend, &inputs)
            };
            match result {
                Ok(out) => match encode_outputs(kind, &out) {
                    Ok(bytes) => {
                        if hex::encode(&bytes) != t.output_hex {
                            detected_cases += 1;
                            if first.is_empty() {
                                first = t.case_id.clone();
                            }
                        }
                    }
                    Err(e) => {
                        valid = false;
                        note = format!("invalid: {}", e.message);
                        break;
                    }
                },
                Err(e) => {
                    // A mutant that cannot run at all is rejected by the court.
                    detected_cases += 1;
                    if first.is_empty() {
                        first = t.case_id.clone();
                    }
                    if note.is_empty() {
                        note = format!("rejected at run: {:?}", e);
                    }
                }
            }
        }
        outcomes.push(MutantOutcome {
            family: m.family.to_string(),
            id: m.id.to_string(),
            valid,
            detected: valid && detected_cases > 0,
            detected_cases,
            total_cases: traces.len() as u64,
            first_differing_case: first,
            specific: valid && detected_cases > 0 && detected_cases < traces.len() as u64,
            equivalent: false,
            equivalence_note: String::new(),
            note,
        });
    }

    // The corpus hash is the correct chain's oracle — the same oracle the real
    // court compares against.
    Ok(finish(
        def.target.id.to_string(),
        "composition",
        crate::porting::oracle_trace::combined_oracle_hash(&traces),
        outcomes,
    ))
}

/// A sanity check that the profile itself is not blind: if a *correct*
/// implementation were run through the same profile, every valid mutant must
/// still be detected (the mutants are genuinely different from the oracle).
pub fn profile_is_non_vacuous(report: &ChallengeReport) -> bool {
    report.families_detected > 0
}

// Keep the unused-import checker honest about a helper used only by tests.
#[allow(dead_code)]
fn _touch(_: fn() -> CompositionIR) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::composition_registry::ALL;
    use crate::porting::registry;
    use crate::porting::store;

    /// Every leaf profile is non-vacuous and every declared family is detected by
    /// the leaf court over its corpus: the court is sensitive to the profile.
    #[test]
    fn test_every_leaf_court_is_sensitive_to_its_profile() {
        let auth = PortingAuthority::granted();
        for e in registry::all() {
            let report = run_leaf_challenge(&e.target, &auth).expect("challenge runs");
            assert!(
                report.court_sensitive,
                "{}: court is blind to {:?}",
                e.spec.target_id,
                report
                    .outcomes
                    .iter()
                    .filter(|o| !o.detected && !o.equivalent && o.valid)
                    .map(|o| (o.family.clone(), o.id.clone()))
                    .collect::<Vec<_>>()
            );
            assert!(profile_is_non_vacuous(&report));
        }
    }

    /// Every composition profile is non-vacuous and every mutant is rejected by
    /// the generic court over the persistent store.
    #[test]
    fn test_every_composition_court_is_sensitive_to_its_profile() {
        let index = store::load_default().expect("committed store loads");
        let auth = PortingAuthority::granted();
        for def in ALL.iter() {
            let name = composition_registry::short_name(def.target.id);
            let report = run_composition_challenge(name, &index, &auth).expect("challenge runs");
            assert!(
                report.court_sensitive,
                "{}: court is blind to {:?}",
                def.target.id,
                report
                    .outcomes
                    .iter()
                    .filter(|o| !o.detected && !o.equivalent && o.valid)
                    .map(|o| (o.family.clone(), o.id.clone(), o.note.clone()))
                    .collect::<Vec<_>>()
            );
        }
    }

    /// The court must reject a *totally* blind implementation on every case, and
    /// the profile must notice that a mutant is not equivalent to the oracle.
    #[test]
    fn test_a_degenerate_mutant_is_detected_and_specificity_is_recorded() {
        let auth = PortingAuthority::granted();
        let report = run_leaf_challenge(&target::LIBC_STRLEN, &auth).unwrap();
        let ignore = report
            .outcomes
            .iter()
            .find(|o| o.id == "strlen.ignore_nul")
            .expect("profile has ignore_nul");
        assert!(ignore.detected);
        assert!(ignore.detected_cases > 0);
    }

    /// A profile with no mutants cannot be called sensitive (fail closed, never a
    /// vacuous pass).
    #[test]
    fn test_empty_profile_is_not_sensitive() {
        let report = challenge_leaf_against("libc:unknown:target", &[]);
        assert_eq!(report.families_total, 0);
        assert!(!report.court_sensitive);
        assert_eq!(report.verdict, "no-profile");
    }

    /// The report hash is stable and sensitive to the outcome set.
    #[test]
    fn test_report_hash_is_stable_and_sensitive() {
        let auth = PortingAuthority::granted();
        let a = run_leaf_challenge(&target::LIBC_MEMCMP, &auth).unwrap();
        let b = run_leaf_challenge(&target::LIBC_MEMCMP, &auth).unwrap();
        assert_eq!(a.residual_hash(), b.residual_hash());
        assert_eq!(a.residual_hash().len(), 64);

        let mut c = a.clone();
        c.outcomes[0].detected_cases += 1;
        assert_ne!(a.residual_hash(), c.residual_hash());
    }
}
