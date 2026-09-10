// Phost — Phorensic OS Runtime
// Kernel, boot, framebuffer, compositor, and status screen
//
// This library is `#![no_std]` so that phost_kernel (a no_std kernel runtime)
// can depend on it. Three build modes:
//
//   - default ("std"):     full host runtime — all modules, filesystem/process
//   - "alloc" only:        no_std + allocator — canvas/console/compositor/
//                          nucleus/drivers/status_screen/input/presentation/shell
//   - neither:             minimal no_std — only boot + kernel types
//
// The "alloc" feature pulls in `extern crate alloc` for modules that need
// Vec/String/format (compositor, console, shell, loader, etc.).

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// no_std-safe modules — available always (no allocation)
pub mod boot;
pub mod kernel;

// alloc-gated modules — available when the "alloc" feature is enabled
// (implied by "std", or explicitly for the kernel runtime)
#[cfg(feature = "alloc")]
pub mod canvas;
#[cfg(feature = "alloc")]
pub mod compositor;
#[cfg(feature = "alloc")]
pub mod console;
#[cfg(feature = "alloc")]
pub mod drivers;
#[cfg(feature = "alloc")]
pub mod input;
#[cfg(feature = "alloc")]
pub mod nucleus;
#[cfg(feature = "alloc")]
pub mod phor_compositor;
#[cfg(feature = "alloc")]
pub mod presentation;
#[cfg(feature = "alloc")]
pub mod shell;
#[cfg(feature = "alloc")]
pub mod status_screen;

// std-gated modules — require filesystem/process (only with full "std")
#[cfg(feature = "std")]
pub mod loader;
#[cfg(feature = "std")]
pub mod phorc_bridge;
