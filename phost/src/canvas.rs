// Phost — Canvas Rendering Module
// High-level framebuffer abstraction with shape drawing, clipping, and scrolling

use crate::boot::Framebuffer;

/// Canvas wraps a Framebuffer with higher-level drawing primitives.
/// It owns the framebuffer data (raw pointer + metadata) independently
/// so there are no borrow-lifetime complications.
pub struct Canvas {
    fb: Framebuffer,
}

impl Canvas {
    /// Create a Canvas from an existing Framebuffer (copies the metadata).
    /// The underlying pixel buffer is shared, not cloned.
    pub fn new_from_framebuffer(fb: &mut Framebuffer) -> Self {
        Self {
            fb: Framebuffer {
                base: fb.base,
                width: fb.width,
                height: fb.height,
                pitch: fb.pitch,
                bpp: fb.bpp,
                format: fb.format,
            },
        }
    }

    // ── Accessors ──────────────────────────────────────────────────────

    pub fn width(&self) -> u32 {
        self.fb.width
    }

    pub fn height(&self) -> u32 {
        self.fb.height
    }

    /// Returns a mutable reference to the underlying Framebuffer.
    pub fn framebuffer(&mut self) -> &mut Framebuffer {
        &mut self.fb
    }

    // ── Internal pixel helpers ─────────────────────────────────────────

    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.fb.pitch as usize) + (x as usize * self.fb.bpp as usize)
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        if x >= self.fb.width || y >= self.fb.height {
            return;
        }
        let off = self.offset(x, y);
        unsafe {
            let p = self.fb.base.add(off);
            p.write(b);
            p.add(1).write(g);
            p.add(2).write(r);
            // byte 3 (reserved/alpha) is left as-is
        }
    }

    // ── Drawing primitives ─────────────────────────────────────────────

    /// Clear the entire canvas to a solid color.
    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        self.fb.clear(r, g, b);
    }

    /// Fill a rectangle with clipping to canvas bounds.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, r: u8, g: u8, b: u8) {
        self.fb.fill_rect(x, y, w, h, r, g, b);
    }

    /// Draw an outlined rectangle (1-pixel border).
    pub fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, r: u8, g: u8, b: u8) {
        if w == 0 || h == 0 {
            return;
        }
        self.fb.hline(x, y, w, r, g, b); // top
        self.fb.hline(x, y + h - 1, w, r, g, b); // bottom
        self.fb.vline(x, y, h, r, g, b); // left
        self.fb.vline(x + w - 1, y, h, r, g, b); // right
    }

    /// Draw a line using Bresenham's line algorithm.
    pub fn draw_line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, r: u8, g: u8, b: u8) {
        let mut x = x1 as i32;
        let mut y = y1 as i32;
        let dx = (x2 as i32 - x).abs();
        let dy = -(y2 as i32 - y).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.set_pixel(x as u32, y as u32, r, g, b);
            if x == x2 as i32 && y == y2 as i32 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a circle using the midpoint circle algorithm.
    pub fn draw_circle(&mut self, cx: u32, cy: u32, radius: u32, r: u8, g: u8, b: u8) {
        let cx = cx as i32;
        let cy = cy as i32;
        let radius = radius as i32;
        let mut x = 0;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            // Draw eight symmetrical points
            self.set_pixel((cx + x) as u32, (cy + y) as u32, r, g, b);
            self.set_pixel((cx - x) as u32, (cy + y) as u32, r, g, b);
            self.set_pixel((cx + x) as u32, (cy - y) as u32, r, g, b);
            self.set_pixel((cx - x) as u32, (cy - y) as u32, r, g, b);
            self.set_pixel((cx + y) as u32, (cy + x) as u32, r, g, b);
            self.set_pixel((cx - y) as u32, (cy + x) as u32, r, g, b);
            self.set_pixel((cx + y) as u32, (cy - x) as u32, r, g, b);
            self.set_pixel((cx - y) as u32, (cy - x) as u32, r, g, b);

            x += 1;
            if d < 0 {
                d += 2 * x + 1;
            } else {
                y -= 1;
                d += 2 * (x - y) + 1;
            }
        }
    }

    /// Draw text at a position (delegates to the 8x8 bitmap font).
    pub fn draw_text(&mut self, x: u32, y: u32, text: &str, r: u8, g: u8, b: u8) {
        self.fb.draw_str(x, y, text, r, g, b);
    }

    /// Draw text with a background fill behind each character.
    pub fn draw_text_with_bg(
        &mut self,
        x: u32,
        y: u32,
        text: &str,
        fg_r: u8,
        fg_g: u8,
        fg_b: u8,
        bg_r: u8,
        bg_g: u8,
        bg_b: u8,
    ) {
        // Draw background rectangle for each character (8w x 9h per cell)
        let _char_w = 8u32;
        let char_h = 9u32;
        // Simple approach: fill one big rect for the whole string
        let total_w = (text.len() as u32) * 9; // 9px per char advance
        self.fb.fill_rect(x, y, total_w, char_h, bg_r, bg_g, bg_b);
        // Draw the text on top
        self.fb.draw_str(x, y, text, fg_r, fg_g, fg_b);
    }

    /// Scroll content up by the given number of pixel lines.
    /// The newly-exposed bottom area is filled with the background color.
    pub fn scroll_up(&mut self, lines: u32, bg_r: u8, bg_g: u8, bg_b: u8) {
        if lines == 0 {
            return;
        }
        if lines >= self.fb.height {
            self.clear(bg_r, bg_g, bg_b);
            return;
        }

        let bytes_per_row = self.fb.pitch as usize;
        let copy_rows = (self.fb.height - lines) as usize;

        unsafe {
            core::ptr::copy(
                self.fb.base.add(lines as usize * bytes_per_row),
                self.fb.base,
                copy_rows * bytes_per_row,
            );
        }

        // Fill exposed bottom rows with background
        let bottom_y = self.fb.height - lines;
        self.fb
            .fill_rect(0, bottom_y, self.fb.width, lines, bg_r, bg_g, bg_b);
    }
}
