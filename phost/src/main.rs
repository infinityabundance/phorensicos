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
    // JIT-Porting Court CLI:
    //   phost port <observe|replay|promote|court> <symbol> [--out DIR]
    // Handled before the boot path so it runs on a plain host process.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "port" {
        std::process::exit(port_cli(&args[2..]));
    }

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

/// JIT-Porting Court CLI.
///
/// Usage: `phost port <observe|replay|promote|court> <symbol> [--out DIR]`
///
/// `observe` seals oracle traces, `replay` adds the candidate/replay residuals,
/// and `promote` (alias `court`) writes the full evidence set and seals the
/// native candidate — but only if every replayed case matched exactly.
fn port_cli(args: &[String]) -> i32 {
    use phost::porting::{self, PortDepth, PortingAuthority};

    if args.is_empty() {
        eprintln!(
            "Usage: phost port <observe|replay|promote|court> <symbol> [--out DIR] [--phorc PATH]"
        );
        return 2;
    }

    let stage = args[0].as_str();
    let depth = match PortDepth::parse(stage) {
        Some(d) => d,
        None => {
            eprintln!(
                "unknown port stage: {} (expected observe|replay|promote|court)",
                stage
            );
            return 2;
        }
    };

    let symbol = args.get(1).map(|s| s.as_str()).unwrap_or("toupper");

    let mut out = format!("phost/evidence/porting/{}", symbol);
    let mut phorc: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--out" if i + 1 < args.len() => {
                out = args[i + 1].clone();
                i += 2;
            }
            "--phorc" if i + 1 < args.len() => {
                phorc = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }

    // A granted PORTING capability: there is no ambient authority, and the
    // court refuses both observation and promotion without it.
    let auth = PortingAuthority::granted();

    match porting::run_port_court(symbol, &auth, &out, depth, phorc.as_deref()) {
        Ok(r) => {
            println!("=== JIT-Porting Court: {} ===", r.symbol);
            println!("Target id:      {}", r.target);
            println!(
                "Dialect:        {} ({}, locale {})",
                r.dialect, r.version, r.locale_contract
            );
            println!("Observed cases: {}", r.observed_cases);
            println!("Replay cases:   {}", r.replay_cases);
            println!("Passed:         {}", r.passed);
            println!("Failed:         {}", r.failed);
            println!("Oracle hash:    {}", r.oracle_hash);
            println!("Candidate behavior hash: {}", r.candidate_behavior_hash);
            println!("Candidate source hash:   {}", r.candidate_source_hash);
            println!("Candidate object hash:   {}", r.candidate_object_hash);
            println!("Candidate receipt hash:  {}", r.candidate_receipt_hash);
            println!("Compiler:                {}", r.compiler_version);
            if !r.candidate_object_path.is_empty() {
                println!("Compiled object:         {}", r.candidate_object_path);
            }
            println!("Verdict:        {}", r.verdict);
            println!("Promotion:      {}", r.promotion);
            println!("Evidence:       {}", r.evidence_dir);
            if !r.sealed_package.is_empty() {
                println!("Sealed package: {}", r.sealed_package);
            }
            0
        }
        Err(e) => {
            eprintln!("port {} {}: {}", stage, symbol, e);
            1
        }
    }
}
