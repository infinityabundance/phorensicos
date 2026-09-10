// capability_driver.rs — Capability-gated driver framework
// Provides the Driver trait + registration for capability-gated hardware access.

use crate::kernel::CapabilitySet;

/// Driver state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverState {
    Unloaded,
    Loaded,
    Initialized,
    Active,
    Error,
    Removed,
}

/// Driver descriptor with capability requirements
#[derive(Debug, Clone)]
pub struct DriverDescriptor {
    pub name: [u8; 64],
    pub name_len: usize,
    pub state: DriverState,
    pub required_capabilities: CapabilitySet,
    pub hardware_id: [u8; 32],
    pub hardware_id_len: usize,
    pub version: u32,
    pub irq_line: Option<u8>,
    pub trust_level: u8,         // 0=unknown, 1=observed, 5=sealed
    pub drift_count: u64,        // number of trust drift events detected
    pub last_verified_tick: u64, // tick when driver was last court-verified
}

impl DriverDescriptor {
    pub fn new(name: &str, caps: CapabilitySet, hw_id: &str, version: u32) -> Self {
        let mut n = [0u8; 64];
        let nl = name.len().min(63);
        n[..nl].copy_from_slice(&name.as_bytes()[..nl]);

        let mut h = [0u8; 32];
        let hl = hw_id.len().min(31);
        h[..hl].copy_from_slice(&hw_id.as_bytes()[..hl]);

        Self {
            name: n,
            name_len: nl,
            state: DriverState::Unloaded,
            required_capabilities: caps,
            hardware_id: h,
            hardware_id_len: hl,
            version,
            irq_line: None,
            trust_level: 0,
            drift_count: 0,
            last_verified_tick: 0,
        }
    }

    pub fn new_with_irq(
        name: &str,
        caps: CapabilitySet,
        hw_id: &str,
        version: u32,
        irq: u8,
    ) -> Self {
        let mut desc = Self::new(name, caps, hw_id, version);
        desc.irq_line = Some(irq);
        desc
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }

    pub fn hw_id_str(&self) -> &str {
        core::str::from_utf8(&self.hardware_id[..self.hardware_id_len]).unwrap_or("?")
    }

    pub fn set_trust(&mut self, level: u8) {
        self.trust_level = level;
    }

    pub fn trust_level(&self) -> u8 {
        self.trust_level
    }

    pub fn is_sealed(&self) -> bool {
        self.trust_level >= 5
    }

    pub fn irq_line(&self) -> Option<u8> {
        self.irq_line
    }

    pub fn record_drift(&mut self) {
        self.drift_count += 1;
        self.state = DriverState::Error;
    }

    pub fn verify_trust(&mut self, tick: u64) {
        self.last_verified_tick = tick;
    }

    pub fn drift_count(&self) -> u64 {
        self.drift_count
    }

    pub fn is_active(&self) -> bool {
        self.state == DriverState::Active
    }
}

/// Capability-gated driver registry
pub struct DriverRegistry {
    drivers: [Option<DriverDescriptor>; 32],
    count: usize,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Default::default(),
            count: 0,
        }
    }

    pub const fn new_const() -> Self {
        Self {
            drivers: [const { None }; 32],
            count: 0,
        }
    }

    /// Register a driver. Requires the DRIVER_LOAD capability.
    pub fn register(
        &mut self,
        desc: DriverDescriptor,
        caps: CapabilitySet,
    ) -> Result<usize, &'static str> {
        if !caps.has(CapabilitySet::DRIVER_LOAD) {
            return Err("missing DRIVER_LOAD capability");
        }
        if self.count >= 32 {
            return Err("driver registry full");
        }
        let idx = self.count;
        self.drivers[idx] = Some(desc);
        self.count += 1;
        Ok(idx)
    }

    /// Initialize a driver. Requires the driver's required capabilities.
    pub fn init(&mut self, idx: usize, caps: CapabilitySet) -> Result<(), &'static str> {
        if idx >= 32 {
            return Err("invalid driver index");
        }
        if let Some(ref mut desc) = self.drivers[idx] {
            if !caps.has_all(desc.required_capabilities) {
                return Err("insufficient capabilities for driver");
            }
            desc.state = DriverState::Initialized;
            Ok(())
        } else {
            Err("no driver at index")
        }
    }

    /// Activate a driver
    pub fn activate(&mut self, idx: usize) -> Result<(), &'static str> {
        if let Some(ref mut desc) = self.drivers[idx] {
            desc.state = DriverState::Active;
            Ok(())
        } else {
            Err("no driver at index")
        }
    }

    /// Activate a driver with sealed trust checking
    pub fn activate_sealed(&mut self, idx: usize, caps: CapabilitySet) -> Result<(), &'static str> {
        if idx >= 32 {
            return Err("invalid driver index");
        }
        if let Some(ref mut desc) = self.drivers[idx] {
            if !caps.has_all(desc.required_capabilities) {
                return Err("insufficient capabilities");
            }
            if !desc.is_sealed() {
                return Err("driver not sealed \u{2014} cannot activate in secure mode");
            }
            if desc.state != DriverState::Initialized {
                return Err("driver must be initialized before activation");
            }
            desc.state = DriverState::Active;
            Ok(())
        } else {
            Err("no driver at index")
        }
    }

    /// Revoke a driver due to trust drift. Moves it to Error state.
    pub fn revoke(&mut self, idx: usize) -> Result<(), &'static str> {
        if idx >= 32 {
            return Err("invalid driver index");
        }
        if let Some(ref mut desc) = self.drivers[idx] {
            desc.record_drift();
            Ok(())
        } else {
            Err("no driver at index")
        }
    }

    /// Verify trust for all active drivers. If any fail, revoke them.
    pub fn verify_all_active(&mut self, tick: u64) -> u64 {
        let mut revoked = 0u64;
        for i in 0..self.count {
            if let Some(ref mut desc) = self.drivers[i] {
                if desc.is_active() {
                    // Check if trust is still valid (drift check)
                    // In production, this would re-check hashes, court verdicts, etc.
                    if desc.trust_level < 5 {
                        desc.record_drift();
                        revoked += 1;
                    } else {
                        desc.verify_trust(tick);
                    }
                }
            }
        }
        revoked
    }

    /// Activate only if court-verified. Returns Ok if activated, Err with reason otherwise.
    pub fn activate_court_verified(
        &mut self,
        idx: usize,
        caps: CapabilitySet,
    ) -> Result<(), &'static str> {
        if idx >= 32 {
            return Err("invalid index");
        }
        if let Some(ref mut desc) = self.drivers[idx] {
            if !caps.has_all(desc.required_capabilities) {
                return Err("insufficient capabilities");
            }
            if desc.trust_level < 5 {
                return Err("driver not sealed \u{2014} court verification required");
            }
            if desc.state != DriverState::Initialized {
                return Err("driver must be initialized first");
            }
            desc.state = DriverState::Active;
            Ok(())
        } else {
            Err("no driver at index")
        }
    }

    /// List all registered drivers
    pub fn list(&self) -> &[Option<DriverDescriptor>; 32] {
        &self.drivers
    }

    pub fn driver_count(&self) -> usize {
        self.count
    }

    pub fn get(&self, idx: usize) -> Option<&DriverDescriptor> {
        if idx < 32 {
            self.drivers[idx].as_ref()
        } else {
            None
        }
    }

    /// Load a driver from a sealed package. Verifies the seal hash before
    /// registering. Requires the DRIVER_LOAD capability.
    pub fn load_from_sealed(
        &mut self,
        package_name: &str,
        package_data: &[u8],
        expected_hash: &[u8],
        caps: CapabilitySet,
    ) -> Result<usize, &'static str> {
        if !caps.has(CapabilitySet::DRIVER_LOAD) {
            return Err("missing DRIVER_LOAD capability for sealed load");
        }
        if self.count >= 32 {
            return Err("driver registry full");
        }

        // Verify package integrity by checking the expected hash
        // Uses 32-byte wide-state hash as SHA-256 substitute
        let actual_hash = crate::drivers::serial::hash_bytes_32(package_data);
        if actual_hash.len() != expected_hash.len()
            || !actual_hash
                .iter()
                .zip(expected_hash.iter())
                .all(|(a, b)| a == b)
        {
            return Err("sealed package hash mismatch");
        }

        let desc = DriverDescriptor::new(package_name, caps, "SEALED", 1);
        let idx = self.count;
        self.drivers[idx] = Some(desc);
        self.count += 1;
        Ok(idx)
    }
}

// Register the standard drivers
pub fn register_standard_drivers(registry: &mut DriverRegistry, caps: CapabilitySet) {
    // Serial driver with IRQ
    let serial_caps = CapabilitySet {
        bits: CapabilitySet::CONSOLE | CapabilitySet::IO_PORT,
    };
    let mut serial_desc =
        DriverDescriptor::new_with_irq("UART 16550 Serial", serial_caps, "PNP0501", 1, 4);
    serial_desc.set_trust(5); // Sealed
    let _ = registry.register(serial_desc, caps);

    // PS/2 Keyboard with IRQ
    let kbd_caps = CapabilitySet {
        bits: CapabilitySet::IO_PORT | CapabilitySet::IRQ_LINE,
    };
    let mut kbd_desc = DriverDescriptor::new_with_irq("PS/2 Keyboard", kbd_caps, "PNP0303", 1, 1);
    kbd_desc.set_trust(5); // Sealed
    let _ = registry.register(kbd_desc, caps);

    // Framebuffer
    let fb_caps = CapabilitySet {
        bits: CapabilitySet::MEMORY_MANAGEMENT | CapabilitySet::FRAMEBUFFER,
    };
    let mut fb_desc = DriverDescriptor::new("UEFI GOP Framebuffer", fb_caps, "ACPI\\QEMUGPU", 1);
    fb_desc.set_trust(5); // Sealed
    let _ = registry.register(fb_desc, caps);
}

/// Global driver registry
pub static mut DRIVER_REGISTRY: DriverRegistry = DriverRegistry::new_const();
