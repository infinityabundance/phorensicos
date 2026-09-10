// Phorensic OS — Boot Status Screen
// Displays boot progress, diagnostics, and system state on the framebuffer
// Runs before the full compositor is available (boot-time fallback display)

use crate::boot::{BootInfo, Framebuffer};

/// Boot phase indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    BiosInit,
    Bootloader,
    UefiGop,
    KernelLoad,
    MemoryInit,
    CapabilityInit,
    SchedulerInit,
    GuiInit,
    CompositorReady,
    SystemReady,
    Panic,
}

impl BootPhase {
    pub fn label(&self) -> &'static str {
        match self {
            BootPhase::BiosInit => "BIOS INIT",
            BootPhase::Bootloader => "BOOTLOADER",
            BootPhase::UefiGop => "UEFI GOP",
            BootPhase::KernelLoad => "KERNEL LOAD",
            BootPhase::MemoryInit => "MEMORY INIT",
            BootPhase::CapabilityInit => "CAPABILITY INIT",
            BootPhase::SchedulerInit => "SCHEDULER INIT",
            BootPhase::GuiInit => "GUI INIT",
            BootPhase::CompositorReady => "COMPOSITOR READY",
            BootPhase::SystemReady => "SYSTEM READY",
            BootPhase::Panic => "PANIC",
        }
    }
}

/// Boot status screen — renders boot progress on framebuffer
pub struct StatusScreen {
    fb: Framebuffer,
    current_phase: BootPhase,
    messages: [MessageLine; 16],
    msg_count: usize,
    progress: f32,
}

#[derive(Clone, Copy)]
struct MessageLine {
    text: [u8; 64],
    len: usize,
    is_error: bool,
}

impl MessageLine {
    fn new(text: &str, is_error: bool) -> Self {
        let mut bytes = [0u8; 64];
        let len = text.len().min(63);
        bytes[..len].copy_from_slice(&text.as_bytes()[..len]);
        Self { text: bytes, len, is_error }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.text[..self.len]).unwrap_or("")
    }
}

impl StatusScreen {
    /// Create from boot info
    pub fn new(boot_info: &BootInfo) -> Option<Self> {
        let fb = Framebuffer::from_boot_info(boot_info)?;
        Some(Self {
            fb,
            current_phase: BootPhase::BiosInit,
            messages: [MessageLine::new("", false); 16],
            msg_count: 0,
            progress: 0.0,
        })
    }

    /// Draw the initial boot screen (solid background with logo area)
    pub fn init(&mut self) {
        // Dark background
        self.fb.clear(0x0A, 0x0A, 0x0E); // Very dark blue-gray

        let w = self.fb.width;
        let h = self.fb.height;

        // Top bar — accent color
        self.fb.fill_rect(0, 0, w, 8, 0x00, 0x88, 0xFF);

        // Bottom bar
        self.fb.fill_rect(0, h - 24, w, 24, 0x15, 0x15, 0x1E);

        // Title text
        let title = "Phorensic OS v0.1.0";
        let tx = (w - (title.len() as u32 * 9)) / 2;
        self.fb.draw_str(tx, 16, title, 0x00, 0xCC, 0xFF);

        // Subtitle
        let subtitle = "Forensic Residual-Primacy Operating System";
        let sx = (w - (subtitle.len() as u32 * 9)) / 2;
        self.fb.draw_str(sx, 28, subtitle, 0x88, 0x88, 0xAA);
    }

    /// Draw the progress bar
    fn draw_progress(&mut self, progress: f32) {
        let w = self.fb.width;
        let h = self.fb.height;
        let bar_w = w - 80;
        let bar_x = 40;
        let bar_y = h - 18;
        let bar_h = 12;

        // Background
        self.fb.fill_rect(bar_x - 1, bar_y - 1, bar_w + 2, bar_h + 2, 0x30, 0x30, 0x40);

        // Fill
        let fill = (bar_w as f32 * progress.clamp(0.0, 1.0)) as u32;
        if fill > 0 {
            self.fb.fill_rect(bar_x, bar_y, fill, bar_h, 0x00, 0xCC, 0x66);
        }
    }

    /// Transition to a new boot phase
    pub fn set_phase(&mut self, phase: BootPhase) {
        self.current_phase = phase;
        let w = self.fb.width;
        let h = self.fb.height;

        // Update status in top-right
        let status_x = w - (phase.label().len() as u32 * 9) - 10;
        self.fb.fill_rect(status_x - 4, 0, w - status_x + 4, 10, 0x00, 0x88, 0xFF);
        self.fb.draw_str(status_x, 1, phase.label(), 0xFF, 0xFF, 0xFF);

        // Update phase progress
        let phase_count = 10u32;
        let current = phase as u32;
        self.progress = (current as f32 + 0.5) / phase_count as f32;
        self.draw_progress(self.progress);
    }

    /// Add a log message to the status screen
    pub fn log(&mut self, text: &str, is_error: bool) {
        if self.msg_count < 16 {
            self.messages[self.msg_count] = MessageLine::new(text, is_error);
            self.msg_count += 1;
        } else {
            // Shift messages up
            self.messages.rotate_left(1);
            self.messages[15] = MessageLine::new(text, is_error);
        }
        self.render_messages();
    }

    /// Render the message area
    fn render_messages(&mut self) {
        // Clear message area
        let w = self.fb.width;
        self.fb.fill_rect(20, 50, w - 40, self.fb.height - 80, 0x0A, 0x0A, 0x0E);

        // Draw messages
        let start_y = 52u32;
        for (i, msg) in self.messages.iter().enumerate() {
            if i >= self.msg_count { break; }
            let (r, g, b) = if msg.is_error { (0xFF, 0x44, 0x44) } else { (0xCC, 0xCC, 0xCC) };
            self.fb.draw_str(24, start_y + (i as u32) * 10, msg.as_str(), r, g, b);
        }
    }

    /// Show a panic screen (fatal error)
    pub fn panic(&mut self, message: &str) {
        self.fb.clear(0x1A, 0x00, 0x00); // Dark red background
        let w = self.fb.width;
        let h = self.fb.height;

        // Big "PANIC" header
        let title = "*** KERNEL PANIC ***";
        let tx = (w - (title.len() as u32 * 9)) / 2;
        self.fb.draw_str(tx, h / 2 - 30, title, 0xFF, 0x00, 0x00);

        // Panic message
        let mx = (w - (message.len() as u32 * 9)) / 2;
        self.fb.draw_str(mx, h / 2 - 10, message, 0xFF, 0x88, 0x88);

        // Instruction
        let foot = "System halted. Reboot to continue.";
        let fx = (w - (foot.len() as u32 * 9)) / 2;
        self.fb.draw_str(fx, h / 2 + 10, foot, 0x88, 0x88, 0xAA);
    }

    /// Draw diagnostic counters
    pub fn draw_stats(&mut self, _cpu_count: u32, _mem_mb: u64, _processes: u32) {
        let w = self.fb.width;
        // CPU cores
        // Draw stats using static format strings
        self.fb.draw_str(40, self.fb.height - 22, "CPU: ...", 0x88, 0xCC, 0x88);
        self.fb.draw_str(200, self.fb.height - 22, "MEM: ... MB", 0x88, 0xCC, 0x88);
        self.fb.draw_str(w - 200, self.fb.height - 22, "PROC: ...", 0x88, 0xCC, 0x88);
    }

    /// Mark system as ready (green flash)
    pub fn system_ready(&mut self) {
        self.set_phase(BootPhase::SystemReady);
        // Draw a "Ready" overlay
        let w = self.fb.width;
        let msg = "SYSTEM READY";
        let x = (w - (msg.len() as u32 * 9)) / 2;
        self.fb.draw_str(x, self.fb.height / 2, msg, 0x00, 0xFF, 0x44);
        self.progress = 1.0;
        self.draw_progress(1.0);
    }

    /// Get a reference to the framebuffer
    pub fn framebuffer(&mut self) -> &mut Framebuffer {
        &mut self.fb
    }
}
