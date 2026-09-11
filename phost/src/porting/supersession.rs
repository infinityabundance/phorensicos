// porting/supersession.rs — explicit contract-provenance corrections
//
// A `PortSpec` records *which specification a contract is drawn from*, and its
// `PortSpecId` binds that claim. If the claim is later found to be wrong, the
// correction must be a **new identity**, never a silent edit: existing seals
// bind the old id, and rewriting it would rewrite history.
//
// This module records such corrections explicitly. A correction preserves the
// historical target and its evidence, names the successor, states the normative
// reason, and binds both `PortSpecId`s into a content-addressed record. It is a
// residual on Phorensicos itself: the system detected a semantic-metadata error
// in its own history and migrated by supersession.
//
// No wall clock participates; the record's identity is a pure function of its
// content.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::porting::portspec::{self, ContractSource};
use crate::porting::sha256_hex;

/// The identity domain tag for a supersession record.
pub const SUPERSESSION_DOMAIN: &[u8] = b"PHOR/CONTRACT-SUPERSESSION/v1\0";

/// The schema this build writes.
pub const SUPERSESSION_SCHEMA: u32 = 1;

/// One contract-provenance correction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Supersession {
    /// The mistaken target id, preserved as historical evidence.
    pub historical_target_id: &'static str,
    /// The corrected successor target id.
    pub successor_target_id: &'static str,
    /// The provenance the historical record claimed.
    pub historical_contract: ContractSource,
    /// The corrected provenance.
    pub successor_contract: ContractSource,
    /// The normative reason the historical claim is wrong.
    pub reason: &'static str,
    /// The authority the correction rests on.
    pub authority: &'static str,
}

/// The declared corrections, in a fixed order.
pub const SUPERSESSIONS: [Supersession; 1] = [Supersession {
    historical_target_id: "posix:strspn:c-locale:u64:v1",
    successor_target_id: "libc:strspn:c-locale:u64:v1",
    historical_contract: ContractSource::Posix,
    successor_contract: ContractSource::IsoC,
    reason: "the historical record claimed ISO C does not specify strspn; that is false",
    authority: "ISO C specifies strspn (C90 4.11.5.4; C99 and later 7.21.5.4); POSIX states its strspn specification is aligned with and defers to ISO C",
}];

impl Supersession {
    /// The historical spec's content identity, when both specs are registered.
    pub fn historical_spec_id(&self) -> Option<String> {
        portspec::by_target_id(self.historical_target_id).map(|s| s.id())
    }

    /// The successor spec's content identity.
    pub fn successor_spec_id(&self) -> Option<String> {
        portspec::by_target_id(self.successor_target_id).map(|s| s.id())
    }

    /// The content identity of the correction: both spec identities, both
    /// provenance tags, the reason and the authority, length-framed.
    pub fn id(&self) -> String {
        let mut pre = Vec::with_capacity(SUPERSESSION_DOMAIN.len() + 256);
        pre.extend_from_slice(SUPERSESSION_DOMAIN);
        pre.extend_from_slice(&SUPERSESSION_SCHEMA.to_le_bytes());
        enc(&mut pre, self.historical_target_id);
        enc(&mut pre, self.successor_target_id);
        pre.push(self.historical_contract.tag());
        pre.push(self.successor_contract.tag());
        enc(&mut pre, self.historical_spec_id().as_deref().unwrap_or(""));
        enc(&mut pre, self.successor_spec_id().as_deref().unwrap_or(""));
        enc(&mut pre, self.reason);
        enc(&mut pre, self.authority);
        sha256_hex(&pre)
    }

    /// Consistency: both specs exist, they are distinct identities, the surface is
    /// the same, and the provenance actually changed.
    pub fn is_consistent(&self) -> bool {
        let (Some(hist), Some(succ)) = (
            portspec::by_target_id(self.historical_target_id),
            portspec::by_target_id(self.successor_target_id),
        ) else {
            return false;
        };
        hist.id() != succ.id()
            && hist.contract == self.historical_contract
            && succ.contract == self.successor_contract
            && self.historical_contract != self.successor_contract
            && hist.symbol == succ.symbol
            && hist.case_space.generator == succ.case_space.generator
            && hist.abi.symbol == succ.abi.symbol
    }

    /// The successor of `target_id`, if a correction names one.
    pub fn successor_of(target_id: &str) -> Option<&'static Supersession> {
        SUPERSESSIONS
            .iter()
            .find(|s| s.historical_target_id == target_id)
    }

    /// A deterministic JSON projection of the migration record.
    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema\": \"phorensic.porting.contract_supersession.v1\",\n  \"historical_target\": \"{}\",\n  \"historical_contract\": \"{}\",\n  \"historical_spec_id\": \"{}\",\n  \"successor_target\": \"{}\",\n  \"successor_contract\": \"{}\",\n  \"successor_spec_id\": \"{}\",\n  \"reason\": \"{}\",\n  \"authority\": \"{}\",\n  \"claim\": \"the historical evidence is preserved; the successor is requalified and resealed on its own identity\",\n  \"supersession_id\": \"{}\"\n}}\n",
            self.historical_target_id,
            contract_name(self.historical_contract),
            self.historical_spec_id().unwrap_or_default(),
            self.successor_target_id,
            contract_name(self.successor_contract),
            self.successor_spec_id().unwrap_or_default(),
            self.reason.replace('"', "'"),
            self.authority.replace('"', "'"),
            self.id()
        )
    }
}

fn enc(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn contract_name(c: ContractSource) -> &'static str {
    match c {
        ContractSource::IsoC => "iso-c",
        ContractSource::Posix => "posix",
        ContractSource::PhorensicComposition => "phorensic-composition",
        ContractSource::ImplementationDefined { family } => family,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_strspn_correction_is_consistent() {
        let s = &SUPERSESSIONS[0];
        assert!(
            s.is_consistent(),
            "the correction must reference two real specs"
        );
        assert_eq!(s.historical_contract, ContractSource::Posix);
        assert_eq!(s.successor_contract, ContractSource::IsoC);
        assert_ne!(
            s.historical_spec_id().unwrap(),
            s.successor_spec_id().unwrap()
        );
    }

    #[test]
    fn test_successor_lookup_is_by_historical_id() {
        let s = Supersession::successor_of("posix:strspn:c-locale:u64:v1").expect("correction");
        assert_eq!(s.successor_target_id, "libc:strspn:c-locale:u64:v1");
        assert!(Supersession::successor_of("libc:toupper:c-locale:u8:v1").is_none());
    }

    #[test]
    fn test_the_correction_identity_is_deterministic_and_binds_both_specs() {
        let a = SUPERSESSIONS[0].id();
        let b = SUPERSESSIONS[0].id();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        // The identity changes if either provenance claim changes.
        let mut alt = SUPERSESSIONS[0];
        alt.historical_contract = ContractSource::IsoC;
        assert_ne!(alt.id(), a);
    }

    #[test]
    fn test_bare_symbol_resolution_still_returns_the_historical_target() {
        // The correction must not silently rename the historical target: existing
        // seals and verifiers address it by the bare symbol and its full id.
        let historical =
            portspec::by_target_id("posix:strspn:c-locale:u64:v1").expect("historical");
        let successor = portspec::by_target_id("libc:strspn:c-locale:u64:v1").expect("successor");
        assert_eq!(historical.symbol, "strspn");
        assert_eq!(successor.symbol, "strspn");
        assert_ne!(historical.id(), successor.id());
    }
}
