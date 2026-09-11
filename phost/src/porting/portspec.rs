// porting/portspec.rs — the canonical, typed port specification
//
// Phase 1 of the autonomous porting foundry (docs/AUTONOMOUS_PORTING_ARCHITECTURE.md).
//
// The bootstrap `PortTarget` is a bag of strings: it can *describe* a foreign
// surface, but it cannot *separate* the contract from the implementation that was
// observed, and it cannot be content-addressed in a way a synthesizer, a mutation
// profile, a precondition validator and a qualification policy can all bind to.
//
// A `PortSpec` makes those distinctions structural:
//
//   * what contract is being reconstructed     (`ContractSource`)
//   * what is observed, precisely              (`ObservableSpec`)
//   * how a raw observation is normalized      (`ObservationProjectionSpec`)
//   * what a case must satisfy to be valid     (`PreconditionSpec`)
//   * how the ABI packs the arguments          (`AbiSpec`)
//   * how cases are generated                  (`CaseSpaceSpec`)
//   * what the candidate may not do            (`CandidateConstraints`)
//   * how strongly the result is qualified     (`QualificationPolicy`)
//
// Everything is `&'static` data (the specs are compile-time constants), so a
// `PortSpec` is `Copy` and lives in the same `no_std + alloc` closure as the rest
// of the runtime. The content identity is a domain-separated canonical encoding
// followed by SHA-256; JSON is a projection, never the authority.
//
// This module is additive: it does not change `target.rs`, the courts, or any
// existing evidence. `PortSpec::of_target` derives the spec for a bootstrap
// target, and `portspec::all()` returns the six leaves. A later subphase removes
// the target-id branches from the generic engine; that migration is versioned and
// must preserve the baseline (Gate A).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::porting::target::PortTarget;

/// The identity domain tag for canonical encoding. Bumping this changes every
/// `PortSpecId` and is a deliberate, versioned act.
pub const PORTSPEC_DOMAIN: &[u8] = b"PHOR/PORTSPEC/v1\0";

/// The specification version carried by every spec (distinct from the target's
/// `v1` contract version, which is part of the qualified id).
pub const PORTSPEC_VERSION: &str = "v1";

/// Positional argument identity within a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgId(pub u8);

/// The specification a behavioral contract is drawn from.
///
/// This is deliberately distinct from the library that was observed: a POSIX
/// contract observed through glibc is not the statement "glibc defines the
/// contract". See docs/DIALECT_QUALIFICATION.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractSource {
    /// An ISO C surface (`toupper`, `memcmp`, `memchr`, `strlen`, `strrchr`).
    IsoC,
    /// A POSIX surface not specified by ISO C (`strspn`).
    Posix,
    /// A Phorensic composition over already-sealed ports.
    PhorensicComposition,
    /// Implementation-defined behavior, observed on a named implementation family.
    ImplementationDefined { family: &'static str },
}

impl ContractSource {
    fn tag(&self) -> u8 {
        match self {
            ContractSource::IsoC => 1,
            ContractSource::Posix => 2,
            ContractSource::PhorensicComposition => 3,
            ContractSource::ImplementationDefined { .. } => 4,
        }
    }
}

/// The semantic shape of what the court observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservableSpec {
    /// Exact output bytes, compared byte for byte.
    ExactBytes,
    /// An unsigned integer of the given bit width.
    UnsignedInteger { bits: u16 },
    /// A signed integer of the given bit width.
    SignedInteger { bits: u16 },
    /// The sign of an integer result (`-1 | 0 | 1`), the C ordering contract.
    Sign,
    /// An index into an argument buffer, or `-1` when absent.
    IndexOrAbsent,
    /// A LENGTH: a non-negative count of bytes.
    Length,
}

impl ObservableSpec {
    fn tag(&self) -> u8 {
        match self {
            ObservableSpec::ExactBytes => 1,
            ObservableSpec::UnsignedInteger { .. } => 2,
            ObservableSpec::SignedInteger { .. } => 3,
            ObservableSpec::Sign => 4,
            ObservableSpec::IndexOrAbsent => 5,
            ObservableSpec::Length => 6,
        }
    }
}

/// How a raw oracle observation is normalized to the observable.
///
/// Normalization is part of the question, so it must be sealed, never applied
/// silently. A pointer result is only portable (and only composable) once it is
/// normalized to an index; a comparison is only the C contract once it is
/// normalized to a sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationProjectionSpec {
    /// The observation is already the observable.
    Identity,
    /// Keep the low 8 bits of the machine return value.
    LowByte,
    /// Map an integer result to its sign.
    RawSign,
    /// Map a returned pointer to its offset from the base of argument `base_arg`.
    PointerToIndex { base_arg: ArgId },
}

impl ObservationProjectionSpec {
    fn tag(&self) -> u8 {
        match self {
            ObservationProjectionSpec::Identity => 1,
            ObservationProjectionSpec::LowByte => 2,
            ObservationProjectionSpec::RawSign => 3,
            ObservationProjectionSpec::PointerToIndex { .. } => 4,
        }
    }
}

/// A machine-checkable constraint a case must satisfy before the foreign oracle
/// is invoked.
///
/// This is load-bearing: a differential court contaminated by undefined or
/// out-of-contract behavior is not useful evidence. Only a case that passes
/// [`validate_case`] reaches foreign execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreconditionSpec {
    /// `len_arg`'s value must not exceed the length of `buffer_arg`.
    LengthWithinBuffer { len_arg: ArgId, buffer_arg: ArgId },
    /// The first NUL of `buffer_arg` must lie within the first `bound_arg` bytes.
    NulWithinBound { buffer_arg: ArgId, bound_arg: ArgId },
    /// `arg` must contain no NUL byte.
    NulFree { arg: ArgId },
    /// `arg`'s value must lie in `[min, max]`.
    IntegerRange { arg: ArgId, min: u64, max: u64 },
}

impl PreconditionSpec {
    fn tag(&self) -> u8 {
        match self {
            PreconditionSpec::LengthWithinBuffer { .. } => 1,
            PreconditionSpec::NulWithinBound { .. } => 2,
            PreconditionSpec::NulFree { .. } => 3,
            PreconditionSpec::IntegerRange { .. } => 4,
        }
    }
}

/// The declared ABI the compiled candidate exposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgPacking {
    /// Each argument is a scalar word.
    Words,
    /// A buffer is packed little-endian into one word (byte `i` in bits `8*i`).
    LittleEndianPrefix,
    /// A buffer is packed big-endian into one word.
    BigEndianPrefix,
    /// A buffer is zero-extended into one word.
    ZeroExtended,
}

impl ArgPacking {
    fn tag(&self) -> u8 {
        match self {
            ArgPacking::Words => 1,
            ArgPacking::LittleEndianPrefix => 2,
            ArgPacking::BigEndianPrefix => 3,
            ArgPacking::ZeroExtended => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbiSpec {
    /// The symbol `phorc` emits the entry point for, as `_phor_<symbol>`.
    pub symbol: &'static str,
    /// The argument packing used by the leaf ABI.
    pub packing: ArgPacking,
}

/// How the case corpus for this target is produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseSpaceKind {
    /// The complete finite input domain (e.g. all 256 bytes).
    ExhaustiveFinite,
    /// A bounded, deterministic, specification-derived corpus.
    BoundedDeterministic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseSpaceSpec {
    pub kind: CaseSpaceKind,
    /// The generator function that enumerates the corpus (Phase 1 keeps the
    /// existing generators; this is the extension boundary, not a new engine).
    pub generator: &'static str,
    pub summary: &'static str,
}

/// What the candidate producer may not do. Exists to bound the search and keep
/// the artifact auditable; it is not a correctness claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateConstraints {
    pub max_source_bytes: u32,
    /// The execution court requires a leaf function: no calls, no external symbols.
    pub leaf_only: bool,
    /// The execution court requires no relocations inside the entry point.
    pub no_relocations: bool,
}

impl CandidateConstraints {
    pub const LEAF: CandidateConstraints = CandidateConstraints {
        max_source_bytes: 64 * 1024,
        leaf_only: true,
        no_relocations: true,
    };
}

/// How strongly a result is qualified. Phase 1 recorded the honest original
/// state (one implementation observed). Phase 7 extends the policy to the
/// implementation axis: a `PortSpec` may require multiple independent
/// implementation or environment witnesses to agree (never a majority vote).
///
/// The names describe what is *observed*: none of these proves the
/// specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualificationPolicy {
    /// Observed through one implementation.
    HostObserved,
    /// Observed through one implementation family, explicitly named.
    ImplementationFamily { family: &'static str },
    /// At least `minimum_witnesses` independent implementations must agree.
    MultiImplementation { minimum_witnesses: u16 },
    /// At least `minimum_environments` distinct environments must agree.
    MultiEnvironment { minimum_environments: u16 },
    /// A portable candidate: at least `minimum_witnesses` implementations agree.
    PortableCandidate { minimum_witnesses: u16 },
}

impl QualificationPolicy {
    pub fn tag(&self) -> u8 {
        match self {
            QualificationPolicy::HostObserved => 1,
            QualificationPolicy::ImplementationFamily { .. } => 2,
            QualificationPolicy::MultiImplementation { .. } => 3,
            QualificationPolicy::MultiEnvironment { .. } => 4,
            QualificationPolicy::PortableCandidate { .. } => 5,
        }
    }

    /// The minimum number of independent witnesses this policy requires.
    ///
    /// `HostObserved` and `ImplementationFamily` name a single witness; the
    /// multi-* policies require their declared count, never fewer than two.
    pub fn required_witnesses(self) -> u16 {
        match self {
            QualificationPolicy::HostObserved => 1,
            QualificationPolicy::ImplementationFamily { .. } => 1,
            QualificationPolicy::MultiImplementation { minimum_witnesses } => {
                minimum_witnesses.max(2)
            }
            QualificationPolicy::MultiEnvironment {
                minimum_environments,
            } => minimum_environments.max(2),
            QualificationPolicy::PortableCandidate { minimum_witnesses } => {
                minimum_witnesses.max(2)
            }
        }
    }
}

/// The canonical specification of one foreign API surface.
///
/// Everything is static data; the identity is `PortSpecId` (§[`PortSpec::id`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortSpec {
    /// The qualified target id (`dialect:symbol:locale:contract:version`).
    pub target_id: &'static str,
    pub dialect: &'static str,
    pub symbol: &'static str,
    pub version: &'static str,
    pub locale_contract: &'static str,
    pub contract: ContractSource,
    pub observable: ObservableSpec,
    pub projection: ObservationProjectionSpec,
    pub preconditions: &'static [PreconditionSpec],
    pub abi: AbiSpec,
    pub case_space: CaseSpaceSpec,
    pub qualification: QualificationPolicy,
    pub candidate: CandidateConstraints,
    /// The clean-room candidate source bound into the seal (repo-relative).
    pub candidate_source: &'static str,
}

// ---------------------------------------------------------------------------
// Canonical encoding
// ---------------------------------------------------------------------------

/// A deterministic, length-prefixed byte encoder. Every value is unambiguous:
/// counts and lengths are fixed-width little-endian, so no delimiter can be
/// smuggled through a string.
#[derive(Default)]
struct Canon {
    buf: Vec<u8>,
}

impl Canon {
    fn new() -> Self {
        Canon {
            buf: Vec::with_capacity(256),
        }
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

fn encode_arg_id(c: &mut Canon, a: ArgId) {
    c.u8(a.0);
}

/// The canonical byte encoding of a spec, **excluding** the domain tag.
///
/// Fields are emitted in a fixed order; adding a field is a versioned change
/// (bump [`PORTSPEC_DOMAIN`]) because it changes every identity.
pub fn canonical_bytes(spec: &PortSpec) -> Vec<u8> {
    let mut c = Canon::new();
    c.str(PORTSPEC_VERSION);
    c.str(spec.target_id);
    c.str(spec.dialect);
    c.str(spec.symbol);
    c.str(spec.version);
    c.str(spec.locale_contract);

    c.u8(spec.contract.tag());
    if let ContractSource::ImplementationDefined { family } = spec.contract {
        c.str(family);
    }

    c.u8(spec.observable.tag());
    match spec.observable {
        ObservableSpec::UnsignedInteger { bits } | ObservableSpec::SignedInteger { bits } => {
            c.u16(bits);
        }
        _ => {}
    }

    c.u8(spec.projection.tag());
    if let ObservationProjectionSpec::PointerToIndex { base_arg } = spec.projection {
        encode_arg_id(&mut c, base_arg);
    }

    c.u32(spec.preconditions.len() as u32);
    for p in spec.preconditions {
        c.u8(p.tag());
        match *p {
            PreconditionSpec::LengthWithinBuffer {
                len_arg,
                buffer_arg,
            } => {
                encode_arg_id(&mut c, len_arg);
                encode_arg_id(&mut c, buffer_arg);
            }
            PreconditionSpec::NulWithinBound {
                buffer_arg,
                bound_arg,
            } => {
                encode_arg_id(&mut c, buffer_arg);
                encode_arg_id(&mut c, bound_arg);
            }
            PreconditionSpec::NulFree { arg } => encode_arg_id(&mut c, arg),
            PreconditionSpec::IntegerRange { arg, min, max } => {
                encode_arg_id(&mut c, arg);
                c.u64(min);
                c.u64(max);
            }
        }
    }

    c.str(spec.abi.symbol);
    c.u8(spec.abi.packing.tag());

    c.u8(match spec.case_space.kind {
        CaseSpaceKind::ExhaustiveFinite => 1,
        CaseSpaceKind::BoundedDeterministic => 2,
    });
    c.str(spec.case_space.generator);
    c.str(spec.case_space.summary);

    c.u8(spec.qualification.tag());
    match spec.qualification {
        QualificationPolicy::HostObserved => {}
        QualificationPolicy::ImplementationFamily { family } => c.str(family),
        QualificationPolicy::MultiImplementation { minimum_witnesses }
        | QualificationPolicy::PortableCandidate { minimum_witnesses } => c.u16(minimum_witnesses),
        QualificationPolicy::MultiEnvironment {
            minimum_environments,
        } => c.u16(minimum_environments),
    }

    c.u32(spec.candidate.max_source_bytes);
    c.u8(spec.candidate.leaf_only as u8);
    c.u8(spec.candidate.no_relocations as u8);

    c.str(spec.candidate_source);
    c.buf
}

/// The domain-separated content identity preimage.
pub fn id_preimage(spec: &PortSpec) -> Vec<u8> {
    let mut v = Vec::with_capacity(PORTSPEC_DOMAIN.len() + 256);
    v.extend_from_slice(PORTSPEC_DOMAIN);
    v.extend_from_slice(&canonical_bytes(spec));
    v
}

impl PortSpec {
    /// The content identity: `SHA-256(PHOR/PORTSPEC/v1\0 || canonical_bytes)`.
    ///
    /// Deliberately *not* a hash of pretty JSON: JSON is a human projection, and
    /// hashing it would let key order or formatting leak into identity.
    pub fn id(&self) -> String {
        crate::porting::sha256_hex(&id_preimage(self))
    }

    /// A human-readable JSON projection. Never the authority; never hashed.
    pub fn to_json(&self) -> String {
        let pre: Vec<String> = self
            .preconditions
            .iter()
            .map(|p| format!("\"{}\"", precondition_name(p)))
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.portspec.v1\",\n  \"target_id\": \"{}\",\n  \"spec_version\": \"{}\",\n  \"contract\": \"{}\",\n  \"observable\": \"{}\",\n  \"projection\": \"{}\",\n  \"preconditions\": [{}],\n  \"abi\": {{ \"symbol\": \"{}\", \"packing\": \"{}\" }},\n  \"case_space\": \"{}\",\n  \"generator\": \"{}\",\n  \"qualification\": \"{}\",\n  \"candidate\": {{ \"max_source_bytes\": {}, \"leaf_only\": {}, \"no_relocations\": {} }},\n  \"candidate_source\": \"{}\",\n  \"id\": \"{}\"\n}}\n",
            self.target_id,
            PORTSPEC_VERSION,
            contract_name(self.contract),
            observable_name(self.observable),
            projection_name(self.projection),
            pre.join(", "),
            self.abi.symbol,
            packing_name(self.abi.packing),
            case_space_name(self.case_space.kind),
            self.case_space.generator,
            qualification_name(self.qualification),
            self.candidate.max_source_bytes,
            self.candidate.leaf_only,
            self.candidate.no_relocations,
            self.candidate_source,
            self.id(),
        )
    }
}

fn contract_name(c: ContractSource) -> &'static str {
    match c {
        ContractSource::IsoC => "iso-c",
        ContractSource::Posix => "posix",
        ContractSource::PhorensicComposition => "phorensic-composition",
        ContractSource::ImplementationDefined { family } => family,
    }
}

fn observable_name(o: ObservableSpec) -> &'static str {
    match o {
        ObservableSpec::ExactBytes => "exact-bytes",
        ObservableSpec::UnsignedInteger { .. } => "unsigned-integer",
        ObservableSpec::SignedInteger { .. } => "signed-integer",
        ObservableSpec::Sign => "sign",
        ObservableSpec::IndexOrAbsent => "index-or-absent",
        ObservableSpec::Length => "length",
    }
}

fn projection_name(p: ObservationProjectionSpec) -> &'static str {
    match p {
        ObservationProjectionSpec::Identity => "identity",
        ObservationProjectionSpec::LowByte => "low-byte",
        ObservationProjectionSpec::RawSign => "raw-sign",
        ObservationProjectionSpec::PointerToIndex { .. } => "pointer-to-index",
    }
}

fn precondition_name(p: &PreconditionSpec) -> &'static str {
    match p {
        PreconditionSpec::LengthWithinBuffer { .. } => "length-within-buffer",
        PreconditionSpec::NulWithinBound { .. } => "nul-within-bound",
        PreconditionSpec::NulFree { .. } => "nul-free",
        PreconditionSpec::IntegerRange { .. } => "integer-range",
    }
}

fn packing_name(p: ArgPacking) -> &'static str {
    match p {
        ArgPacking::Words => "words",
        ArgPacking::LittleEndianPrefix => "little-endian-prefix",
        ArgPacking::BigEndianPrefix => "big-endian-prefix",
        ArgPacking::ZeroExtended => "zero-extended",
    }
}

fn case_space_name(k: CaseSpaceKind) -> &'static str {
    match k {
        CaseSpaceKind::ExhaustiveFinite => "exhaustive-finite",
        CaseSpaceKind::BoundedDeterministic => "bounded-deterministic",
    }
}

pub fn qualification_name(q: QualificationPolicy) -> &'static str {
    match q {
        QualificationPolicy::HostObserved => "host-observed",
        QualificationPolicy::ImplementationFamily { .. } => "implementation-family",
        QualificationPolicy::MultiImplementation { .. } => "multi-implementation",
        QualificationPolicy::MultiEnvironment { .. } => "multi-environment",
        QualificationPolicy::PortableCandidate { .. } => "portable-candidate",
    }
}

// ---------------------------------------------------------------------------
// Precondition validation
// ---------------------------------------------------------------------------

/// A case that has passed every declared precondition. Only this type reaches
/// foreign execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCase {
    pub args: Vec<Vec<u8>>,
}

/// Why a case is out of contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreconditionViolation {
    /// A named argument index is absent.
    MissingArg { arg: ArgId },
    /// A length argument exceeds its buffer.
    LengthExceedsBuffer { len_arg: ArgId, buffer_arg: ArgId },
    /// No NUL was found within the declared bound.
    NulNotWithinBound { buffer_arg: ArgId, bound_arg: ArgId },
    /// A required NUL-free argument contained a NUL.
    NulInNulFreeArg { arg: ArgId },
    /// An integer argument fell outside its declared range.
    IntegerOutOfRange { arg: ArgId },
}

fn arg<'a>(args: &'a [Vec<u8>], a: ArgId) -> Result<&'a Vec<u8>, PreconditionViolation> {
    args.get(a.0 as usize)
        .ok_or(PreconditionViolation::MissingArg { arg: a })
}

fn arg_u64(args: &[Vec<u8>], a: ArgId) -> Result<u64, PreconditionViolation> {
    let v = arg(args, a)?;
    Ok(u64::from_le_bytes(match v.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return Err(PreconditionViolation::MissingArg { arg: a }),
    }))
}

/// Validate `args` against `spec`'s preconditions.
///
/// The order is the declared order, so the *first* violated precondition is
/// reported deterministically.
pub fn validate_case(
    spec: &PortSpec,
    args: &[Vec<u8>],
) -> Result<ValidatedCase, PreconditionViolation> {
    for p in spec.preconditions {
        match *p {
            PreconditionSpec::LengthWithinBuffer {
                len_arg,
                buffer_arg,
            } => {
                let n = arg_u64(args, len_arg)? as usize;
                let buf = arg(args, buffer_arg)?;
                if n > buf.len() {
                    return Err(PreconditionViolation::LengthExceedsBuffer {
                        len_arg,
                        buffer_arg,
                    });
                }
            }
            PreconditionSpec::NulWithinBound {
                buffer_arg,
                bound_arg,
            } => {
                let n = arg_u64(args, bound_arg)? as usize;
                let buf = arg(args, buffer_arg)?;
                let found = buf.iter().take(n).any(|&b| b == 0);
                if !found {
                    return Err(PreconditionViolation::NulNotWithinBound {
                        buffer_arg,
                        bound_arg,
                    });
                }
            }
            PreconditionSpec::NulFree { arg: a } => {
                let buf = arg(args, a)?;
                if buf.iter().any(|&b| b == 0) {
                    return Err(PreconditionViolation::NulInNulFreeArg { arg: a });
                }
            }
            PreconditionSpec::IntegerRange { arg: a, min, max } => {
                let v = arg_u64(args, a)?;
                if v < min || v > max {
                    return Err(PreconditionViolation::IntegerOutOfRange { arg: a });
                }
            }
        }
    }
    Ok(ValidatedCase {
        args: args.to_vec(),
    })
}

// ---------------------------------------------------------------------------
// The six leaves
// ---------------------------------------------------------------------------

const NO_PRECONDITIONS: &[PreconditionSpec] = &[];
const MEMCMP_PRECONDITIONS: &[PreconditionSpec] = &[
    PreconditionSpec::LengthWithinBuffer {
        len_arg: ArgId(2),
        buffer_arg: ArgId(0),
    },
    PreconditionSpec::LengthWithinBuffer {
        len_arg: ArgId(2),
        buffer_arg: ArgId(1),
    },
];
const MEMCHR_PRECONDITIONS: &[PreconditionSpec] = &[PreconditionSpec::LengthWithinBuffer {
    len_arg: ArgId(2),
    buffer_arg: ArgId(0),
}];
const STRLEN_PRECONDITIONS: &[PreconditionSpec] = &[PreconditionSpec::NulWithinBound {
    buffer_arg: ArgId(0),
    bound_arg: ArgId(1),
}];
const STRRCHR_PRECONDITIONS: &[PreconditionSpec] = &[PreconditionSpec::NulWithinBound {
    buffer_arg: ArgId(0),
    bound_arg: ArgId(2),
}];
const STRSPN_PRECONDITIONS: &[PreconditionSpec] = &[
    PreconditionSpec::NulWithinBound {
        buffer_arg: ArgId(0),
        bound_arg: ArgId(2),
    },
    PreconditionSpec::NulFree { arg: ArgId(1) },
];

/// `libc:toupper:c-locale:u8:v1`.
pub const SPEC_TOUPPER: PortSpec = PortSpec {
    target_id: "libc:toupper:c-locale:u8:v1",
    dialect: "libc",
    symbol: "toupper",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::IsoC,
    observable: ObservableSpec::UnsignedInteger { bits: 8 },
    projection: ObservationProjectionSpec::LowByte,
    preconditions: NO_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_toupper",
        packing: ArgPacking::Words,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::ExhaustiveFinite,
        generator: "byte_domain_cases",
        summary: "exhaustive 0..=255",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_toupper.phor",
};

/// `libc:memcmp:c-locale:sign:v1`.
pub const SPEC_MEMCMP: PortSpec = PortSpec {
    target_id: "libc:memcmp:c-locale:sign:v1",
    dialect: "libc",
    symbol: "memcmp",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::IsoC,
    observable: ObservableSpec::Sign,
    projection: ObservationProjectionSpec::RawSign,
    preconditions: MEMCMP_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_memcmp_sign",
        packing: ArgPacking::BigEndianPrefix,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::BoundedDeterministic,
        generator: "memcmp_corpus",
        summary:
            "lengths 0..=8, five patterns, every mismatch position, n-boundary, unsigned edge bytes",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_memcmp.phor",
};

/// `libc:memchr:c-locale:index:v1`.
pub const SPEC_MEMCHR: PortSpec = PortSpec {
    target_id: "libc:memchr:c-locale:index:v1",
    dialect: "libc",
    symbol: "memchr",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::IsoC,
    observable: ObservableSpec::IndexOrAbsent,
    projection: ObservationProjectionSpec::PointerToIndex { base_arg: ArgId(0) },
    preconditions: MEMCHR_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_memchr_index",
        packing: ArgPacking::LittleEndianPrefix,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::BoundedDeterministic,
        generator: "memchr_corpus",
        summary: "lengths 0..=8, first match at every index, absent needle, repeated needles, n-boundary, edge bytes, exhaustive needle sweep",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_memchr.phor",
};

/// `libc:strlen:c-locale:u64:v1`.
pub const SPEC_STRLEN: PortSpec = PortSpec {
    target_id: "libc:strlen:c-locale:u64:v1",
    dialect: "libc",
    symbol: "strlen",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::IsoC,
    observable: ObservableSpec::Length,
    projection: ObservationProjectionSpec::Identity,
    preconditions: STRLEN_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_strlen_len",
        packing: ArgPacking::LittleEndianPrefix,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::BoundedDeterministic,
        generator: "strlen_corpus",
        summary: "the complete (terminator-index k, scan-bound n) grid 0 <= k < n <= 8, non-NUL fillers, ignored tails, exhaustive 0..=255 non-terminator sweep",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_strlen.phor",
};

/// `libc:strrchr:c-locale:index:v1`.
pub const SPEC_STRRCHR: PortSpec = PortSpec {
    target_id: "libc:strrchr:c-locale:index:v1",
    dialect: "libc",
    symbol: "strrchr",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::IsoC,
    observable: ObservableSpec::IndexOrAbsent,
    projection: ObservationProjectionSpec::PointerToIndex { base_arg: ArgId(0) },
    preconditions: STRRCHR_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_strrchr_index",
        packing: ArgPacking::ZeroExtended,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::BoundedDeterministic,
        generator: "strrchr_corpus",
        summary: "unique and repeated last occurrences, needles after the terminator, NUL needle, exhaustive needle sweep",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_strrchr.phor",
};

/// `posix:strspn:c-locale:u64:v1`.
pub const SPEC_STRSPN: PortSpec = PortSpec {
    target_id: "posix:strspn:c-locale:u64:v1",
    dialect: "posix",
    symbol: "strspn",
    version: "host-observed-v1",
    locale_contract: "C",
    contract: ContractSource::Posix,
    observable: ObservableSpec::Length,
    projection: ObservationProjectionSpec::Identity,
    preconditions: STRSPN_PRECONDITIONS,
    abi: AbiSpec {
        symbol: "phor_strspn_len",
        packing: ArgPacking::LittleEndianPrefix,
    },
    case_space: CaseSpaceSpec {
        kind: CaseSpaceKind::BoundedDeterministic,
        generator: "strspn_corpus",
        summary: "the complete (span, n) grid, empty set/string, every set size 1..=8, disjoint set, two exhaustive 0..=255 sweeps",
    },
    qualification: QualificationPolicy::HostObserved,
    candidate: CandidateConstraints::LEAF,
    candidate_source: "examples/jit_port_strspn.phor",
};

/// Every leaf spec, in the store's canonical order.
pub const ALL: [PortSpec; 6] = [
    SPEC_TOUPPER,
    SPEC_MEMCMP,
    SPEC_MEMCHR,
    SPEC_STRLEN,
    SPEC_STRRCHR,
    SPEC_STRSPN,
];

/// Every leaf spec, as a slice.
pub fn all() -> &'static [PortSpec] {
    &ALL
}

/// Look a spec up by qualified target id.
pub fn by_target_id(id: &str) -> Option<&'static PortSpec> {
    ALL.iter().find(|s| s.target_id == id)
}

impl PortSpec {
    /// Derive the spec for a bootstrap [`PortTarget`].
    ///
    /// This is the bridge for the migration: every existing target already has a
    /// spec in [`ALL`], so this asserts the two agree rather than re-deriving a
    /// weaker object. It returns `None` for a target with no spec, so a new
    /// target cannot enter the engine without a spec.
    pub fn of_target(t: &PortTarget) -> Option<&'static PortSpec> {
        by_target_id(t.id)
    }
}

/// Validate `args` for the target with `target_id`.
pub fn validate_for_target(
    target_id: &str,
    args: &[Vec<u8>],
) -> Option<Result<ValidatedCase, PreconditionViolation>> {
    by_target_id(target_id).map(|s| validate_case(s, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The content identities are pinned. Any change to a semantic field — or to
    /// the canonical encoding — changes them, so this is the seal's tripwire.
    #[test]
    fn test_spec_ids_are_pinned_golden() {
        let golden = [
            (
                "libc:toupper:c-locale:u8:v1",
                "a375d704d504131f164e79a3e7f476a27022604c7a9589aff14b5025df6d93e7",
            ),
            (
                "libc:memcmp:c-locale:sign:v1",
                "fde427474312a5329f30d16fdfdd3ba38f0c690c01e4b606be98c7da420eb54f",
            ),
            (
                "libc:memchr:c-locale:index:v1",
                "c41d3de23d13e775427c1237e6cc1d47489b638a92435e7b63de0938befe212d",
            ),
            (
                "libc:strlen:c-locale:u64:v1",
                "c5f0882dec71150f33a1d8e161eed10f65eafc5e6df4dc81feed893c1aa0e61c",
            ),
            (
                "libc:strrchr:c-locale:index:v1",
                "a5c0fcd53f17d975fcd9910c2c4f2c0a85aa4eb1f90154cbbc0ce60577f623ee",
            ),
            (
                "posix:strspn:c-locale:u64:v1",
                "bc0420f01bad52131db35d97636f9e31d55c7903585498ace718524e306f4c23",
            ),
        ];
        for (id, want) in golden {
            let s = by_target_id(id).expect("spec exists");
            assert_eq!(s.id(), want, "{} spec id drifted", id);
        }
    }

    #[test]
    fn test_all_six_leaves_have_distinct_specs_and_ids() {
        assert_eq!(all().len(), 6);
        let mut ids: Vec<String> = all().iter().map(|s| s.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 6, "spec ids must be unique");

        // Every spec's dialect is the namespace of its target id.
        for s in all() {
            let ns = s.target_id.split(':').next().unwrap();
            assert_eq!(s.dialect, ns, "{} dialect/namespace mismatch", s.target_id);
        }
    }

    #[test]
    fn test_id_is_stable_and_domain_separated() {
        let a = SPEC_TOUPPER.id();
        let b = SPEC_TOUPPER.id();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);

        // A different spec has a different id.
        assert_ne!(a, SPEC_MEMCMP.id());

        // The preimage carries the domain tag and the canonical bytes.
        let pre = id_preimage(&SPEC_TOUPPER);
        assert!(pre.starts_with(PORTSPEC_DOMAIN));
        assert_eq!(
            pre[PORTSPEC_DOMAIN.len()..],
            canonical_bytes(&SPEC_TOUPPER)[..]
        );
    }

    /// The identity must change when any semantic field changes. This is the
    /// property the seal depends on.
    #[test]
    fn test_id_changes_when_a_semantic_field_changes() {
        let base = SPEC_MEMCMP.id();

        let mut other = SPEC_MEMCMP;
        other.observable = ObservableSpec::IndexOrAbsent;
        assert_ne!(other.id(), base, "observable is not hashed");

        let mut other = SPEC_MEMCMP;
        other.projection = ObservationProjectionSpec::Identity;
        assert_ne!(other.id(), base, "projection is not hashed");

        let mut other = SPEC_MEMCMP;
        other.preconditions = NO_PRECONDITIONS;
        assert_ne!(other.id(), base, "preconditions are not hashed");

        let mut other = SPEC_MEMCMP;
        other.abi.packing = ArgPacking::Words;
        assert_ne!(other.id(), base, "abi packing is not hashed");

        let mut other = SPEC_MEMCMP;
        other.contract = ContractSource::Posix;
        assert_ne!(other.id(), base, "contract source is not hashed");

        let mut other = SPEC_MEMCMP;
        other.qualification = QualificationPolicy::HostObserved; // unchanged
        assert_eq!(other.id(), base, "no-op must not change the id");
    }

    #[test]
    fn test_canonical_encoding_is_unambiguous_across_argument_order() {
        // Two specs differing only by the ORDER of a two-precondition list must
        // hash differently: the list is ordered, not a set.
        const A: &[PreconditionSpec] = &[
            PreconditionSpec::LengthWithinBuffer {
                len_arg: ArgId(2),
                buffer_arg: ArgId(0),
            },
            PreconditionSpec::LengthWithinBuffer {
                len_arg: ArgId(2),
                buffer_arg: ArgId(1),
            },
        ];
        const B: &[PreconditionSpec] = &[
            PreconditionSpec::LengthWithinBuffer {
                len_arg: ArgId(2),
                buffer_arg: ArgId(1),
            },
            PreconditionSpec::LengthWithinBuffer {
                len_arg: ArgId(2),
                buffer_arg: ArgId(0),
            },
        ];
        let mut x = SPEC_MEMCMP;
        x.preconditions = A;
        let mut y = SPEC_MEMCMP;
        y.preconditions = B;
        assert_ne!(x.id(), y.id());
    }

    #[test]
    fn test_validate_memcmp_lengths_within_buffers() {
        let ok = alloc::vec![
            alloc::vec![0x61u8, 0x62, 0x63],
            alloc::vec![0x61u8, 0x62, 0x64],
            3u64.to_le_bytes().to_vec(),
        ];
        assert!(validate_case(&SPEC_MEMCMP, &ok).is_ok());

        let too_long = alloc::vec![
            alloc::vec![0x61u8],
            alloc::vec![0x61u8, 0x62],
            2u64.to_le_bytes().to_vec(),
        ];
        assert_eq!(
            validate_case(&SPEC_MEMCMP, &too_long),
            Err(PreconditionViolation::LengthExceedsBuffer {
                len_arg: ArgId(2),
                buffer_arg: ArgId(0),
            })
        );
    }

    #[test]
    fn test_validate_strlen_requires_nul_within_bound() {
        // "abc\0" with bound 4: a NUL lies inside.
        let ok = alloc::vec![
            alloc::vec![0x61u8, 0x62, 0x63, 0x00],
            4u64.to_le_bytes().to_vec()
        ];
        assert!(validate_case(&SPEC_STRLEN, &ok).is_ok());

        // The NUL is outside the bound.
        let bad = alloc::vec![
            alloc::vec![0x61u8, 0x62, 0x63, 0x00],
            3u64.to_le_bytes().to_vec()
        ];
        assert_eq!(
            validate_case(&SPEC_STRLEN, &bad),
            Err(PreconditionViolation::NulNotWithinBound {
                buffer_arg: ArgId(0),
                bound_arg: ArgId(1),
            })
        );
    }

    #[test]
    fn test_validate_strspn_requires_nul_within_bound_and_nul_free_set() {
        let ok = alloc::vec![
            alloc::vec![0x61u8, 0x62, 0x63, 0x00],
            alloc::vec![0x61u8, 0x62],
            4u64.to_le_bytes().to_vec()
        ];
        assert!(validate_case(&SPEC_STRSPN, &ok).is_ok());

        // A NUL in the accept set is out of contract.
        let bad_set = alloc::vec![
            alloc::vec![0x61u8, 0x62, 0x63, 0x00],
            alloc::vec![0x61u8, 0x00],
            4u64.to_le_bytes().to_vec()
        ];
        assert_eq!(
            validate_case(&SPEC_STRSPN, &bad_set),
            Err(PreconditionViolation::NulInNulFreeArg { arg: ArgId(1) })
        );
    }

    #[test]
    fn test_missing_argument_is_reported_not_panicked() {
        let v = validate_case(&SPEC_MEMCHR, &[]);
        assert_eq!(v, Err(PreconditionViolation::MissingArg { arg: ArgId(2) }));
    }

    #[test]
    fn test_every_leaf_corpus_is_in_contract() {
        // The declared preconditions must accept the existing corpus: a target
        // whose own cases fail its own preconditions would be unusable.
        use crate::porting::target::{cases_for, resolve_target};

        for spec in all() {
            let target = resolve_target(spec.symbol).expect("target resolves");
            assert_eq!(target.id, spec.target_id);
            let cases = cases_for(&target);
            assert!(!cases.is_empty(), "{} has no cases", spec.target_id);
            for c in &cases {
                assert!(
                    validate_case(spec, &c.args).is_ok(),
                    "{} case {} violates a declared precondition: {:?}",
                    spec.target_id,
                    c.case_id,
                    validate_case(spec, &c.args)
                );
            }
        }
    }

    #[test]
    fn test_of_target_bridges_every_bootstrap_target() {
        use crate::porting::target::{
            LIBC_MEMCHR, LIBC_MEMCMP, LIBC_STRLEN, LIBC_STRRCHR, LIBC_TOUPPER, POSIX_STRSPN,
        };
        for t in [
            LIBC_TOUPPER,
            LIBC_MEMCMP,
            LIBC_MEMCHR,
            LIBC_STRLEN,
            LIBC_STRRCHR,
            POSIX_STRSPN,
        ] {
            let s = PortSpec::of_target(&t).expect("every bootstrap target has a spec");
            assert_eq!(s.target_id, t.id);
            assert_eq!(s.symbol, t.symbol);
            assert_eq!(s.candidate_source, t.candidate_source);
            assert_eq!(s.abi.symbol, t.abi_symbol);
            assert_eq!(s.locale_contract, t.locale_contract);
        }
    }
}
