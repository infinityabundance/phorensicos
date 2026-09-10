// presentation.rs — Presentation loop and frame timing
// Provides frame scheduling, receipt generation, and vsync simulation.

use crate::canvas::Canvas;
use crate::compositor::Compositor;

/// Frame timing information
#[derive(Debug, Clone, Copy)]
pub struct FrameTiming {
    pub frame_number: u64,
    pub surface_count: usize,
    pub draw_calls: u64,
    pub render_start: u64,
    pub render_end: u64,
    pub render_us: u64,
}

impl FrameTiming {
    pub fn new(frame: u64) -> Self {
        Self {
            frame_number: frame,
            surface_count: 0,
            draw_calls: 0,
            render_start: 0,
            render_end: 0,
            render_us: 0,
        }
    }
}

/// Presentation loop — renders compositor frames to canvas with timing
pub struct PresentationLoop {
    frame_count: u64,
    timing_history: [FrameTiming; 64],
    history_count: usize,
    vsync_interval_us: u64, // simulated vsync
}

impl PresentationLoop {
    pub fn new(vsync_hz: u64) -> Self {
        Self {
            frame_count: 0,
            timing_history: [FrameTiming::new(0); 64],
            history_count: 0,
            vsync_interval_us: if vsync_hz > 0 {
                1_000_000 / vsync_hz
            } else {
                16667
            }, // ~60Hz default
        }
    }

    /// Render a single frame and return timing info
    pub fn render_frame(
        &mut self,
        compositor: &mut Compositor,
        canvas: &mut Canvas,
    ) -> FrameTiming {
        let frame = self.frame_count;
        self.frame_count += 1;

        let mut timing = FrameTiming::new(frame);
        timing.surface_count = compositor.surface_count();
        timing.render_start = self.read_tick();

        // Clear and render
        compositor.render(canvas);

        timing.render_end = self.read_tick();
        timing.render_us = timing.render_end - timing.render_start;

        // Record in history
        if self.history_count < 64 {
            self.timing_history[self.history_count] = timing;
            self.history_count += 1;
        }

        timing
    }

    /// Get average frame render time in microseconds
    pub fn avg_render_us(&self) -> u64 {
        if self.history_count == 0 {
            return 0;
        }
        let sum: u64 = self.timing_history[..self.history_count]
            .iter()
            .map(|t| t.render_us)
            .sum();
        sum / self.history_count as u64
    }

    /// Get total frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get vsync interval
    pub fn vsync_interval_us(&self) -> u64 {
        self.vsync_interval_us
    }

    /// Read a simulated tick counter (would use real HPET/TSC in kernel)
    fn read_tick(&self) -> u64 {
        // Use a simple counter since we don't have real hardware
        self.frame_count * 16667 / 60 // simulated microsecond tick
    }
}
