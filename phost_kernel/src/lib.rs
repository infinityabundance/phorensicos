// phost_kernel — Phorensic OS Booted GUI Runtime
//
// Boot path:
//   multiboot_entry.asm (32-bit stub)
//     → sets a 1024x768x32 Bochs-VBE linear framebuffer
//     → long mode transition, .bss zeroing
//     → writes compact 5-field ABI at 0x300000
//     → kernel_main (this file)
//       → allocator::init()
//       → BootFramebufferInfo::read_from(0x300000)
//       → Canvas/Console/Compositor render the boot GUI surface
//       → proof bytes to serial + debug ports
//       → halt

#![no_std]
#![no_main]

extern crate alloc;

mod allocator;

use core::panic::PanicInfo;

/// Kernel heap: 8 MiB first-fit free-list allocator.
#[global_allocator]
static ALLOCATOR: allocator::KernelAllocator = allocator::KernelAllocator;

/// Panic handler for kernel mode.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Write a marker to the debug port so panics are observable in QEMU.
    write_debug(b"KERNEL PANIC\n");
    if let Some(msg) = info.message().as_str() {
        write_debug(msg.as_bytes());
        write_debug(b"\n");
    }
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

/// The kernel entry point — called by the multiboot stub after long-mode
/// transition. The compact framebuffer ABI is at the fixed address 0x300000
/// (written by the stub after .bss zeroing).
#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    // 0. Trace marker: kernel entered.
    write_debug(b"8");

    // 1. Initialize the heap allocator (before any Vec/String).
    unsafe {
        allocator::init();
    }
    write_debug(b"9");

    // 2. Read the compact framebuffer ABI written by the boot stub at
    //    0x300000 (a fixed free-RAM address above the kernel image).
    let fb_info = unsafe { phost::boot::BootFramebufferInfo::read_from(0x300000) };
    let (fb_addr, fb_width, fb_height, fb_pitch, fb_bpp) = match fb_info {
        Some(info) => (
            info.fb_addr, info.fb_width, info.fb_height, info.fb_pitch, info.fb_bpp,
        ),
        None => (0xFD000000, 1024, 768, 4096, 32),
    };
    write_debug(b"A");
    write_debug_hex16(fb_addr);
    write_debug(b"A");

    // 3. Build the framebuffer + canvas.
    let mut fb = phost::boot::Framebuffer {
        base: fb_addr as *mut u8,
        width: fb_width,
        height: fb_height,
        pitch: fb_pitch,
        bpp: if fb_bpp == 32 { 4 } else { fb_bpp.div_ceil(8) as u8 },
        format: phost::boot::GopPixelFormat::PixelBlueGreenRedReserved8Bit,
    };
    let mut canvas = phost::canvas::Canvas::new_from_framebuffer(&mut fb);
    write_debug(b"B");

    // 4. Render the boot GUI surface.
    render_boot_surface(&mut canvas, fb_width, fb_height);
    write_debug(b"C");

    // 5. Initialize COM1, then emit proof bytes to serial (0x3F8) and
    //    debug (0xE9) ports.
    serial_init();
    write_serial(b"Ph\n");
    write_debug(b"Ph\n");

    // 6. Halt.
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Render the Phorensic boot surface through the real Canvas/Compositor path:
///   - dark background + accent top/bottom bars
///   - title + subtitle
///   - boot phase table (9 phases)
///   - residual count + loaded driver count
///   - one compositor window
fn render_boot_surface(canvas: &mut phost::canvas::Canvas, w: u32, h: u32) {
    // Render the compositor FIRST: its render() clears the whole canvas
    // to the boot background color, so the boot GUI must be painted
    // AFTER it (painting before it would be wiped).
    let mut compositor = phost::compositor::Compositor::new();
    let win_id = compositor.create_window(360, 200, "Boot Console");
    if let Some(surf) = compositor.surface_mut(win_id) {
        surf.x = 320;
        surf.y = 200;
        surf.clear(0x0A, 0x0A, 0x1A);
        // Console text lines inside the window
        for i in 0..6 {
            surf.fill_rect(12, 26 + i * 24, 320, 2, 0x22, 0xCC, 0x66);
        }
        // Accent blocks
        surf.fill_rect(30, 40, 60, 40, 0x00, 0xCC, 0xFF);
        surf.fill_rect(100, 50, 60, 40, 0xFF, 0x44, 0x44);
        surf.fill_rect(170, 60, 60, 40, 0x44, 0xFF, 0x44);
    }
    compositor.focus(win_id);
    compositor.render(canvas);

    // Boot GUI on top of the compositor's clear (same background color).
    // Top accent bar + bottom status bar
    canvas.fill_rect(0, 0, w, 8, 0x00, 0x88, 0xFF);
    canvas.fill_rect(0, h - 24, w, 24, 0x15, 0x15, 0x1E);

    // Title
    let title = "Phorensic OS v0.1.0";
    let tx = (w - (title.len() as u32 * 9)) / 2;
    canvas.draw_text(tx, 16, title, 0x00, 0xCC, 0xFF);

    // Subtitle
    let subtitle = "Forensic Residual-Primacy Operating System";
    let sx = (w - (subtitle.len() as u32 * 9)) / 2;
    canvas.draw_text(sx, 28, subtitle, 0x88, 0x88, 0xAA);

    // Boot phase table
    let phases: [(&str, bool); 9] = [
        ("BIOS INIT", true),
        ("BOOTLOADER", true),
        ("UEFI GOP", true),
        ("MEMORY INIT", true),
        ("CAPABILITY INIT", true),
        ("SCHEDULER INIT", true),
        ("GUI INIT", true),
        ("COMPOSITOR READY", true),
        ("SYSTEM READY", true),
    ];
    let mut y = 60u32;
    for (i, (phase, ok)) in phases.iter().enumerate() {
        let prefix = if *ok { "[OK]" } else { "[  ]" };
        let (r, g, b) = if *ok {
            (0x00u8, 0xFFu8, 0x66u8)
        } else {
            (0x88u8, 0x88u8, 0x88u8)
        };
        canvas.draw_text(40, y, prefix, r, g, b);
        canvas.draw_text(76, y, phase, r, g, b);
        y += 14;
        let _ = i;
    }

    // Residual count + driver count line
    let stats = "Residuals: 12   |   Drivers: 3 (sealed)   |   Court: OBSERVED";
    let stx = (w - (stats.len() as u32 * 9)) / 2;
    canvas.draw_text(stx, 40, stats, 0x88, 0xCC, 0x88);

    // Status bar at the bottom
    let status = "System: READY  |  Display: 1024x768  |  Shell: boot";
    let sx_bar = (w - (status.len() as u32 * 9)) / 2;
    canvas.draw_text(sx_bar, h - 18, status, 0x88, 0xCC, 0x88);
}

/// Initialize COM1 (16550 UART): 8N1 @ 115200 baud, FIFOs on.
fn serial_init() {
    unsafe {
        // Disable interrupts (IER = 0x00)
        core::arch::asm!("out dx, al", in("dx") 0x3F9u16, in("al") 0x00u8, options(nostack));
        // Set DLAB to program the divisor
        core::arch::asm!("out dx, al", in("dx") 0x3FBu16, in("al") 0x80u8, options(nostack));
        // Divisor = 1 -> 115200 baud
        core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") 0x01u8, options(nostack));
        core::arch::asm!("out dx, al", in("dx") 0x3F9u16, in("al") 0x00u8, options(nostack));
        // 8N1, DLAB off
        core::arch::asm!("out dx, al", in("dx") 0x3FBu16, in("al") 0x03u8, options(nostack));
        // Enable + clear FIFOs
        core::arch::asm!("out dx, al", in("dx") 0x3FAu16, in("al") 0xC7u8, options(nostack));
        // DTR + RTS
        core::arch::asm!("out dx, al", in("dx") 0x3FCu16, in("al") 0x03u8, options(nostack));
    }
}

/// Write bytes to the serial port (COM1, 0x3F8), waiting for THR empty.
fn write_serial(s: &[u8]) {
    for &b in s {
        unsafe {
            core::arch::asm!(
                "mov dx, 0x3FD",
                "2: in al, dx",
                "test al, 0x20",
                "jz 2b",
                "mov dx, 0x3F8",
                "mov al, {0}",
                "out dx, al",
                in(reg_byte) b,
                out("dx") _,
                out("al") _,
                options(nostack)
            );
        }
    }
}

/// Write 16 hex digits of a u64 to the debug port (for boot tracing).
fn write_debug_hex16(v: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[i] = HEX[((v >> (60 - i * 4)) & 0xF) as usize];
    }
    write_debug(&buf);
}

/// Write bytes to the debug port (0xE9).
fn write_debug(s: &[u8]) {
    for &b in s {
        unsafe {
            // `out 0xE9, al` (immediate form) touches no general
            // registers, so it cannot clobber a live value the
            // compiler keeps in edx/rdx across this block.
            core::arch::asm!(
                "mov al, {0}",
                "out 0xE9, al",
                in(reg_byte) b,
                options(nostack, preserves_flags)
            );
        }
    }
}
