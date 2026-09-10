// Phost — Scrolling Text Console
// Implements a scrollable text region on a Canvas with cursor tracking,
// configurable colors, and core::fmt::Write support for format! macros.

use crate::canvas::Canvas;
use alloc::string::ToString;
use core::fmt;

/// Character cell dimensions (matching the 8x8 bitmap font + 1px spacing).
pub const CHAR_WIDTH: u32 = 8;
pub const CHAR_HEIGHT: u32 = 9; // 8px glyph + 1px spacing

/// A scrollable text console rendered on a Canvas.
///
/// The console defines a rectangular region in pixel coordinates. Text is
/// drawn character-by-character with automatic scrolling when the cursor
/// reaches the bottom of the region.
pub struct Console {
    canvas: Canvas,
    /// Console region origin and size (pixels)
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Cursor position in character cells
    pub cursor_x: u32,
    pub cursor_y: u32,
    /// Number of character columns and rows
    pub cols: u32,
    pub rows: u32,
    /// Foreground (text) and background colors
    fg_r: u8,
    fg_g: u8,
    fg_b: u8,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
}

impl Console {
    /// Create a new Console on the given Canvas within the specified region.
    ///
    /// `x`, `y`, `width`, `height` define the console area in pixel coordinates.
    pub fn new(canvas: Canvas, x: u32, y: u32, width: u32, height: u32) -> Self {
        let cols = width / 9;
        let rows = height / 9;
        Self {
            canvas,
            x,
            y,
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            cols,
            rows,
            fg_r: 0xCC,
            fg_g: 0xCC,
            fg_b: 0xCC,
            bg_r: 0x0A,
            bg_g: 0x0A,
            bg_b: 0x0E,
        }
    }

    // ── Color configuration ────────────────────────────────────────────

    /// Set the foreground (text) color.
    pub fn set_fg(&mut self, r: u8, g: u8, b: u8) {
        self.fg_r = r;
        self.fg_g = g;
        self.fg_b = b;
    }

    /// Set the background color.
    pub fn set_bg(&mut self, r: u8, g: u8, b: u8) {
        self.bg_r = r;
        self.bg_g = g;
        self.bg_b = b;
    }

    // ── Cursor helpers ─────────────────────────────────────────────────

    /// Pixel X of the cursor.
    fn cursor_pixel_x(&self) -> u32 {
        self.x + self.cursor_x * 9
    }

    /// Pixel Y of the cursor.
    fn cursor_pixel_y(&self) -> u32 {
        self.y + self.cursor_y * 9
    }

    /// Advance cursor to the next line. Scrolls if at the bottom.
    fn newline(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 >= self.rows {
            self.scroll(1);
        } else {
            self.cursor_y += 1;
        }
    }

    // ── Core operations ────────────────────────────────────────────────

    /// Write a single character to the console.
    ///
    /// Handles printable ASCII, newlines, and wraps long lines.
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.newline();
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                // Tab = 4 spaces
                let stop = 4;
                let spaces = stop - (self.cursor_x % stop);
                for _ in 0..spaces {
                    self.write_char(' ');
                }
            }
            c if c.is_ascii_graphic() || c == ' ' => {
                // Check if we need to wrap
                if self.cursor_x >= self.cols {
                    self.newline();
                }
                // Draw character with background
                let px = self.cursor_pixel_x();
                let py = self.cursor_pixel_y();
                self.canvas.draw_text_with_bg(
                    px,
                    py,
                    &c.to_string(),
                    self.fg_r,
                    self.fg_g,
                    self.fg_b,
                    self.bg_r,
                    self.bg_g,
                    self.bg_b,
                );
                self.cursor_x += 1;
            }
            _ => {
                // Non-printable / non-ASCII: skip or show placeholder
                self.write_char('.');
            }
        }
    }

    /// Write a string to the console.
    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    /// Write a string followed by a newline.
    pub fn write_line(&mut self, s: &str) {
        self.write_str(s);
        self.write_char('\n');
    }

    /// Clear the console region (fill with background color).
    pub fn clear(&mut self) {
        self.canvas.fill_rect(
            self.x,
            self.y,
            self.width,
            self.height,
            self.bg_r,
            self.bg_g,
            self.bg_b,
        );
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Scroll the console region up by the given number of pixel lines.
    /// Only shifts data within the console rectangle — never touches pixels
    /// outside the console region.
    pub fn scroll(&mut self, lines: u32) {
        let fb = self.canvas.framebuffer();
        let pitch = fb.pitch as usize;
        let bpp = fb.bpp as usize;
        // Bytes per row of the console region (not the full framebuffer)
        let region_row_bytes = self.width as usize * bpp;
        let line_pixels = lines as usize;

        unsafe {
            let base = fb.base;
            // Shift each row of the console region upward
            for row in line_pixels..self.height as usize {
                let src_off = (self.y as usize + row) * pitch + self.x as usize * bpp;
                let dst_off = (self.y as usize + row - line_pixels) * pitch + self.x as usize * bpp;
                core::ptr::copy(base.add(src_off), base.add(dst_off), region_row_bytes);
            }
        }

        // Clear the exposed bottom lines
        let clear_y = self.y + self.height - lines;
        self.canvas.fill_rect(
            self.x, clear_y, self.width, lines, self.bg_r, self.bg_g, self.bg_b,
        );

        // Adjust cursor upward
        let char_lines = lines / 9;
        if self.cursor_y >= char_lines {
            self.cursor_y -= char_lines;
        } else {
            self.cursor_y = 0;
        }
    }

    /// Get a mutable reference to the inner Canvas.
    pub fn canvas(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Consume the Console and return the underlying Canvas.
    pub fn into_canvas(self) -> Canvas {
        self.canvas
    }
}

// ── core::fmt::Write implementation ──────────────────────────────────────

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}
