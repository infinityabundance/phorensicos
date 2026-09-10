// Phost — Phorensic OS Kernel / Trusted Nucleus
// Core kernel types, object model, capability system, scheduler

// ============================================================
// Trust Level Ladder — PHORENSIC_OS.md §9
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TrustLevel {
    Unknown = 0,
    Observed = 1,
    Replayed = 2,
    OracleCompared = 3,
    ResidualStable = 4,
    Sealed = 5,
    Promoted = 6,
}

// ============================================================
// Kernel Object — PHORENSIC_OS.md §2
// ============================================================

pub type ObjectId = u64;
pub type Generation = u64;
pub type HandleId = u64;

/// Every object in the kernel carries forensic metadata
#[derive(Debug, Clone)]
pub struct KernelObject<T: Sized> {
    pub id: ObjectId,
    pub generation: Generation,
    pub data: T,
    pub identity: Identity,
    pub capabilities: CapabilitySet,
    pub provenance: ProvenanceRecord,
    pub trust_level: TrustLevel,
    pub residual_fingerprints: [u64; 4], // 256-bit hash of residual state
}

impl<T: Sized> KernelObject<T> {
    pub fn new(id: ObjectId, data: T, identity: Identity) -> Self {
        Self {
            id,
            generation: 1,
            data,
            identity,
            capabilities: CapabilitySet::empty(),
            provenance: ProvenanceRecord::new(),
            trust_level: TrustLevel::Unknown,
            residual_fingerprints: [0; 4],
        }
    }

    pub fn handle(&self) -> Handle {
        Handle {
            id: self.id,
            generation: self.generation,
        }
    }

    pub fn validate_handle(&self, handle: &Handle) -> Result<(), HandleError> {
        if handle.id != self.id {
            return Err(HandleError::WrongObject(handle.id, self.id));
        }
        if handle.generation != self.generation {
            return Err(HandleError::StaleGeneration(
                handle.generation,
                self.generation,
            ));
        }
        Ok(())
    }
}

// ============================================================
// Handle with Generation Tag — PHORENSIC_LANGUAGE.md §3.5
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    pub id: HandleId,
    pub generation: Generation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleError {
    WrongObject(HandleId, HandleId),
    StaleGeneration(Generation, Generation),
    InvalidHandle,
}

// ============================================================
// Identity & Provenance
// ============================================================

#[derive(Debug, Clone)]
pub struct Identity {
    pub creator: HandleId,
    pub created_at_tick: u64,
    pub creation_context: [u8; 32],
}

impl Identity {
    pub fn system() -> Self {
        Self {
            creator: 0,
            created_at_tick: 0,
            creation_context: [0; 32],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProvenanceRecord {
    pub source_hash: [u8; 32],
    pub dialect_profile: [u8; 16],
    pub creation_receipt: Option<Receipt>,
}

impl ProvenanceRecord {
    pub fn new() -> Self {
        Self {
            source_hash: [0; 32],
            dialect_profile: [0; 16],
            creation_receipt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Receipt {
    pub stage: [u8; 16],
    pub version: u64,
    pub input_hash: [u8; 32],
    pub output_hash: [u8; 32],
}

// ============================================================
// Capabilities — PHORENSIC_LANGUAGE.md §4
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet {
    pub bits: u64,
}

impl CapabilitySet {
    pub const CONSOLE: u64 = 1 << 0;
    pub const MSR_ACCESS: u64 = 1 << 1;
    pub const MEMORY_MANAGEMENT: u64 = 1 << 2;
    pub const INTERRUPT_CONTROL: u64 = 1 << 3;
    pub const BLOCK_DEVICE: u64 = 1 << 4;
    pub const FRAMEBUFFER: u64 = 1 << 5;
    pub const FILE_SYSTEM: u64 = 1 << 6;
    pub const PROCESS_SPAWN: u64 = 1 << 7;
    pub const IPC_ENDPOINT: u64 = 1 << 8;
    pub const DRIVER_LOAD: u64 = 1 << 9;
    pub const SEAL_SERVICE: u64 = 1 << 10;
    pub const COURT_SERVICE: u64 = 1 << 11;
    pub const ORACLE_SERVICE: u64 = 1 << 12;
    pub const ROLLBACK: u64 = 1 << 13;
    pub const IO_PORT: u64 = 1 << 14;
    pub const IRQ_LINE: u64 = 1 << 15;

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn all() -> Self {
        Self { bits: u64::MAX }
    }

    pub fn has(&self, cap: u64) -> bool {
        self.bits & cap == cap
    }

    /// Check if this set contains all bits from another set.
    pub fn has_all(&self, other: CapabilitySet) -> bool {
        self.bits & other.bits == other.bits
    }

    pub fn grant(&mut self, cap: u64) {
        self.bits |= cap;
    }

    pub fn revoke(&mut self, cap: u64) {
        self.bits &= !cap;
    }

    pub fn restrict(&self, mask: u64) -> Self {
        Self {
            bits: self.bits & mask,
        }
    }
}

// ============================================================
// Effect Declarations — PHORENSIC_LANGUAGE.md §5
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSet {
    pub bits: u16,
}

impl EffectSet {
    pub const COMPUTE: u16 = 1 << 0;
    pub const IO_READ: u16 = 1 << 1;
    pub const IO_WRITE: u16 = 1 << 2;
    pub const BLOCKING: u16 = 1 << 3;
    pub const IRQ_HANDLE: u16 = 1 << 4;
    pub const RESIDUAL: u16 = 1 << 5;
    pub const MACHINE_IOPORT: u16 = 1 << 6;
    pub const MEMORY_MMIO: u16 = 1 << 7;
    pub const CAGE_TRANSLATE: u16 = 1 << 8;
    pub const COURT_REQUEST: u16 = 1 << 9;
    pub const DMA: u16 = 1 << 10;

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.bits & !other.bits == 0
    }

    pub fn contains(&self, effect: u16) -> bool {
        self.bits & effect == effect
    }
}

// ============================================================
// Residual Record — PHORENSIC_LANGUAGE.md §9
// ============================================================

#[derive(Debug, Clone)]
pub struct ResidualRecord {
    pub operation: [u8; 32], // hash of operation name
    pub object_id: ObjectId,
    pub generation: Generation,
    pub state_before: [u8; 32],
    pub state_after: [u8; 32],
    pub court_id: Option<[u8; 16]>,
    pub timestamp: u64,
}

// ============================================================
// Scheduler — PHORENSIC_OS.md §5
// ============================================================

#[derive(Debug, Clone)]
pub struct Scheduler {
    pub policy: SchedulePolicy,
    pub quantum: u64,
    pub tick_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulePolicy {
    RoundRobin,
    Priority,
    CapabilityBalanced,
}

impl Scheduler {
    pub const fn new(policy: SchedulePolicy) -> Self {
        Self {
            policy,
            quantum: 100,
            tick_count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count += 1;
    }
}

// ============================================================
// IPC Channel — PHORENSIC_OS.md §4
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcDirection {
    Send,
    Receive,
    Duplex,
}

#[derive(Debug, Clone)]
pub struct IpcChannel {
    pub id: ObjectId,
    pub direction: IpcDirection,
    pub peer: Handle,
    pub generation: Generation,
}

// ============================================================
// Trust Court — REPLAY_COURTS.md
// ============================================================

#[derive(Debug, Clone)]
pub struct CourtVerdict {
    pub court_id: [u8; 16],
    pub object_id: ObjectId,
    pub verdict: VerdictKind,
    pub tests_passed: u64,
    pub tests_failed: u64,
    pub evidence_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictKind {
    Consistent,
    Inconsistent,
    Identical,
    Divergent,
    Inconclusive,
    Promote,
    Deny,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_set() {
        let mut caps = CapabilitySet::empty();
        assert!(!caps.has(CapabilitySet::CONSOLE));
        caps.grant(CapabilitySet::CONSOLE);
        assert!(caps.has(CapabilitySet::CONSOLE));
        caps.revoke(CapabilitySet::CONSOLE);
        assert!(!caps.has(CapabilitySet::CONSOLE));
    }

    #[test]
    fn test_effect_subset() {
        let outer = EffectSet {
            bits: EffectSet::COMPUTE | EffectSet::IO_READ,
        };
        let inner = EffectSet {
            bits: EffectSet::COMPUTE,
        };
        assert!(inner.is_subset_of(&outer));
        let bad = EffectSet {
            bits: EffectSet::IO_WRITE,
        };
        assert!(!bad.is_subset_of(&outer));
    }

    #[test]
    fn test_handle_generation() {
        let identity = Identity::system();
        let obj: KernelObject<u64> = KernelObject::new(1, 42, identity);
        let handle = obj.handle();
        assert!(obj.validate_handle(&handle).is_ok());
        let stale = Handle {
            id: 1,
            generation: 0,
        };
        assert!(obj.validate_handle(&stale).is_err());
    }

    #[test]
    fn test_trust_ladder() {
        assert!(TrustLevel::Unknown < TrustLevel::Observed);
        assert!(TrustLevel::Observed < TrustLevel::Replayed);
        assert!(TrustLevel::Replayed < TrustLevel::OracleCompared);
        assert!(TrustLevel::OracleCompared < TrustLevel::ResidualStable);
        assert!(TrustLevel::ResidualStable < TrustLevel::Sealed);
        assert!(TrustLevel::Sealed < TrustLevel::Promoted);
    }
}
