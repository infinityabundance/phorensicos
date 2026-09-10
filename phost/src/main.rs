// Phost — Phorensic OS Host / Runtime
// Boot path demo: status screen, canvas, console, and shell

use phost::boot;
use phost::canvas;
use phost::compositor::Compositor;
use phost::console;
use phost::drivers::serial;
use phost::loader;
use phost::phorc_bridge;
use phost::shell;
use phost::status_screen;

fn main() {
    // Try to read boot info from the kernel boot page at 0x108000.
    // The ASM stub writes a compact 5-field framebuffer layout there.
    // If no real boot page is present (e.g., running as a test binary), fall back
    // to an empty boot info struct with simulated dimensions.
    let boot_info = unsafe {
        // First try the compact framebuffer layout (what the boot stub actually writes)
        boot::BootInfo::from_boot_framebuffer_page(0x108000)
            // Then try the full BootInfo layout (with 0xAA55 signature)
            .or_else(|| boot::BootInfo::from_boot_page(0x108000))
            .unwrap_or_else(|| {
                let mut info = boot::BootInfo::empty();
                // Provide default framebuffer dimensions for testing/demo
                info.framebuffer_width = 800;
                info.framebuffer_height = 600;
                info.framebuffer_pitch = 800 * 4;
                info.framebuffer_addr = 0x1000;
                info
            })
    };

    // Initialize status screen
    let mut status = status_screen::StatusScreen::new(&boot_info);

    if let Some(ref mut screen) = status {
        screen.init();
        screen.set_phase(status_screen::BootPhase::BiosInit);
        screen.log("BIOS initialization complete", false);
        screen.set_phase(status_screen::BootPhase::Bootloader);
        screen.log("Bootloader loaded", false);
        screen.set_phase(status_screen::BootPhase::UefiGop);
        screen.log("UEFI GOP framebuffer initialized", false);

        // Initialize serial
        screen.log("Serial port initializing...", false);
        serial::serial_init();

        // Load drivers from sealed packages
        screen.log("Loading sealed drivers...", false);
        {
            use phost::drivers::capability_driver::{register_standard_drivers, DRIVER_REGISTRY};
            use phost::kernel::CapabilitySet;

            let caps = CapabilitySet {
                bits: CapabilitySet::DRIVER_LOAD
                    | CapabilitySet::CONSOLE
                    | CapabilitySet::IO_PORT
                    | CapabilitySet::FRAMEBUFFER,
            };

            unsafe {
                register_standard_drivers(&mut DRIVER_REGISTRY, caps);
                screen.log(
                    &format!("  Drivers registered: {}", DRIVER_REGISTRY.driver_count()),
                    false,
                );
            }

            // Initialize and activate sealed drivers
            screen.log("Initializing and activating sealed drivers...", false);
            {
                let init_caps = CapabilitySet {
                    bits: CapabilitySet::CONSOLE
                        | CapabilitySet::IO_PORT
                        | CapabilitySet::FRAMEBUFFER
                        | CapabilitySet::IRQ_LINE,
                };
                let active_caps = CapabilitySet {
                    bits: CapabilitySet::CONSOLE
                        | CapabilitySet::IO_PORT
                        | CapabilitySet::FRAMEBUFFER
                        | CapabilitySet::DRIVER_LOAD,
                };
                unsafe {
                    for i in 0..DRIVER_REGISTRY.driver_count() {
                        // First, initialize the driver
                        if let Err(e) = DRIVER_REGISTRY.init(i, init_caps) {
                            screen.log(
                                &format!(
                                    "  Init failed: {} \u{2014} {}",
                                    DRIVER_REGISTRY.get(i).map_or("?", |d| d.name_str()),
                                    e
                                ),
                                true,
                            );
                            continue;
                        }
                        // Then activate with sealed trust check
                        match DRIVER_REGISTRY.activate_sealed(i, active_caps) {
                            Ok(()) => {
                                if let Some(desc) = DRIVER_REGISTRY.get(i) {
                                    screen.log(
                                        &format!(
                                            "  Activated: {} (IRQ {:?}, trust={})",
                                            desc.name_str(),
                                            desc.irq_line(),
                                            desc.trust_level()
                                        ),
                                        false,
                                    );
                                }
                            }
                            Err(e) => {
                                screen.log(
                                    &format!(
                                        "  Activation failed: {} \u{2014} {}",
                                        DRIVER_REGISTRY.get(i).map_or("?", |d| d.name_str()),
                                        e
                                    ),
                                    true,
                                );
                            }
                        }
                    }
                }
            }
        }

        screen.set_phase(status_screen::BootPhase::MemoryInit);
        screen.log("Memory map parsed", false);
        screen.set_phase(status_screen::BootPhase::CapabilityInit);
        screen.log("Capability system initialized", false);
        screen.set_phase(status_screen::BootPhase::SchedulerInit);
        screen.log("Scheduler initialized", false);
        screen.set_phase(status_screen::BootPhase::GuiInit);
        screen.log("GUI subsystem initializing...", false);

        // Initialize compositor bridge and phorc compiler bridge
        let mut compositor = Compositor::new();
        let _bridge_test = phorc_bridge::CompileResult::simulated("init.phor");
        let hello_prog = loader::load_phor_program("hello");
        let _ = hello_prog.call_entry();
        screen.log("Compositor bridge initialized", false);
        screen.log("Phorc compiler bridge ready", false);

        // Demonstrate the phor compositor bridge
        {
            use phost::phor_compositor::{self, PhorCompositor};
            let mut phor_comp = PhorCompositor::new();
            let result =
                phor_compositor::run_phor_program(&mut compositor, &mut phor_comp, "hello");
            screen.log(
                &format!(
                    "Phor compositor bridge: {}, {} surface(s)",
                    result,
                    phor_comp.active_count()
                ),
                false,
            );
        }

        // Demo the canvas and console
        let fb = screen.framebuffer();
        let canvas = canvas::Canvas::new_from_framebuffer(fb);
        let mut console = console::Console::new(canvas, 20, 50, 760, 500);

        console.write_line("Phorensic OS Console initialized.");
        console.write_line("Keyboard input available. Type 'help' for commands.");
        console.write_line("");

        let mut shell = shell::Shell::new(console);
        shell.run();
    } else {
        serial::serial_init();
        serial::serial_write_line("Phorensic OS v0.1.0 \u{2014} Headless boot");
    }
}
