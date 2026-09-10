// Phost Boot — UEFI GOP initialization, ASM stub handoff, kernel loader
// Handles the transition from BIOS/UEFI boot stub to Phorensic kernel

use core::ptr;

// ============================================================================
// UEFI GOP (Graphics Output Protocol) constants
// ============================================================================

/// GUID for UEFI Graphics Output Protocol
pub const GOP_GUID: [u8; 16] = [
    0xDE, 0xA9, 0x42, 0x90, 0xDC, 0x23, 0x38, 0x4A, 0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A,
];

/// GOP pixel formats
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GopPixelFormat {
    PixelRedGreenBlueReserved8Bit = 0,
    PixelBlueGreenRedReserved8Bit = 1,
    PixelBitMask = 2,
    PixelBltOnly = 3,
    PixelFormatMax = 4,
}

/// GOP mode information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GopModeInfo {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: GopPixelFormat,
    pub pixels_per_scanline: u32,
}

/// GOP mode (from UEFI protocol)
#[repr(C)]
pub struct GopMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *mut GopModeInfo,
    pub info_size: usize,
    pub framebuffer_base: *mut u8,
    pub framebuffer_size: usize,
}

/// GOP protocol (simplified UEFI protocol struct)
#[repr(C)]
pub struct GopProtocol {
    pub query_mode: *mut u8, // function pointer
    pub set_mode: *mut u8,   // function pointer
    pub blt: *mut u8,        // function pointer
    pub mode: *mut GopMode,
}

// ============================================================================
// Boot information — passed from ASM stub to kernel
// ============================================================================

/// Boot info passed from the ASM boot stub to the kernel entry point
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BootInfo {
    /// Boot signature (0xAA55)
    pub boot_signature: u16,
    /// Boot drive number (from BIOS)
    pub boot_drive: u8,
    /// UEFI present flag
    pub uefi_present: bool,
    /// Pointer to UEFI system table
    pub uefi_system_table: *mut u8,
    /// Framebuffer physical address
    pub framebuffer_addr: u64,
    /// Framebuffer width in pixels
    pub framebuffer_width: u32,
    /// Framebuffer height in pixels
    pub framebuffer_height: u32,
    /// Framebuffer pixels per scanline
    pub framebuffer_pitch: u32,
    /// Framebuffer pixel format
    pub framebuffer_format: u32,
    /// Memory map address
    pub memory_map_addr: u64,
    /// Memory map entry count
    pub memory_map_count: u64,
    /// Kernel entry point
    pub kernel_entry: u64,
    /// Reserved
    pub reserved: [u64; 8],
}

impl BootInfo {
    pub const fn empty() -> Self {
        Self {
            boot_signature: 0,
            boot_drive: 0,
            uefi_present: false,
            uefi_system_table: ptr::null_mut(),
            framebuffer_addr: 0,
            framebuffer_width: 0,
            framebuffer_height: 0,
            framebuffer_pitch: 0,
            framebuffer_format: 0,
            memory_map_addr: 0,
            memory_map_count: 0,
            kernel_entry: 0,
            reserved: [0; 8],
        }
    }

    /// Check if a framebuffer was initialized
    pub fn has_framebuffer(&self) -> bool {
        self.framebuffer_addr != 0 && self.framebuffer_width > 0 && self.framebuffer_height > 0
    }

    /// Framebuffer size in bytes
    pub fn framebuffer_size(&self) -> u64 {
        self.framebuffer_pitch as u64 * self.framebuffer_height as u64
    }

    /// Read a BootInfo structure from the kernel boot page at the given physical address.
    ///
    /// In the real boot path, the ASM stub writes boot info at 0x108000 before
    /// transitioning to long mode. This method reads that page and reconstructs
    /// the BootInfo. Returns `None` if the signature is invalid.
    ///
    /// # Safety
    /// The caller must ensure the address points to a valid BootInfo written by
    /// the boot stub and that the memory is mapped and accessible.
    pub unsafe fn from_boot_page(phys_addr: u64) -> Option<Self> {
        let ptr = phys_addr as *const BootInfo;
        let info = ptr.read();
        if info.boot_signature != 0xAA55 {
            return None;
        }
        Some(info)
    }

    /// Read framebuffer info from the compact boot page layout written by the
    /// x86_64_boot.asm stub.
    ///
    /// The boot stub writes a compact layout at 0x108000:
    ///   +0x00 framebuffer_addr (u64)
    ///   +0x08 width (u32)
    ///   +0x0C height (u32)
    ///   +0x10 pitch (u32)
    ///   +0x14 bpp (u32)
    ///
    /// This layout has no 0xAA55 signature — it is a raw 5-field struct.
    /// The address is validated non-zero as a basic sanity check.
    ///
    /// # Safety
    /// The caller must ensure the address points to valid framebuffer metadata
    /// written by the boot stub.
    pub unsafe fn from_boot_framebuffer_page(phys_addr: u64) -> Option<Self> {
        let ptr = phys_addr as *const u8;
        let fb_addr = u64::from_le_bytes([
            ptr.read(),
            ptr.add(1).read(),
            ptr.add(2).read(),
            ptr.add(3).read(),
            ptr.add(4).read(),
            ptr.add(5).read(),
            ptr.add(6).read(),
            ptr.add(7).read(),
        ]);
        if fb_addr == 0 {
            return None;
        }
        let width = u32::from_le_bytes([
            ptr.add(8).read(),
            ptr.add(9).read(),
            ptr.add(10).read(),
            ptr.add(11).read(),
        ]);
        let height = u32::from_le_bytes([
            ptr.add(12).read(),
            ptr.add(13).read(),
            ptr.add(14).read(),
            ptr.add(15).read(),
        ]);
        let pitch = u32::from_le_bytes([
            ptr.add(16).read(),
            ptr.add(17).read(),
            ptr.add(18).read(),
            ptr.add(19).read(),
        ]);
        let _bpp = u32::from_le_bytes([
            ptr.add(20).read(),
            ptr.add(21).read(),
            ptr.add(22).read(),
            ptr.add(23).read(),
        ]);

        Some(Self {
            boot_signature: 0,
            boot_drive: 0,
            uefi_present: true,
            uefi_system_table: core::ptr::null_mut(),
            framebuffer_addr: fb_addr,
            framebuffer_width: width,
            framebuffer_height: height,
            framebuffer_pitch: pitch,
            framebuffer_format: 1, // BGRx
            memory_map_addr: 0,
            memory_map_count: 0,
            kernel_entry: 0,
            reserved: [0; 8],
        })
    }
}

// ============================================================================
// Compact Boot Framebuffer Info — ABI between ASM stub and kernel runtime
// ============================================================================

/// Compact framebuffer info written by the ASM boot stub at 0x108000.
/// This is the official ABI between the boot stub and the kernel runtime.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootFramebufferInfo {
    /// Physical address of the linear framebuffer
    pub fb_addr: u64,
    /// Width in pixels
    pub fb_width: u32,
    /// Height in pixels
    pub fb_height: u32,
    /// Bytes per scanline (pitch/stride)
    pub fb_pitch: u32,
    /// Bytes per pixel (typically 4 for 32-bit)
    pub fb_bpp: u32,
}

impl BootFramebufferInfo {
    /// Read from the boot page at the given physical address.
    /// Returns None if the address is null or framebuffer is 0.
    pub unsafe fn read_from(phys_addr: u64) -> Option<Self> {
        if phys_addr == 0 {
            return None;
        }
        let ptr = phys_addr as *const u8;
        let fb_addr = u64::from_le_bytes([
            ptr.read(),
            ptr.add(1).read(),
            ptr.add(2).read(),
            ptr.add(3).read(),
            ptr.add(4).read(),
            ptr.add(5).read(),
            ptr.add(6).read(),
            ptr.add(7).read(),
        ]);
        if fb_addr == 0 {
            return None;
        }
        Some(Self {
            fb_addr,
            fb_width: u32::from_le_bytes([
                ptr.add(8).read(),
                ptr.add(9).read(),
                ptr.add(10).read(),
                ptr.add(11).read(),
            ]),
            fb_height: u32::from_le_bytes([
                ptr.add(12).read(),
                ptr.add(13).read(),
                ptr.add(14).read(),
                ptr.add(15).read(),
            ]),
            fb_pitch: u32::from_le_bytes([
                ptr.add(16).read(),
                ptr.add(17).read(),
                ptr.add(18).read(),
                ptr.add(19).read(),
            ]),
            fb_bpp: u32::from_le_bytes([
                ptr.add(20).read(),
                ptr.add(21).read(),
                ptr.add(22).read(),
                ptr.add(23).read(),
            ]),
        })
    }
}

// ============================================================================
// Framebuffer — pixel buffer abstraction
// ============================================================================

/// A simple linear framebuffer for boot-time display
#[repr(C)]
pub struct Framebuffer {
    /// Pointer to pixel data
    pub base: *mut u8,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per scanline
    pub pitch: u32,
    /// Bytes per pixel
    pub bpp: u8,
    /// Pixel format
    pub format: GopPixelFormat,
}

impl Framebuffer {
    /// Create from boot info
    pub fn from_boot_info(info: &BootInfo) -> Option<Self> {
        if !info.has_framebuffer() {
            return None;
        }
        Some(Self {
            base: info.framebuffer_addr as *mut u8,
            width: info.framebuffer_width,
            height: info.framebuffer_height,
            pitch: info.framebuffer_pitch,
            bpp: 4, // 32-bit color
            format: GopPixelFormat::PixelBlueGreenRedReserved8Bit,
        })
    }

    /// Get pixel offset
    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.pitch as usize) + (x as usize * self.bpp as usize)
    }

    /// Write a pixel (BGRx format)
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let off = self.offset(x, y);
        unsafe {
            let p = self.base.add(off);
            p.write(b);
            p.add(1).write(g);
            p.add(2).write(r);
            // byte 3 (reserved/alpha) is left as-is
        }
    }

    /// Clear the screen to a solid color
    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set_pixel(x, y, r, g, b);
            }
        }
    }

    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, r: u8, g: u8, b: u8) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);
        for py in y..y_end {
            for px in x..x_end {
                self.set_pixel(px, py, r, g, b);
            }
        }
    }

    /// Draw a horizontal line
    pub fn hline(&mut self, x: u32, y: u32, w: u32, r: u8, g: u8, b: u8) {
        let x_end = (x + w).min(self.width);
        for px in x..x_end {
            self.set_pixel(px, y, r, g, b);
        }
    }

    /// Draw a vertical line
    pub fn vline(&mut self, x: u32, y: u32, h: u32, r: u8, g: u8, b: u8) {
        let y_end = (y + h).min(self.height);
        for py in y..y_end {
            self.set_pixel(x, py, r, g, b);
        }
    }

    /// Draw a simple character (8x8 bitmap font, ASCII 32-126)
    pub fn draw_char(&mut self, x: u32, y: u32, c: char, fg_r: u8, fg_g: u8, fg_b: u8) {
        // FONT8X8 is indexed from ASCII 32 (space)
        if !(' '..='~').contains(&c) {
            return;
        }
        let idx = c as usize - 32;
        let glyph = FONT8X8[idx];
        for row in 0..8 {
            let bits = glyph.bytes[row];
            for col in 0..8 {
                if bits & (0x80 >> col) != 0 {
                    self.set_pixel(x + col as u32, y + row as u32, fg_r, fg_g, fg_b);
                }
            }
        }
    }

    /// Draw a string at position
    pub fn draw_str(&mut self, x: u32, y: u32, s: &str, fg_r: u8, fg_g: u8, fg_b: u8) {
        let mut cx = x;
        for c in s.chars() {
            if c == '\n' {
                cx = x;
                continue;
            }
            self.draw_char(cx, y, c, fg_r, fg_g, fg_b);
            cx += 9; // 8 px char + 1 px spacing
            if cx + 8 >= self.width {
                break;
            }
        }
    }
}

// ============================================================================
// 8x8 Bitmap Font (ASCII 32-126)
// ============================================================================

#[derive(Clone, Copy)]
struct Glyph {
    bytes: [u8; 8],
}

const FONT8X8: &[Glyph; 95] = &[
    Glyph {
        bytes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }, // space
    Glyph {
        bytes: [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
    }, // !
    Glyph {
        bytes: [0x6C, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00, 0x00],
    }, // "
    Glyph {
        bytes: [0x6C, 0x6C, 0xFE, 0x6C, 0xFE, 0x6C, 0x6C, 0x00],
    }, // #
    Glyph {
        bytes: [0x18, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x18, 0x00],
    }, // $
    Glyph {
        bytes: [0x00, 0xC6, 0xCC, 0x18, 0x30, 0x66, 0xC6, 0x00],
    }, // %
    Glyph {
        bytes: [0x38, 0x6C, 0x38, 0x76, 0xDC, 0xCC, 0x76, 0x00],
    }, // &
    Glyph {
        bytes: [0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00],
    }, // '
    Glyph {
        bytes: [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
    }, // (
    Glyph {
        bytes: [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
    }, // )
    Glyph {
        bytes: [0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00],
    }, // *
    Glyph {
        bytes: [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
    }, // +
    Glyph {
        bytes: [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30, 0x00],
    }, // ,
    Glyph {
        bytes: [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
    }, // -
    Glyph {
        bytes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
    }, // .
    Glyph {
        bytes: [0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0x80, 0x00],
    }, // /
    Glyph {
        bytes: [0x3C, 0x66, 0x6E, 0x7E, 0x76, 0x66, 0x3C, 0x00],
    }, // 0
    Glyph {
        bytes: [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
    }, // 1
    Glyph {
        bytes: [0x3C, 0x66, 0x06, 0x0C, 0x30, 0x60, 0x7E, 0x00],
    }, // 2
    Glyph {
        bytes: [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
    }, // 3
    Glyph {
        bytes: [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
    }, // 4
    Glyph {
        bytes: [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
    }, // 5
    Glyph {
        bytes: [0x3C, 0x66, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
    }, // 6
    Glyph {
        bytes: [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
    }, // 7
    Glyph {
        bytes: [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
    }, // 8
    Glyph {
        bytes: [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00],
    }, // 9
    Glyph {
        bytes: [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
    }, // :
    Glyph {
        bytes: [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00],
    }, // ;
    Glyph {
        bytes: [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
    }, // <
    Glyph {
        bytes: [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
    }, // =
    Glyph {
        bytes: [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
    }, // >
    Glyph {
        bytes: [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x00, 0x18, 0x00],
    }, // ?
    Glyph {
        bytes: [0x3C, 0x66, 0x6E, 0x6E, 0x60, 0x66, 0x3C, 0x00],
    }, // @
    Glyph {
        bytes: [0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
    }, // A
    Glyph {
        bytes: [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
    }, // B
    Glyph {
        bytes: [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
    }, // C
    Glyph {
        bytes: [0x7C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x00],
    }, // D
    Glyph {
        bytes: [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
    }, // E
    Glyph {
        bytes: [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
    }, // F
    Glyph {
        bytes: [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
    }, // G
    Glyph {
        bytes: [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
    }, // H
    Glyph {
        bytes: [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
    }, // I
    Glyph {
        bytes: [0x3E, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38, 0x00],
    }, // J
    Glyph {
        bytes: [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
    }, // K
    Glyph {
        bytes: [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
    }, // L
    Glyph {
        bytes: [0xC6, 0xEE, 0xFE, 0xD6, 0xC6, 0xC6, 0xC6, 0x00],
    }, // M
    Glyph {
        bytes: [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
    }, // N
    Glyph {
        bytes: [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
    }, // O
    Glyph {
        bytes: [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
    }, // P
    Glyph {
        bytes: [0x3C, 0x66, 0x66, 0x66, 0x6E, 0x3C, 0x0E, 0x00],
    }, // Q
    Glyph {
        bytes: [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
    }, // R
    Glyph {
        bytes: [0x3C, 0x66, 0x70, 0x3C, 0x0E, 0x66, 0x3C, 0x00],
    }, // S
    Glyph {
        bytes: [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
    }, // T
    Glyph {
        bytes: [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
    }, // U
    Glyph {
        bytes: [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
    }, // V
    Glyph {
        bytes: [0xC6, 0xC6, 0xC6, 0xD6, 0xFE, 0xEE, 0xC6, 0x00],
    }, // W
    Glyph {
        bytes: [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
    }, // X
    Glyph {
        bytes: [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
    }, // Y
    Glyph {
        bytes: [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
    }, // Z
    Glyph {
        bytes: [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
    }, // [
    Glyph {
        bytes: [0xC0, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x02, 0x00],
    }, // backslash
    Glyph {
        bytes: [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
    }, // ]
    Glyph {
        bytes: [0x10, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00],
    }, // ^
    Glyph {
        bytes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
    }, // _
    Glyph {
        bytes: [0x18, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00],
    }, // `
    Glyph {
        bytes: [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
    }, // a
    Glyph {
        bytes: [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
    }, // b
    Glyph {
        bytes: [0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00],
    }, // c
    Glyph {
        bytes: [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
    }, // d
    Glyph {
        bytes: [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
    }, // e
    Glyph {
        bytes: [0x1C, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x30, 0x00],
    }, // f
    Glyph {
        bytes: [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
    }, // g
    Glyph {
        bytes: [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
    }, // h
    Glyph {
        bytes: [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
    }, // i
    Glyph {
        bytes: [0x06, 0x00, 0x0E, 0x06, 0x06, 0x06, 0x36, 0x1C],
    }, // j
    Glyph {
        bytes: [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
    }, // k
    Glyph {
        bytes: [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
    }, // l
    Glyph {
        bytes: [0x00, 0x00, 0xEC, 0xFE, 0xD6, 0xC6, 0xC6, 0x00],
    }, // m
    Glyph {
        bytes: [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
    }, // n
    Glyph {
        bytes: [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
    }, // o
    Glyph {
        bytes: [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
    }, // p
    Glyph {
        bytes: [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
    }, // q
    Glyph {
        bytes: [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
    }, // r
    Glyph {
        bytes: [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
    }, // s
    Glyph {
        bytes: [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
    }, // t
    Glyph {
        bytes: [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
    }, // u
    Glyph {
        bytes: [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
    }, // v
    Glyph {
        bytes: [0x00, 0x00, 0xC6, 0xC6, 0xD6, 0xFE, 0x6C, 0x00],
    }, // w
    Glyph {
        bytes: [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
    }, // x
    Glyph {
        bytes: [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
    }, // y
    Glyph {
        bytes: [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
    }, // z
    Glyph {
        bytes: [0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
    }, // {
    Glyph {
        bytes: [0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x00],
    }, // |
    Glyph {
        bytes: [0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
    }, // }
    Glyph {
        bytes: [0x76, 0xDC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }, // ~
];
