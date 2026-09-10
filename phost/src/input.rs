// input.rs — PS/2 Keyboard Input Driver
// Reads keystrokes from the PS/2 controller and converts to ASCII

use crate::nucleus::AssemblyShim;

/// PS/2 controller ports
const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;
#[allow(dead_code)]
const PS2_COMMAND: u16 = 0x64;

/// Status register bits
const PS2_OUTPUT_FULL: u8 = 0x01;

/// Modifier state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub caps_lock: bool,
}

/// A circular buffer of keyboard events
pub struct Keyboard {
    buffer: [char; 64],
    head: usize,
    tail: usize,
    modifiers: Modifiers,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            buffer: ['\0'; 64],
            head: 0,
            tail: 0,
            modifiers: Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                caps_lock: false,
            },
        }
    }

    /// Check if a key is available
    pub fn has_key(&self) -> bool {
        self.head != self.tail
    }

    /// Read the next key from buffer
    pub fn read_key(&mut self) -> Option<char> {
        if self.head == self.tail {
            return None;
        }
        let c = self.buffer[self.tail];
        self.tail = (self.tail + 1) % 64;
        Some(c)
    }

    /// Try to read a key; returns immediately if none available
    pub fn try_read(&mut self) -> Option<char> {
        self.read_key()
    }

    /// Read a full line from buffered keys
    pub fn read_line(&mut self, buffer: &mut [u8]) -> Option<usize> {
        let mut len = 0;
        while let Some(c) = self.read_key() {
            if c == '\n' || c == '\r' {
                buffer[len] = b'\n';
                len += 1;
                return Some(len);
            } else if c == '\x7f' || c == '\x08' {
                // backspace
                if len > 0 {
                    len -= 1;
                }
            } else if (c as u8).is_ascii() && len < buffer.len() - 1 {
                buffer[len] = c as u8;
                len += 1;
            }
        }
        if len > 0 {
            Some(len)
        } else {
            None
        }
    }

    /// Poll the PS/2 controller for new scancodes and process them
    pub fn poll(&mut self) {
        unsafe {
            // Check if output is ready
            if AssemblyShim::inb(PS2_STATUS) & PS2_OUTPUT_FULL != 0 {
                let scancode = AssemblyShim::inb(PS2_DATA);
                self.process_scancode(scancode);
            }
        }
    }

    fn process_scancode(&mut self, scancode: u8) {
        // Handle key release (bit 7 set)
        let released = (scancode & 0x80) != 0;
        let key = scancode & 0x7F;

        // PS/2 Set 1 scancode to ASCII conversion (basic)
        let ascii = match key {
            0x01 => Some(if released { None } else { Some('\x1b') }), // Escape
            0x02 => Some(Some(if self.modifiers.shift { '!' } else { '1' })),
            0x03 => Some(Some(if self.modifiers.shift { '@' } else { '2' })),
            0x04 => Some(Some(if self.modifiers.shift { '#' } else { '3' })),
            0x05 => Some(Some(if self.modifiers.shift { '$' } else { '4' })),
            0x06 => Some(Some(if self.modifiers.shift { '%' } else { '5' })),
            0x07 => Some(Some(if self.modifiers.shift { '^' } else { '6' })),
            0x08 => Some(Some(if self.modifiers.shift { '&' } else { '7' })),
            0x09 => Some(Some(if self.modifiers.shift { '*' } else { '8' })),
            0x0A => Some(Some(if self.modifiers.shift { '(' } else { '9' })),
            0x0B => Some(Some(if self.modifiers.shift { ')' } else { '0' })),
            0x0C => Some(Some(if self.modifiers.shift { '_' } else { '-' })),
            0x0D => Some(Some(if self.modifiers.shift { '+' } else { '=' })),
            0x0E => Some(Some('\x7f')), // Backspace
            0x0F => Some(Some('\t')),   // Tab
            0x10 => Some(Some(if self.modifiers.shift { 'Q' } else { 'q' })),
            0x11 => Some(Some(if self.modifiers.shift { 'W' } else { 'w' })),
            0x12 => Some(Some(if self.modifiers.shift { 'E' } else { 'e' })),
            0x13 => Some(Some(if self.modifiers.shift { 'R' } else { 'r' })),
            0x14 => Some(Some(if self.modifiers.shift { 'T' } else { 't' })),
            0x15 => Some(Some(if self.modifiers.shift { 'Y' } else { 'y' })),
            0x16 => Some(Some(if self.modifiers.shift { 'U' } else { 'u' })),
            0x17 => Some(Some(if self.modifiers.shift { 'I' } else { 'i' })),
            0x18 => Some(Some(if self.modifiers.shift { 'O' } else { 'o' })),
            0x19 => Some(Some(if self.modifiers.shift { 'P' } else { 'p' })),
            0x1A => Some(Some(if self.modifiers.shift { '{' } else { '[' })),
            0x1B => Some(Some(if self.modifiers.shift { '}' } else { ']' })),
            0x1C => Some(Some('\n')), // Enter
            0x1E => Some(Some(if self.modifiers.shift { 'A' } else { 'a' })),
            0x1F => Some(Some(if self.modifiers.shift { 'S' } else { 's' })),
            0x20 => Some(Some(if self.modifiers.shift { 'D' } else { 'd' })),
            0x21 => Some(Some(if self.modifiers.shift { 'F' } else { 'f' })),
            0x22 => Some(Some(if self.modifiers.shift { 'G' } else { 'g' })),
            0x23 => Some(Some(if self.modifiers.shift { 'H' } else { 'h' })),
            0x24 => Some(Some(if self.modifiers.shift { 'J' } else { 'j' })),
            0x25 => Some(Some(if self.modifiers.shift { 'K' } else { 'k' })),
            0x26 => Some(Some(if self.modifiers.shift { 'L' } else { 'l' })),
            0x27 => Some(Some(if self.modifiers.shift { ':' } else { ';' })),
            0x28 => Some(Some(if self.modifiers.shift { '"' } else { '\'' })),
            0x29 => Some(Some(if self.modifiers.shift { '~' } else { '`' })),
            0x2B => Some(Some(if self.modifiers.shift { '|' } else { '\\' })),
            0x2C => Some(Some(if self.modifiers.shift { 'Z' } else { 'z' })),
            0x2D => Some(Some(if self.modifiers.shift { 'X' } else { 'x' })),
            0x2E => Some(Some(if self.modifiers.shift { 'C' } else { 'c' })),
            0x2F => Some(Some(if self.modifiers.shift { 'V' } else { 'v' })),
            0x30 => Some(Some(if self.modifiers.shift { 'B' } else { 'b' })),
            0x31 => Some(Some(if self.modifiers.shift { 'N' } else { 'n' })),
            0x32 => Some(Some(if self.modifiers.shift { 'M' } else { 'm' })),
            0x33 => Some(Some(if self.modifiers.shift { '<' } else { ',' })),
            0x34 => Some(Some(if self.modifiers.shift { '>' } else { '.' })),
            0x35 => Some(Some(if self.modifiers.shift { '?' } else { '/' })),
            0x39 => Some(Some(' ')), // Space
            0x2A | 0x36 => {
                // Shift
                self.modifiers.shift = !released;
                None
            }
            0x1D => {
                // Ctrl
                self.modifiers.ctrl = !released;
                None
            }
            0x38 => {
                // Alt
                self.modifiers.alt = !released;
                None
            }
            0x3A => {
                // Caps Lock
                if !released {
                    self.modifiers.caps_lock = !self.modifiers.caps_lock;
                }
                None
            }
            _ => None, // Unknown/unmapped key
        };

        if let Some(Some(c)) = ascii {
            if !released {
                let next = (self.head + 1) % 64;
                if next != self.tail {
                    self.buffer[self.head] = c;
                    self.head = next;
                }
            }
        }
    }
}

/// Global keyboard instance
pub static mut KEYBOARD: Keyboard = Keyboard::new();
