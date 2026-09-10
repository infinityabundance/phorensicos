// serial.phost — UART serial driver for Phorensic OS (Rust implementation)
// Uses the 16550 UART with port-based I/O and the capability model

use crate::kernel::CapabilitySet;
use crate::nucleus::AssemblyShim;
use core::fmt;

/// Log levels for serial output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

// ============================================================================
// 16550 UART register offsets (relative to port_base)
// ============================================================================

const THR: u16 = 0; // Transmit Holding Register (write, DLAB=0)
const RBR: u16 = 0; // Receive Buffer Register (read, DLAB=0)
const DLL: u16 = 0; // Divisor Latch Low (DLAB=1)
const IER: u16 = 1; // Interrupt Enable Register (DLAB=0)
const DLH: u16 = 1; // Divisor Latch High (DLAB=1)
const FCR: u16 = 2; // FIFO Control Register (write)
const IIR: u16 = 2; // Interrupt Identity Register (read)
const LCR: u16 = 3; // Line Control Register
const MCR: u16 = 4; // Modem Control Register
const LSR: u16 = 5; // Line Status Register
#[allow(dead_code)]
const MSR: u16 = 6; // Modem Status Register
#[allow(dead_code)]
const SCR: u16 = 7; // Scratch Register

// Line Status Register bits
const LSR_DATA_READY: u8 = 0x01;
const LSR_THR_EMPTY: u8 = 0x20;
#[allow(dead_code)]
const LSR_TEMT: u8 = 0x40;

// ============================================================================
// Convenience wrappers for port I/O
// ============================================================================

#[inline]
fn write_port(port: u16, value: u8) {
    unsafe {
        AssemblyShim::outb(port, value);
    }
}

#[inline]
fn read_port(port: u16) -> u8 {
    unsafe { AssemblyShim::inb(port) }
}

// ============================================================================
// UART Register Map
// ============================================================================

/// Represents the 16550 UART register block at a given port base.
pub struct UartRegisters {
    port_base: u16,
}

impl UartRegisters {
    /// Create a new register block at the given port base.
    pub const fn new(port_base: u16) -> Self {
        Self { port_base }
    }

    /// Read the Receive Buffer Register (DLAB must be 0).
    #[inline]
    pub fn read_rbr(&self) -> u8 {
        read_port(self.port_base + RBR)
    }

    /// Write to the Transmit Holding Register (DLAB must be 0).
    #[inline]
    pub fn write_thr(&self, value: u8) {
        write_port(self.port_base + THR, value);
    }

    /// Write to the Interrupt Enable Register (DLAB must be 0).
    #[inline]
    pub fn write_ier(&self, value: u8) {
        write_port(self.port_base + IER, value);
    }

    /// Write to the FIFO Control Register.
    #[inline]
    pub fn write_fcr(&self, value: u8) {
        write_port(self.port_base + FCR, value);
    }

    /// Read the Interrupt Identity Register.
    #[inline]
    pub fn read_iir(&self) -> u8 {
        read_port(self.port_base + IIR)
    }

    /// Write to the Line Control Register.
    #[inline]
    pub fn write_lcr(&self, value: u8) {
        write_port(self.port_base + LCR, value);
    }

    /// Write to the Modem Control Register.
    #[inline]
    pub fn write_mcr(&self, value: u8) {
        write_port(self.port_base + MCR, value);
    }

    /// Read the Line Status Register.
    #[inline]
    pub fn read_lsr(&self) -> u8 {
        read_port(self.port_base + LSR)
    }

    /// Write the Divisor Latch Low byte (DLAB must be 1).
    #[inline]
    pub fn write_dll(&self, value: u8) {
        write_port(self.port_base + DLL, value);
    }

    /// Write the Divisor Latch High byte (DLAB must be 1).
    #[inline]
    pub fn write_dlh(&self, value: u8) {
        write_port(self.port_base + DLH, value);
    }
}

// ============================================================================
// Serial Port Driver
// ============================================================================

/// A serial port using the 16550 UART, gated by capabilities.
pub struct SerialPort {
    port_base: u16,
    initialized: bool,
    capabilities: CapabilitySet,
}

impl SerialPort {
    /// Create a new serial port at the given I/O port base.
    ///
    /// Standard COM ports:
    /// - COM1: 0x3F8
    /// - COM2: 0x2F8
    /// - COM3: 0x3E8
    /// - COM4: 0x2E8
    pub const fn new(port_base: u16) -> Self {
        Self {
            port_base,
            initialized: false,
            capabilities: CapabilitySet::empty(),
        }
    }

    /// Initialize the serial port: 115200 baud, 8n1, FIFO enabled.
    ///
    /// Required capabilities: `CONSOLE | IO_PORT`
    pub fn init(&mut self) {
        // Disable interrupts
        write_port(self.port_base + IER, 0x00);

        // Set DLAB=1 to access divisor latches
        write_port(self.port_base + LCR, 0x80);

        // Set divisor for 115200 baud (divisor = 1 with 1.8432 MHz crystal)
        write_port(self.port_base + DLL, 0x01);
        write_port(self.port_base + DLH, 0x00);

        // Set DLAB=0, 8n1 (8 bits, no parity, 1 stop bit)
        write_port(self.port_base + LCR, 0x03);

        // Enable FIFO, clear them, with 14-byte threshold
        write_port(self.port_base + FCR, 0xC7);

        // Set RTS/DSR (modem control)
        write_port(self.port_base + MCR, 0x0B);

        // Enable receive-ready interrupt
        write_port(self.port_base + IER, 0x01);

        self.initialized = true;
        self.capabilities = CapabilitySet {
            bits: CapabilitySet::CONSOLE | CapabilitySet::IO_PORT,
        };
    }

    /// Whether the port has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Check that the serial port has sufficient capabilities.
    fn check_capabilities(&self) -> bool {
        self.capabilities.has(CapabilitySet::CONSOLE)
            && self.capabilities.has(CapabilitySet::IO_PORT)
    }

    /// Transmit a single byte (busy-wait until THR is empty).
    pub fn send(&mut self, byte: u8) {
        if !self.initialized || !self.check_capabilities() {
            return;
        }
        // Wait for the Transmit Holding Register to be empty
        while (read_port(self.port_base + LSR) & LSR_THR_EMPTY) == 0 {
            core::hint::spin_loop();
        }
        write_port(self.port_base + THR, byte);
    }

    /// Receive a single byte if available (non-blocking).
    pub fn receive(&mut self) -> Option<u8> {
        if !self.initialized || !self.check_capabilities() {
            return None;
        }
        if (read_port(self.port_base + LSR) & LSR_DATA_READY) != 0 {
            Some(read_port(self.port_base + RBR))
        } else {
            None
        }
    }

    /// Check if the Transmit Holding Register is empty (can send).
    pub fn is_transmit_empty(&self) -> bool {
        if !self.initialized {
            return false;
        }
        (read_port(self.port_base + LSR) & LSR_THR_EMPTY) != 0
    }

    /// Check if data is available to read.
    pub fn has_data(&self) -> bool {
        if !self.initialized {
            return false;
        }
        (read_port(self.port_base + LSR) & LSR_DATA_READY) != 0
    }

    /// Write a string to the serial port, expanding `\n` to `\r\n`.
    pub fn write_str(&mut self, s: &str) {
        for &byte in s.as_bytes() {
            if byte == b'\n' {
                self.send(b'\r');
            }
            self.send(byte);
        }
    }

    /// Write a string followed by `\r\n`.
    pub fn write_line(&mut self, s: &str) {
        self.write_str(s);
        self.send(b'\r');
        self.send(b'\n');
    }

    /// Log a structured message with a level prefix.
    ///
    /// Format: `[LEVEL] message`
    pub fn log(&mut self, level: LogLevel, msg: &str) {
        self.write_str("[");
        self.write_str(level.prefix());
        self.write_str("] ");
        self.write_line(msg);
    }

    /// Return the capabilities required by this driver.
    pub fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerialPort::write_str(self, s);
        Ok(())
    }
}

// ============================================================================
// Global serial singleton for early boot logging
// ============================================================================

/// Global serial port instance (COM1 at standard port 0x3F8).
///
/// This can be used for early boot logging before the kernel is fully
/// initialized. Access it through the convenience functions below.
pub static mut SERIAL: SerialPort = SerialPort::new(0x3F8);

/// Initialize the global serial port (COM1).
///
/// Called early in the boot sequence after the nucleus is set up.
#[allow(static_mut_refs)]
pub fn serial_init() {
    unsafe {
        SERIAL.init();
        SERIAL.write_line("=== Phorensic OS Serial Console ===");
    }
}

/// Write a string to the global serial port.
#[allow(static_mut_refs)]
pub fn serial_write(s: &str) {
    unsafe {
        SERIAL.write_str(s);
    }
}

/// Write a line (`\r\n` terminated) to the global serial port.
#[allow(static_mut_refs)]
pub fn serial_write_line(s: &str) {
    unsafe {
        SERIAL.write_line(s);
    }
}

/// SHA-256 hash for sealed package verification.
///
/// Uses the standard sha2 crate for cryptographic-strength hashing.
pub fn hash_bytes_32(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Log a structured message to the global serial port.
#[allow(static_mut_refs)]
pub fn serial_log(level: LogLevel, msg: &str) {
    unsafe {
        SERIAL.log(level, msg);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_port_new() {
        let port = SerialPort::new(0x3F8);
        assert!(!port.is_initialized());
        assert_eq!(port.port_base, 0x3F8);
        assert_eq!(port.capabilities.bits, 0);
    }

    #[test]
    fn test_log_level_prefix() {
        assert_eq!(LogLevel::Debug.prefix(), "DEBUG");
        assert_eq!(LogLevel::Info.prefix(), "INFO");
        assert_eq!(LogLevel::Warn.prefix(), "WARN");
        assert_eq!(LogLevel::Error.prefix(), "ERROR");
    }

    #[test]
    fn test_register_constants() {
        assert_eq!(THR, 0);
        assert_eq!(RBR, 0);
        assert_eq!(DLL, 0);
        assert_eq!(IER, 1);
        assert_eq!(DLH, 1);
        assert_eq!(FCR, 2);
        assert_eq!(IIR, 2);
        assert_eq!(LCR, 3);
        assert_eq!(MCR, 4);
        assert_eq!(LSR, 5);
        assert_eq!(MSR, 6);
        assert_eq!(SCR, 7);
    }

    #[test]
    fn test_lsr_bits() {
        assert_eq!(LSR_DATA_READY, 0x01);
        assert_eq!(LSR_THR_EMPTY, 0x20);
        assert_eq!(LSR_TEMT, 0x40);
    }

    #[test]
    fn test_uart_registers_new() {
        let regs = UartRegisters::new(0x3F8);
        // No way to test port I/O without real hardware, but we can verify
        // the struct layout is correct.
        assert_eq!(regs.port_base, 0x3F8);
    }

    #[test]
    fn test_uninitialized_returns_safe_defaults() {
        let mut port = SerialPort::new(0x3F8);
        assert_eq!(port.receive(), None);
        assert!(!port.is_transmit_empty());
        assert!(!port.has_data());
    }

    #[test]
    fn test_capabilities_after_manual_init() {
        let mut port = SerialPort::new(0x3F8);
        assert!(!port.is_initialized());
        // Simulate initialization for struct-level tests
        port.initialized = true;
        assert!(port.is_initialized());
    }

    #[test]
    #[allow(static_mut_refs)]
    fn test_serial_static_singleton_safe_to_read() {
        // Just verify the static has the right port base
        unsafe {
            assert_eq!(SERIAL.port_base, 0x3F8);
            assert!(!SERIAL.is_initialized());
        }
    }
}
