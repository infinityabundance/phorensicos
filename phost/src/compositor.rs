// compositor.rs — Surface/Window Manager and Compositor Bridge
// Manages a stack of windows/surfaces rendered on the framebuffer canvas

use crate::canvas::Canvas;
use alloc::vec;
use alloc::vec::Vec;

/// Maximum number of surfaces
const MAX_SURFACES: usize = 64;

/// Surface type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceType {
    Desktop,
    Window,
    Overlay,
    Cursor,
    Popup,
}

/// A surface (window) managed by the compositor
#[derive(Debug, Clone)]
pub struct Surface {
    pub id: u64,
    pub surface_type: SurfaceType,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: [u8; 64],
    pub title_len: usize,
    pub visible: bool,
    pub focused: bool,
    pub z_order: u64,
    /// Pixel buffer (RGBA), heap-allocated and sized to width*height*4 at
    /// window creation. Kept on the heap so a fixed `[Option<Surface>; 64]`
    /// compositor stays a few KB of stack instead of tens of MB.
    pub backing: Vec<u8>,
    pub dirty: bool,
}

impl Surface {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            surface_type: SurfaceType::Window,
            x: 50,
            y: 50,
            width: 400,
            height: 300,
            title: [0u8; 64],
            title_len: 0,
            visible: true,
            focused: false,
            z_order: 0,
            backing: Vec::new(), // sized by create_window() once width/height are known
            dirty: true,
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.title_len = title.len().min(63);
        self.title[..self.title_len].copy_from_slice(&title.as_bytes()[..self.title_len]);
    }

    pub fn title_str(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len]).unwrap_or("")
    }

    /// Set a pixel in the surface backing store
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 3 < self.backing.len() {
            self.backing[idx] = r;
            self.backing[idx + 1] = g;
            self.backing[idx + 2] = b;
            self.backing[idx + 3] = 255;
            self.dirty = true;
        }
    }

    /// Fill a rectangle in the backing store
    pub fn fill_rect(&mut self, rx: u32, ry: u32, rw: u32, rh: u32, r: u8, g: u8, b: u8) {
        let x_end = (rx + rw).min(self.width);
        let y_end = (ry + rh).min(self.height);
        for y in ry..y_end {
            for x in rx..x_end {
                self.set_pixel(x, y, r, g, b);
            }
        }
    }

    /// Clear the surface to a color
    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        self.backing.fill(0);
        self.fill_rect(0, 0, self.width, self.height, r, g, b);
    }

    /// Draw a filled circle (simplified) in the backing store
    pub fn draw_circle_internal(&mut self, cx: u32, cy: u32, radius: u32, r: u8, g: u8, b: u8) {
        let mut y: i32 = -(radius as i32);
        while y <= (radius as i32) {
            let mut x: i32 = -(radius as i32);
            while x <= (radius as i32) {
                if x * x + y * y <= (radius * radius) as i32 {
                    let px = (cx as i32 + x) as u32;
                    let py = (cy as i32 + y) as u32;
                    self.set_pixel(px, py, r, g, b);
                }
                x += 1;
            }
            y += 1;
        }
    }
}

/// A rectangular damage region
#[derive(Debug, Clone, Copy)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The compositor manages all surfaces and renders them to a Canvas
pub struct Compositor {
    surfaces: [Option<Surface>; 64],
    surface_count: usize,
    next_id: u64,
    focused_id: u64,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            surfaces: core::array::from_fn(|_| None),
            surface_count: 0,
            next_id: 1,
            focused_id: 0,
        }
    }

    /// Create a new window surface. Returns the surface ID.
    pub fn create_window(&mut self, width: u32, height: u32, title: &str) -> u64 {
        if self.surface_count >= MAX_SURFACES {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;

        let mut surface = Surface::new(id);
        surface.width = width.min(512);
        surface.height = height.min(512);
        // Size the backing store to the real window dimensions (RGBA).
        surface.backing = vec![0u8; (surface.width as usize * surface.height as usize) * 4];
        surface.set_title(title);
        surface.z_order = self.surface_count as u64;

        // Find empty slot
        for slot in self.surfaces.iter_mut() {
            if slot.is_none() {
                *slot = Some(surface);
                break;
            }
        }

        self.surface_count += 1;
        self.focused_id = id;
        id
    }

    /// Get a mutable reference to a surface by ID
    pub fn surface_mut(&mut self, id: u64) -> Option<&mut Surface> {
        for slot in self.surfaces.iter_mut() {
            if let Some(surf) = slot {
                if surf.id == id {
                    return Some(surf);
                }
            }
        }
        None
    }

    /// Remove a surface
    pub fn destroy_surface(&mut self, id: u64) -> bool {
        for slot in self.surfaces.iter_mut() {
            if let Some(ref surf) = slot {
                if surf.id == id {
                    *slot = None;
                    self.surface_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    /// Focus a surface
    pub fn focus(&mut self, id: u64) {
        self.focused_id = id;
        if let Some(surf) = self.surface_mut(id) {
            surf.focused = true;
        }
    }

    /// Render all surfaces to a Canvas
    pub fn render(&mut self, canvas: &mut Canvas) {
        // Clear canvas
        canvas.clear(0x0A, 0x0A, 0x0E);

        // Draw surfaces in z-order
        let mut sorted: Vec<u64> = self
            .surfaces
            .iter()
            .filter_map(|s| s.as_ref().map(|s| (s.z_order, s.id)))
            .map(|(_, id)| id)
            .collect();
        sorted.sort();

        for id in sorted {
            if let Some(surf) = self.surface_mut(id) {
                if !surf.visible {
                    continue;
                }

                // Draw the surface backing onto the canvas
                let sx = surf.x.max(0) as u32;
                let sy = surf.y.max(0) as u32;
                let cw = canvas.width();
                let ch = canvas.height();

                for dy in 0..surf.height {
                    if sy + dy >= ch {
                        break;
                    }
                    for dx in 0..surf.width {
                        if sx + dx >= cw {
                            break;
                        }
                        let idx = ((dy * surf.width + dx) * 4) as usize;
                        if idx + 2 < surf.backing.len() {
                            let r = surf.backing[idx];
                            let g = surf.backing[idx + 1];
                            let b = surf.backing[idx + 2];
                            // Skip fully transparent pixels
                            if r > 0 || g > 0 || b > 0 {
                                canvas.set_pixel(sx + dx, sy + dy, r, g, b);
                            }
                        }
                    }
                }

                // Draw title bar
                let title_bar_h = 20u32;
                canvas.fill_rect(sx, sy, surf.width, title_bar_h, 0x00, 0x66, 0xCC);
                if surf.focused {
                    canvas.fill_rect(sx, sy, surf.width, title_bar_h, 0x00, 0x88, 0xFF);
                }

                // Draw title text
                let title_str = surf.title_str();
                canvas.draw_text(sx + 4, sy + 4, title_str, 0xFF, 0xFF, 0xFF);

                // Draw border
                let border_bright = if surf.focused { 0xFFu8 } else { 0x88u8 };
                canvas.draw_rect(
                    sx,
                    sy,
                    surf.width,
                    surf.height,
                    border_bright,
                    border_bright,
                    border_bright,
                );
            }
        }
    }

    /// Get surface count
    pub fn surface_count(&self) -> usize {
        self.surface_count
    }

    /// Route a keyboard character to the focused window's virtual input buffer.
    /// Returns true if the key was consumed.
    pub fn route_keyboard_input(&mut self, _c: char) -> bool {
        let focused_id = self.focused_id;
        if focused_id == 0 {
            return false;
        }
        if let Some(surf) = self.surface_mut(focused_id) {
            // Simulate latency — we just toggle dirty state as visual feedback
            surf.dirty = true;
            // In a real system, this would push to the window's input event queue
            true
        } else {
            false
        }
    }

    /// Set the focused surface by ID. Returns false if not found.
    pub fn set_focused(&mut self, id: u64) -> bool {
        if self.surface_mut(id).is_some() {
            // Unfocus all
            for i in 0..64 {
                if let Some(ref mut s) = self.surfaces[i] {
                    s.focused = s.id == id;
                }
            }
            self.focused_id = id;
            true
        } else {
            false
        }
    }

    /// Cycle focus to the next window in the z-order
    pub fn cycle_focus(&mut self) {
        if self.surface_count == 0 {
            return;
        }
        // Find current focused index
        let current_idx = self
            .surfaces
            .iter()
            .position(|s| s.as_ref().map_or(false, |s| s.id == self.focused_id));
        // Find next window after current
        let start = current_idx.map(|i| i + 1).unwrap_or(0);
        for i in start..64 {
            if let Some(ref s) = self.surfaces[i] {
                if s.visible && s.id != self.focused_id {
                    self.set_focused(s.id);
                    return;
                }
            }
        }
        // Wrap around
        for i in 0..start {
            if let Some(ref s) = self.surfaces[i] {
                if s.visible {
                    self.set_focused(s.id);
                    return;
                }
            }
        }
    }

    /// Track damage for a surface
    pub fn mark_damaged(&mut self, surface_id: u64) {
        if let Some(surf) = self.surface_mut(surface_id) {
            surf.dirty = true;
        }
    }

    /// Render only damaged regions using z-order compositing.
    /// Collects damage rects from dirty surfaces, then renders via
    /// render_zordered() which only redraws surfaces intersecting damage.
    pub fn render_damaged(&mut self, canvas: &mut Canvas) -> Vec<DamageRect> {
        let mut damage_regions = Vec::new();
        for i in 0..64 {
            if let Some(ref mut surf) = self.surfaces[i] {
                if surf.dirty && surf.visible {
                    damage_regions.push(DamageRect {
                        x: surf.x.max(0) as u32,
                        y: surf.y.max(0) as u32,
                        width: surf.width,
                        height: surf.height,
                    });
                }
            }
        }
        // Use z-order compositing with damage regions instead of full redraw
        self.render_zordered(canvas, &damage_regions);
        // Clear dirty flags
        for i in 0..64 {
            if let Some(ref mut surf) = self.surfaces[i] {
                surf.dirty = false;
            }
        }
        damage_regions
    }

    /// Render surfaces with proper z-order compositing.
    /// Surfaces with higher z_order are rendered on top.
    /// Damage tracking optimizes what gets redrawn.
    pub fn render_zordered(&mut self, canvas: &mut Canvas, damage: &[DamageRect]) {
        if damage.is_empty() {
            return;
        }

        // Collect all surfaces sorted by z_order
        let mut sorted: Vec<(u64, u64)> = Vec::new(); // (z_order, surface_id)
        for i in 0..64 {
            if let Some(ref s) = self.surfaces[i] {
                if s.visible {
                    sorted.push((s.z_order, s.id));
                }
            }
        }
        sorted.sort_by_key(|k| k.0); // Ascending z_order

        // Clear damaged regions
        for rect in damage {
            canvas.fill_rect(rect.x, rect.y, rect.width, rect.height, 0x0A, 0x0A, 0x0E);
        }

        // Render surfaces in z_order
        for (_, id) in &sorted {
            if let Some(surf) = self.surface_mut(*id) {
                if !surf.visible {
                    continue;
                }

                let sx = surf.x.max(0) as u32;
                let sy = surf.y.max(0) as u32;

                // Check if this surface intersects any damaged region
                for rect in damage {
                    // Simple AABB intersection
                    if sx < rect.x + rect.width
                        && sx + surf.width > rect.x
                        && sy < rect.y + rect.height
                        && sy + surf.height > rect.y
                    {
                        // This surface needs to be redrawn in the damaged area
                        render_surface_to_canvas(surf, canvas);
                        break;
                    }
                }
            }
        }
    }
}

/// Render a single surface to the canvas with its backing store
fn render_surface_to_canvas(surf: &Surface, canvas: &mut Canvas) {
    let sx = surf.x.max(0) as u32;
    let sy = surf.y.max(0) as u32;
    let cw = canvas.width();
    let ch = canvas.height();

    // Draw backing store pixels
    for dy in 0..surf.height {
        if sy + dy >= ch {
            break;
        }
        for dx in 0..surf.width {
            if sx + dx >= cw {
                break;
            }
            let idx = ((dy * surf.width + dx) * 4) as usize;
            if idx + 2 < surf.backing.len() {
                let r = surf.backing[idx];
                let g = surf.backing[idx + 1];
                let b = surf.backing[idx + 2];
                if r > 0 || g > 0 || b > 0 {
                    canvas.set_pixel(sx + dx, sy + dy, r, g, b);
                }
            }
        }
    }

    // Draw title bar
    let title_bar_h = 20u32;
    let title_color = if surf.focused {
        (0x00u8, 0x88u8, 0xFFu8)
    } else {
        (0x00u8, 0x66u8, 0xCCu8)
    };
    canvas.fill_rect(
        sx,
        sy,
        surf.width,
        title_bar_h,
        title_color.0,
        title_color.1,
        title_color.2,
    );

    // Draw title text
    let title_str = surf.title_str();
    canvas.draw_text(sx + 4, sy + 4, title_str, 0xFF, 0xFF, 0xFF);

    // Draw border
    let border = if surf.focused { 0xFFu8 } else { 0x88u8 };
    canvas.draw_rect(sx, sy, surf.width, surf.height, border, border, border);
}
