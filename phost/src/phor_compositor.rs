// phor_compositor.rs — Simulated .phor-to-Compositor bridge
// Provides a draw-command queue that .phor programs can target.
// Runs known programs ("hello", "canvas_demo") by dispatching canned
// draw commands. Real .phor execution would parse compiled ELF output
// and translate PHIR instructions into draw commands at runtime.

use crate::canvas::Canvas;
use crate::compositor::{Compositor, Surface};

/// Maximum number of phor-managed surfaces
const MAX_PHOR_SURFACES: usize = 16;

/// A surface handle returned to .phor code
pub type PhorSurfaceId = u64;

/// Drawing commands that .phor programs can issue
#[derive(Debug, Clone, Copy)]
pub enum DrawCommand {
    Clear {
        r: u8,
        g: u8,
        b: u8,
    },
    SetPixel {
        x: u32,
        y: u32,
        r: u8,
        g: u8,
        b: u8,
    },
    FillRect {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        r: u8,
        g: u8,
        b: u8,
    },
    DrawText {
        x: u32,
        y: u32,
        r: u8,
        g: u8,
        b: u8,
    },
    Present,
}

/// A surface created by a .phor program, tracked by the bridge
pub struct PhorSurface {
    pub surface_id: PhorSurfaceId,
    pub compositor_id: u64, // ID in the Rust Compositor
    pub width: u32,
    pub height: u32,
    pub title: [u8; 64],
    pub title_len: usize,
    pub command_queue: [DrawCommand; 64],
    pub command_count: usize,
}

impl PhorSurface {
    pub fn new(id: PhorSurfaceId, compositor_id: u64, w: u32, h: u32) -> Self {
        Self {
            surface_id: id,
            compositor_id,
            width: w,
            height: h,
            title: [0u8; 64],
            title_len: 0,
            command_queue: [DrawCommand::Clear { r: 0, g: 0, b: 0 }; 64],
            command_count: 0,
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.title_len = title.len().min(63);
        self.title[..self.title_len].copy_from_slice(&title.as_bytes()[..self.title_len]);
    }

    pub fn title_str(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len]).unwrap_or("")
    }

    /// Queue a draw command
    pub fn push_command(&mut self, cmd: DrawCommand) {
        if self.command_count < 64 {
            self.command_queue[self.command_count] = cmd;
            self.command_count += 1;
        }
    }

    /// Execute all queued commands on a Surface
    pub fn execute_commands(&mut self, surface: &mut Surface) {
        for i in 0..self.command_count {
            match self.command_queue[i] {
                DrawCommand::Clear { r, g, b } => {
                    surface.clear(r, g, b);
                }
                DrawCommand::SetPixel { x, y, r, g, b } => {
                    surface.set_pixel(x, y, r, g, b);
                }
                DrawCommand::FillRect {
                    x,
                    y,
                    w,
                    h,
                    r,
                    g,
                    b,
                } => {
                    surface.fill_rect(x, y, w, h, r, g, b);
                }
                DrawCommand::DrawText {
                    x: _,
                    y: _,
                    r: _,
                    g: _,
                    b: _,
                } => {
                    // Text drawing via title bar for now
                }
                DrawCommand::Present => {
                    // Surface is already dirty from pixel ops; mark it
                    surface.dirty = true;
                }
            }
        }
        self.command_count = 0;
    }
}

/// Widget types for the .phor compositor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetType {
    Button,
    Label,
    Panel,
    TextInput,
    ProgressBar,
    Canvas,
}

/// A widget within a phor surface
#[derive(Debug, Clone)]
pub struct Widget {
    pub widget_type: WidgetType,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub label: [u8; 64],
    pub label_len: usize,
    pub bg_r: u8,
    pub bg_g: u8,
    pub bg_b: u8,
    pub fg_r: u8,
    pub fg_g: u8,
    pub fg_b: u8,
    pub clicked: bool,
    pub value: u64,
    pub visible: bool,
}

impl Widget {
    pub fn new_button(x: u32, y: u32, w: u32, h: u32, label: &str) -> Self {
        let mut lbl = [0u8; 64];
        let ll = label.len().min(63);
        lbl[..ll].copy_from_slice(&label.as_bytes()[..ll]);
        Self {
            widget_type: WidgetType::Button,
            x,
            y,
            width: w,
            height: h,
            label: lbl,
            label_len: ll,
            bg_r: 0x44,
            bg_g: 0x88,
            bg_b: 0xCC,
            fg_r: 0xFF,
            fg_g: 0xFF,
            fg_b: 0xFF,
            clicked: false,
            value: 0,
            visible: true,
        }
    }

    pub fn new_label(x: u32, y: u32, text: &str) -> Self {
        let mut lbl = [0u8; 64];
        let ll = text.len().min(63);
        lbl[..ll].copy_from_slice(&text.as_bytes()[..ll]);
        Self {
            widget_type: WidgetType::Label,
            x,
            y,
            width: text.len() as u32 * 9,
            height: 9,
            label: lbl,
            label_len: ll,
            bg_r: 0x0A,
            bg_g: 0x0A,
            bg_b: 0x1A,
            fg_r: 0xCC,
            fg_g: 0xCC,
            fg_b: 0xCC,
            clicked: false,
            value: 0,
            visible: true,
        }
    }

    pub fn new_progress(x: u32, y: u32, w: u32) -> Self {
        Self {
            widget_type: WidgetType::ProgressBar,
            x,
            y,
            width: w,
            height: 12,
            label: [0u8; 64],
            label_len: 0,
            bg_r: 0x30,
            bg_g: 0x30,
            bg_b: 0x40,
            fg_r: 0x00,
            fg_g: 0xCC,
            fg_b: 0x66,
            clicked: false,
            value: 50,
            visible: true,
        }
    }

    pub fn label_str(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len]).unwrap_or("?")
    }
}

/// Extended PhorSurface with widgets
impl PhorSurface {
    /// Render widgets onto the surface using draw commands
    pub fn render_widgets(&mut self, widgets: &[Widget]) {
        for widget in widgets {
            if !widget.visible {
                continue;
            }
            match widget.widget_type {
                WidgetType::Button => {
                    self.push_command(DrawCommand::FillRect {
                        x: widget.x,
                        y: widget.y,
                        w: widget.width,
                        h: widget.height,
                        r: widget.bg_r,
                        g: widget.bg_g,
                        b: widget.bg_b,
                    });
                }
                WidgetType::Label => {
                    // Just mark as present — text rendering handled by compositor
                }
                WidgetType::ProgressBar => {
                    // Background
                    self.push_command(DrawCommand::FillRect {
                        x: widget.x,
                        y: widget.y,
                        w: widget.width,
                        h: widget.height,
                        r: 0x30,
                        g: 0x30,
                        b: 0x40,
                    });
                    // Fill based on value
                    let fill = (widget.width as u64 * widget.value / 100) as u32;
                    if fill > 0 {
                        self.push_command(DrawCommand::FillRect {
                            x: widget.x,
                            y: widget.y,
                            w: fill,
                            h: widget.height,
                            r: 0x00,
                            g: 0xCC,
                            b: 0x66,
                        });
                    }
                }
                _ => {}
            }
        }
        self.push_command(DrawCommand::Present);
    }
}

/// A presentation frame receipt — tracks what was rendered
#[derive(Debug, Clone)]
pub struct PresentationReceipt {
    pub frame_number: u64,
    pub surface_count: u64,
    pub total_widgets: u64,
    pub draw_commands_issued: u64,
    pub timestamp: u64,
}

impl PresentationReceipt {
    pub fn new(frame: u64, surfaces: u64, widgets: u64, cmds: u64) -> Self {
        Self {
            frame_number: frame,
            surface_count: surfaces,
            total_widgets: widgets,
            draw_commands_issued: cmds,
            timestamp: 0,
        }
    }
}

/// The phor-to-compositor bridge
pub struct PhorCompositor {
    surfaces: [Option<PhorSurface>; 16],
    surface_count: usize,
    next_id: PhorSurfaceId,
}

impl PhorCompositor {
    pub fn new() -> Self {
        Self {
            surfaces: Default::default(),
            surface_count: 0,
            next_id: 1,
        }
    }

    /// Create a new surface (called from .phor via FFI)
    pub fn create_surface(
        &mut self,
        compositor: &mut Compositor,
        width: u32,
        height: u32,
        title: &str,
    ) -> PhorSurfaceId {
        if self.surface_count >= MAX_PHOR_SURFACES {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;

        // Create the actual compositor surface
        let comp_id = compositor.create_window(width, height, title);

        let mut phor_surf = PhorSurface::new(id, comp_id, width, height);
        phor_surf.set_title(title);

        // Find empty slot
        for slot in self.surfaces.iter_mut() {
            if slot.is_none() {
                *slot = Some(phor_surf);
                break;
            }
        }
        self.surface_count += 1;
        id
    }

    /// Get a mutable reference to a phor surface
    pub fn surface_mut(&mut self, id: PhorSurfaceId) -> Option<&mut PhorSurface> {
        self.surfaces.iter_mut().find_map(|s| {
            if let Some(ref mut surf) = s {
                if surf.surface_id == id {
                    return Some(surf);
                }
            }
            None
        })
    }

    /// Dispatch queued draw commands to the compositor, then render
    pub fn flush_and_render(&mut self, compositor: &mut Compositor) {
        for i in 0..MAX_PHOR_SURFACES {
            if let Some(ref mut phor_surf) = self.surfaces[i] {
                if let Some(comp_surf) = compositor.surface_mut(phor_surf.compositor_id) {
                    phor_surf.execute_commands(comp_surf);
                }
            }
        }
    }

    /// Count active surfaces
    pub fn active_count(&self) -> usize {
        self.surface_count
    }

    /// Presentation loop: flush all surfaces, render, return receipt
    pub fn present_frame(
        &mut self,
        compositor: &mut Compositor,
        canvas: &mut Canvas,
        frame_num: u64,
    ) -> PresentationReceipt {
        let surface_count = self.active_count() as u64;
        let mut total_cmds = 0u64;
        for i in 0..MAX_PHOR_SURFACES {
            if let Some(ref phor_surf) = self.surfaces[i] {
                total_cmds += phor_surf.command_count as u64;
            }
        }
        self.flush_and_render(compositor);
        compositor.render(canvas);
        PresentationReceipt::new(frame_num, surface_count, 0, total_cmds)
    }
}

/// Simulate running a .phor program that creates a surface and draws to it.
/// Returns a description of what the program did.
pub fn run_phor_program(
    compositor: &mut Compositor,
    phor_comp: &mut PhorCompositor,
    program: &str,
) -> &'static str {
    let result = match program {
        "hello" | "hello.phor" => {
            let sid = phor_comp.create_surface(compositor, 320, 200, "Hello World");
            if let Some(surf) = phor_comp.surface_mut(sid) {
                surf.push_command(DrawCommand::Clear {
                    r: 0x0A,
                    g: 0x0A,
                    b: 0x1A,
                });
                surf.push_command(DrawCommand::FillRect {
                    x: 50,
                    y: 50,
                    w: 80,
                    h: 60,
                    r: 0x00,
                    g: 0xCC,
                    b: 0xFF,
                });
                surf.push_command(DrawCommand::FillRect {
                    x: 180,
                    y: 50,
                    w: 80,
                    h: 60,
                    r: 0xFF,
                    g: 0x44,
                    b: 0x44,
                });
                surf.push_command(DrawCommand::Present);
            }
            "Hello program drew 2 rectangles"
        }
        "canvas_demo" | "canvas_demo.phor" => {
            let sid = phor_comp.create_surface(compositor, 400, 300, "Canvas Demo");
            if let Some(surf) = phor_comp.surface_mut(sid) {
                surf.push_command(DrawCommand::Clear {
                    r: 0x0A,
                    g: 0x0A,
                    b: 0x0E,
                });
                surf.push_command(DrawCommand::FillRect {
                    x: 30,
                    y: 40,
                    w: 100,
                    h: 80,
                    r: 0xFF,
                    g: 0x44,
                    b: 0x44,
                });
                surf.push_command(DrawCommand::FillRect {
                    x: 50,
                    y: 60,
                    w: 100,
                    h: 80,
                    r: 0x44,
                    g: 0xFF,
                    b: 0x44,
                });
                surf.push_command(DrawCommand::FillRect {
                    x: 200,
                    y: 100,
                    w: 100,
                    h: 80,
                    r: 0x44,
                    g: 0x44,
                    b: 0xFF,
                });
                surf.push_command(DrawCommand::Present);
            }
            "Canvas demo drew 3 colored rectangles"
        }
        _ => {
            let sid = phor_comp.create_surface(compositor, 200, 100, program);
            if let Some(surf) = phor_comp.surface_mut(sid) {
                surf.push_command(DrawCommand::Clear {
                    r: 0x1A,
                    g: 0x00,
                    b: 0x00,
                });
                surf.push_command(DrawCommand::Present);
            }
            "Unknown program displayed placeholder"
        }
    };
    phor_comp.flush_and_render(compositor);
    result
}
