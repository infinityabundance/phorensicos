// porting/ident.rs — identity namespaces that are never collapsed
//
// Phase 7, invariant §2.4. A Phorensicos SHA-256 object hash, an FRF
// receipt/claim/run id, an FRF-Fuzz `ContentId` (BLAKE3) and a Gemel `Gid` are
// four distinct identity systems. A cross-system record carries a **reference**;
// it never reinterprets one id as another.
//
// This module makes a mixed-up reference a type error rather than a silent
// string. Each wrapper is opaque — a newtype over the retained identifier — and
// its namespace tag participates in `canonical()`, so two identical payloads
// from different systems can never produce the same canonical token. The
// authority never parses or reinterprets a foreign id; it only retains it
// verbatim and requires that it be present and well-formed when an obligation
// demands it.
//
// The second half of the module is the **evidence closure** (§24): the explicit
// content-addressed set of evidence the final seal binds. Missing members fail
// verification; the ordering is canonical, so a closure's identity does not
// depend on insertion order.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::sha256_hex;

/// The identity domain tag for the evidence-closure encoding. Bumping this
/// changes every `EvidenceClosureId` and is a deliberate, versioned act.
pub const EVIDENCE_CLOSURE_DOMAIN: &[u8] = b"PHOR/EVIDENCE-CLOSURE/v1\0";

/// The closure schema this build writes and accepts.
pub const EVIDENCE_CLOSURE_SCHEMA: u32 = 1;

/// The longest retained identifier. Bounded so a malformed or hostile id cannot
/// allocate without limit; every real id in the four systems is far shorter.
pub const MAX_IDENT_LEN: usize = 1024;

/// Why an identifier was refused. Every variant fails closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentError {
    /// The identifier was empty.
    Empty,
    /// The identifier exceeded [`MAX_IDENT_LEN`].
    TooLong,
    /// The identifier contained an ASCII control character, which would make a
    /// canonical token ambiguous.
    ControlCharacter,
}

impl IdentError {
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentError::Empty => "identifier is empty",
            IdentError::TooLong => "identifier is too long",
            IdentError::ControlCharacter => "identifier contains a control character",
        }
    }
}

/// Validate a retained identifier: non-empty, bounded, no control characters.
pub fn validate_ident(s: &str) -> Result<(), IdentError> {
    if s.is_empty() {
        return Err(IdentError::Empty);
    }
    if s.len() > MAX_IDENT_LEN {
        return Err(IdentError::TooLong);
    }
    if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(IdentError::ControlCharacter);
    }
    Ok(())
}

/// Define one opaque identity namespace.
///
/// Two namespaces with the same payload are different types *and* different
/// canonical tokens, so they can neither be swapped in code nor collide in a
/// hash.
macro_rules! ident {
    ($name:ident, $ns:literal, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The namespace tag, which participates in [`Self::canonical`].
            pub const NAMESPACE: &'static str = $ns;

            /// Construct without validation. Prefer [`Self::parse`] for anything
            /// that crosses a trust boundary.
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Validate and construct.
            pub fn parse(s: &str) -> Result<Self, IdentError> {
                validate_ident(s)?;
                Ok(Self(s.to_string()))
            }

            /// Retain the identifier verbatim.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Is the identifier absent?
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            /// The namespace-tagged canonical token bound into a closure or a
            /// receipt. Distinct namespaces never collide.
            pub fn canonical(&self) -> String {
                format!("{}:{}", $ns, self.0)
            }

            /// `Some(self)` when the identifier is present, else `None`.
            pub fn present(self) -> Option<Self> {
                if self.0.is_empty() {
                    None
                } else {
                    Some(self)
                }
            }
        }
    };
}

// Phorensicos namespace.
ident!(
    PortSpecId,
    "phor.portspec",
    "A `PortSpec` content identity (§1)."
);
ident!(
    CampaignManifestId,
    "phor.campaign",
    "A frozen campaign manifest identity (§23)."
);
ident!(
    CandidateSourceHash,
    "phor.source-sha256",
    "SHA-256 of the exact candidate source bytes."
);
ident!(
    CandidateObjectHash,
    "phor.object-sha256",
    "SHA-256 of the compiled `.phor` object."
);
ident!(
    CompilerReceiptHash,
    "phor.compiler-receipt-sha256",
    "SHA-256 of the compiler receipt that binds source to object."
);
ident!(
    QualificationReceiptId,
    "phor.qualification",
    "A held-out qualification receipt identity."
);
ident!(
    ChallengeReceiptId,
    "phor.challenge",
    "A court-sensitivity (challenge) receipt identity."
);
ident!(
    OracleWitnessId,
    "phor.oracle-witness",
    "One oracle implementation witness identity."
);
ident!(
    EvidenceClosureId,
    "phor.evidence-closure",
    "The content identity of an evidence closure."
);
ident!(
    PromotionReceiptId,
    "phor.promotion",
    "A Phorensicos promotion receipt identity."
);
ident!(
    StoreGenerationId,
    "phor.store-generation",
    "An immutable store-generation identity."
);
ident!(
    CompositionIrId,
    "phor.composition-ir",
    "A `CompositionIR` graph identity."
);
ident!(
    DependencyBindingHash,
    "phor.dependency-binding",
    "The ordered identity of a composition's sealed dependencies."
);
ident!(
    ArtifactHash,
    "phor.artifact",
    "A composition or package artifact identity."
);

// Foreign namespace — retained verbatim, never merged with the above.
ident!(
    FrfRunId,
    "frf.run",
    "An FRF court run id (`run-{court}-{sha}`), retained verbatim."
);
ident!(
    FrfReceiptId,
    "frf.receipt",
    "An FRF OpenReceipt id, retained verbatim."
);
ident!(
    FrfClaimId,
    "frf.claim",
    "An FRF compiled-claim id, retained verbatim."
);
ident!(
    FrfFuzzContentId,
    "frf-fuzz.content",
    "An FRF-Fuzz `ContentId` (BLAKE3), retained verbatim."
);
ident!(
    GemelGid,
    "gemel.gid",
    "A Gemel object `Gid`, retained verbatim."
);

// ---------------------------------------------------------------------------
// Evidence closure
// ---------------------------------------------------------------------------

/// The role an evidence member plays in a closure (§24).
///
/// Roles are a closed set: an unknown role cannot be encoded, so a closure can
/// never smuggle in an unclassified member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceRole {
    PortSpec,
    CampaignManifest,
    OracleWitness,
    DesignResult,
    DiscoveryCounterexample,
    ReductionRecord,
    CandidateSource,
    CandidateBuild,
    CandidateExecution,
    QualificationResult,
    ChallengeResult,
    FrfReceipt,
    FrfClaim,
    DispatchResult,
    PromotionReceipt,
    CompositionIr,
    DependencyBinding,
    StoreGeneration,
    PreviousStoreGeneration,
    FrfFuzzContent,
    GemelGid,
}

impl EvidenceRole {
    pub fn tag(self) -> u8 {
        match self {
            EvidenceRole::PortSpec => 1,
            EvidenceRole::CampaignManifest => 2,
            EvidenceRole::OracleWitness => 3,
            EvidenceRole::DesignResult => 4,
            EvidenceRole::DiscoveryCounterexample => 5,
            EvidenceRole::ReductionRecord => 6,
            EvidenceRole::CandidateSource => 7,
            EvidenceRole::CandidateBuild => 8,
            EvidenceRole::CandidateExecution => 9,
            EvidenceRole::QualificationResult => 10,
            EvidenceRole::ChallengeResult => 11,
            EvidenceRole::FrfReceipt => 12,
            EvidenceRole::FrfClaim => 13,
            EvidenceRole::DispatchResult => 14,
            EvidenceRole::PromotionReceipt => 15,
            EvidenceRole::CompositionIr => 16,
            EvidenceRole::DependencyBinding => 17,
            EvidenceRole::StoreGeneration => 18,
            EvidenceRole::PreviousStoreGeneration => 19,
            EvidenceRole::FrfFuzzContent => 20,
            EvidenceRole::GemelGid => 21,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceRole::PortSpec => "port-spec",
            EvidenceRole::CampaignManifest => "campaign-manifest",
            EvidenceRole::OracleWitness => "oracle-witness",
            EvidenceRole::DesignResult => "design-result",
            EvidenceRole::DiscoveryCounterexample => "discovery-counterexample",
            EvidenceRole::ReductionRecord => "reduction-record",
            EvidenceRole::CandidateSource => "candidate-source",
            EvidenceRole::CandidateBuild => "candidate-build",
            EvidenceRole::CandidateExecution => "candidate-execution",
            EvidenceRole::QualificationResult => "qualification-result",
            EvidenceRole::ChallengeResult => "challenge-result",
            EvidenceRole::FrfReceipt => "frf-receipt",
            EvidenceRole::FrfClaim => "frf-claim",
            EvidenceRole::DispatchResult => "dispatch-result",
            EvidenceRole::PromotionReceipt => "promotion-receipt",
            EvidenceRole::CompositionIr => "composition-ir",
            EvidenceRole::DependencyBinding => "dependency-binding",
            EvidenceRole::StoreGeneration => "store-generation",
            EvidenceRole::PreviousStoreGeneration => "previous-store-generation",
            EvidenceRole::FrfFuzzContent => "frf-fuzz-content",
            EvidenceRole::GemelGid => "gemel-gid",
        }
    }
}

/// One member of an evidence closure: a role and a namespaced identity token.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidenceEdge {
    pub role: EvidenceRole,
    /// The canonical token of some identity type (`X::canonical()`). Kept as a
    /// string so the closure can bind heterogeneous namespaces without
    /// collapsing them.
    pub identity: String,
}

impl EvidenceEdge {
    pub fn new(role: EvidenceRole, identity: impl Into<String>) -> Self {
        EvidenceEdge {
            role,
            identity: identity.into(),
        }
    }
}

/// Why an evidence closure was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureError {
    UnknownSchema(u32),
    Empty,
    MalformedIdentity,
}

impl ClosureError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClosureError::UnknownSchema(_) => "unknown evidence-closure schema version",
            ClosureError::Empty => "evidence closure is empty",
            ClosureError::MalformedIdentity => "evidence closure member is malformed",
        }
    }
}

/// The explicit, content-addressed set of evidence a seal binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceClosure {
    pub schema_version: u32,
    pub members: Vec<EvidenceEdge>,
}

impl EvidenceClosure {
    pub fn new(members: Vec<EvidenceEdge>) -> Self {
        EvidenceClosure {
            schema_version: EVIDENCE_CLOSURE_SCHEMA,
            members,
        }
    }

    /// The members in canonical order: sorted by `(role.tag, identity)`.
    ///
    /// Sorting makes the identity order-independent, so a closure assembled in a
    /// different order is the *same* closure. Duplicate `(role, identity)` pairs
    /// are preserved (the same oracle witness may legitimately be referenced
    /// twice); they do not change the identity's meaning.
    fn canonical_members(&self) -> Vec<&EvidenceEdge> {
        let mut v: Vec<&EvidenceEdge> = self.members.iter().collect();
        v.sort_by(|a, b| {
            a.role
                .tag()
                .cmp(&b.role.tag())
                .then_with(|| a.identity.cmp(&b.identity))
        });
        v
    }

    /// The canonical byte encoding (excluding the domain tag). Length-prefixed
    /// and unambiguous.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.schema_version.to_le_bytes());
        let members = self.canonical_members();
        out.extend_from_slice(&(members.len() as u32).to_le_bytes());
        for m in members {
            out.push(m.role.tag());
            let bytes = m.identity.as_bytes();
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    /// Validate and derive the content identity.
    ///
    /// Fails closed on an unknown schema version, an empty closure, or a
    /// malformed member identity.
    pub fn id(&self) -> Result<EvidenceClosureId, ClosureError> {
        self.validate()?;
        let mut pre = Vec::with_capacity(EVIDENCE_CLOSURE_DOMAIN.len() + 64);
        pre.extend_from_slice(EVIDENCE_CLOSURE_DOMAIN);
        pre.extend_from_slice(&self.canonical_bytes());
        Ok(EvidenceClosureId::new(sha256_hex(&pre)))
    }

    pub fn validate(&self) -> Result<(), ClosureError> {
        if self.schema_version != EVIDENCE_CLOSURE_SCHEMA {
            return Err(ClosureError::UnknownSchema(self.schema_version));
        }
        if self.members.is_empty() {
            return Err(ClosureError::Empty);
        }
        for m in &self.members {
            validate_ident(&m.identity).map_err(|_| ClosureError::MalformedIdentity)?;
        }
        Ok(())
    }

    /// Does the closure reference this identity token under any role?
    pub fn contains(&self, identity: &str) -> bool {
        self.members.iter().any(|m| m.identity == identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_namespaces_never_collide() {
        // The same payload in two systems is two different canonical tokens.
        let spec = PortSpecId::parse("deadbeef").unwrap();
        let receipt = FrfReceiptId::parse("deadbeef").unwrap();
        assert_ne!(spec.canonical(), receipt.canonical());
        assert_eq!(spec.canonical(), "phor.portspec:deadbeef");
        assert_eq!(receipt.canonical(), "frf.receipt:deadbeef");
        // And they cannot be assigned to each other at compile time (distinct
        // types here is the point; this is the runtime half of the check).
        assert_eq!(spec.as_str(), receipt.as_str());
    }

    #[test]
    fn test_parse_rejects_empty_control_and_oversize() {
        assert_eq!(PortSpecId::parse(""), Err(IdentError::Empty));
        assert_eq!(
            FrfReceiptId::parse("bad\nid"),
            Err(IdentError::ControlCharacter)
        );
        let big = "x".repeat(MAX_IDENT_LEN + 1);
        assert_eq!(GemelGid::parse(&big), Err(IdentError::TooLong));
    }

    #[test]
    fn test_present_filters_the_empty_identifier() {
        assert!(PortSpecId::new("").present().is_none());
        assert_eq!(PortSpecId::new("abc").present().unwrap().as_str(), "abc");
    }

    fn edges() -> Vec<EvidenceEdge> {
        vec![
            EvidenceEdge::new(EvidenceRole::PortSpec, "phor.portspec:a"),
            EvidenceEdge::new(EvidenceRole::CandidateSource, "phor.source-sha256:b"),
            EvidenceEdge::new(EvidenceRole::FrfReceipt, "frf.receipt:c"),
        ]
    }

    fn closure() -> EvidenceClosure {
        EvidenceClosure::new(edges())
    }

    #[test]
    fn test_closure_identity_is_order_independent() {
        let a = closure();
        let mut m = edges();
        m.reverse();
        let b = EvidenceClosure::new(m);
        assert_eq!(a.id().unwrap(), b.id().unwrap());
    }

    #[test]
    fn test_closure_identity_changes_when_a_member_changes() {
        let a = closure();
        let mut m = edges();
        m[2].identity = String::from("frf.receipt:d");
        let b = EvidenceClosure::new(m);
        assert_ne!(a.id().unwrap(), b.id().unwrap());
    }

    #[test]
    fn test_closure_fails_closed_on_empty_unknown_schema_and_bad_member() {
        assert_eq!(EvidenceClosure::new(vec![]).id(), Err(ClosureError::Empty));
        let mut c = closure();
        c.schema_version = 999;
        assert_eq!(c.id(), Err(ClosureError::UnknownSchema(999)));
        let mut c = closure();
        c.members[0].identity = String::new();
        assert_eq!(c.id(), Err(ClosureError::MalformedIdentity));
    }

    #[test]
    fn test_closure_contains_an_exact_namespaced_token() {
        let c = closure();
        assert!(c.contains("phor.portspec:a"));
        assert!(!c.contains("a"));
        assert!(!c.contains("frf.receipt:a"));
    }
}
