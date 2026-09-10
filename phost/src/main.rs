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
        eprintln!(
            "       phost port native <symbol> [ARGS_HEX] [--no-capability] [--out DIR] [--phorc PATH]"
        );
        eprintln!(
            "       phost port compose [--target toupper_memchr|toupper_strlen_memchr|toupper_strlen_memchr_pair|toupper_each|toupper_each_strlen_memchr] [ARGS_HEX] [--no-capability] [--out DIR] [--phorc PATH]"
        );
        return 2;
    }

    let stage = args[0].as_str();

    // Runtime call-site path: publish the seal, then dispatch one call through
    // the sealed-native dispatcher.
    if stage == "native" {
        return native_cli(&args[1..]);
    }

    // Sealed Composition Dispatch Court: a composed target built from sealed
    // ports, executed through the dispatcher.
    if stage == "compose" {
        return compose_cli(&args[1..]);
    }
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
            if r.execution_cases > 0 {
                println!(
                    "Execution:      {} ({} cases, {} passed, {} failed)",
                    r.execution_verdict, r.execution_cases, r.execution_passed, r.execution_failed
                );
                println!("ABI symbol:     {} ({})", r.abi_symbol, r.elf_symbol);
                println!("Execution hash: {}", r.execution_hash);
            }
            if r.dispatch_cases > 0 {
                println!(
                    "Dispatch:       {} ({} cases, {} native, {} fallback, {} broken-seal)",
                    r.dispatch_verdict,
                    r.dispatch_cases,
                    r.dispatch_native_cases,
                    r.dispatch_fallback_cases,
                    r.dispatch_broken_seal_cases
                );
                println!("Dispatch hash:  {}", r.dispatch_hash);
            }
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

/// `phost port native <symbol> [ARGS_HEX] [--no-capability] [--out DIR] [--phorc PATH]`
///
/// The runtime call-site path: publish/refresh the seal, then dispatch **one**
/// call through the sealed-native dispatcher. With `--no-capability` the call
/// site has no `PORTING` authority, so the dispatcher reports a foreign fallback
/// instead of running the native artifact.
fn native_cli(args: &[String]) -> i32 {
    use phost::porting::{self, PortingAuthority};
    if args.is_empty() {
        eprintln!("Usage: phost port native <symbol> [ARGS_HEX] [--no-capability] [--out DIR] [--phorc PATH]");
        return 2;
    }

    let symbol = args[0].as_str();
    let mut framed: Option<String> = None;
    let mut out = format!("phost/evidence/porting/{}", symbol);
    let mut phorc: Option<String> = None;
    let mut dispatch_auth = PortingAuthority::granted();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                dispatch_auth = PortingAuthority::none();
                i += 1;
            }
            "--out" if i + 1 < args.len() => {
                out = args[i + 1].clone();
                i += 2;
            }
            "--phorc" if i + 1 < args.len() => {
                phorc = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                // Remaining positional: the `:`-joined hex argument framing.
                framed = Some(match framed {
                    Some(prev) => format!("{}:{}", prev, other),
                    None => other.to_string(),
                });
                i += 1;
            }
        }
    }

    let court_auth = PortingAuthority::granted();
    match porting::run_native_call(
        symbol,
        framed.as_deref(),
        &court_auth,
        &dispatch_auth,
        &out,
        phorc.as_deref(),
    ) {
        Ok(r) => {
            println!("=== Sealed Native Dispatch: {} ===", symbol);
            println!("Target:        {}", r.target);
            println!("Source:        {}", r.source);
            if r.source == "sealed-object" {
                println!("Trust:         {}", r.trust);
                println!("Object hash:   {}", r.object_hash);
                println!("ELF symbol:    {}", r.elf_symbol);
            } else {
                println!("Reason:        {}", r.reason);
            }
            println!("Input:         {}", r.input_hex);
            println!("Output:        {}", r.output_hex);
            if r.source == "sealed-object" {
                println!(
                    "Matches mirror: {}",
                    if r.matches_mirror { "yes" } else { "no" }
                );
            }
            println!("Sealed package: {}", r.sealed_package);
            0
        }
        Err(e) => {
            eprintln!("port native {}: {}", symbol, e);
            1
        }
    }
}

/// `phost port compose [HAY_HEX:NEEDLE_HEX:N_HEX] [--no-capability] [--out DIR] [--phorc PATH]`
///
/// The Sealed Composition Dispatch Court.
///
/// `phost port compose [--target NAME] [HAY_HEX:NEEDLE_HEX:N_HEX] [--no-capability] [--out DIR] [--phorc PATH]`
///
/// With no positional argument it runs the whole composition corpus through the
/// sealed chain and writes the composition evidence; with an argument it runs one
/// composed call and prints each stage. `--target` selects the chain
/// (`toupper_memchr`, `toupper_strlen_memchr`, `toupper_strlen_memchr_pair`,
/// `toupper_each`, or `toupper_each_strlen_memchr`).
fn compose_cli(args: &[String]) -> i32 {
    use phost::porting::{self, CompositionKind, PortingAuthority};

    let mut call: Option<String> = None;
    let mut kind = CompositionKind::ToupperMemchr;
    let mut out: Option<String> = None;
    let mut phorc: Option<String> = None;
    let mut dispatch_auth = PortingAuthority::granted();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                dispatch_auth = PortingAuthority::none();
                i += 1;
            }
            "--target" if i + 1 < args.len() => {
                match CompositionKind::parse(&args[i + 1]) {
                    Some(k) => kind = k,
                    None => {
                        eprintln!(
                            "port compose: unknown --target {} (expected toupper_memchr|toupper_strlen_memchr|toupper_strlen_memchr_pair|toupper_each|toupper_each_strlen_memchr)",
                            args[i + 1]
                        );
                        return 2;
                    }
                }
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out = Some(args[i + 1].clone());
                i += 2;
            }
            "--phorc" if i + 1 < args.len() => {
                phorc = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                call = Some(match call {
                    Some(prev) => format!("{}:{}", prev, other),
                    None => other.to_string(),
                });
                i += 1;
            }
        }
    }
    let out = out.unwrap_or_else(|| kind.evidence_dir().to_string());

    match call {
        None => match porting::run_composition_court(
            kind,
            &PortingAuthority::granted(),
            &out,
            phorc.as_deref(),
        ) {
            Ok(r) => {
                println!("=== Sealed Composition Dispatch Court ===");
                println!("Target:        {}", r.target);
                println!("Stages:        {}", r.stages.join(" -> "));
                println!("Cases:         {}", r.cases_run);
                for s in &r.stage_reports {
                    println!(
                        "{:<22} {} native / {} cases",
                        format!("{}:", s.label),
                        s.native_cases,
                        r.cases_run
                    );
                }
                println!(
                    "Fallback:      {}   Broken seal: {}",
                    r.fallback_cases, r.broken_seal_cases
                );
                println!("Passed:        {}", r.cases_passed);
                println!("Failed:        {}", r.cases_failed);
                println!("Dispatches:    {}", r.dispatches_run);
                // Deduplicate the object lines: a leaf used twice in a chain (e.g.
                // toupper for the haystack and the needle) is one object.
                let mut seen: Vec<(&str, &str)> = Vec::new();
                for s in &r.stage_reports {
                    if s.object_hash.is_empty() {
                        continue;
                    }
                    let key = (s.leaf.as_str(), s.object_hash.as_str());
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    println!("{:<22} {}", format!("{} object:", s.label), s.object_hash);
                }
                println!("Chain hash:    {}", r.chain_hash);
                println!("Oracle hash:   {}", r.oracle_hash);
                println!("Verdict:       {}", r.verdict);
                println!("Sealed:        {}", if r.sealed { "yes" } else { "no" });
                println!("Evidence:      {}", r.evidence_dir);
                0
            }
            Err(e) => {
                eprintln!("port compose: {}", e);
                1
            }
        },
        Some(framed) => {
            let args = match porting::parse_hex_args(&framed) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("port compose: {}", e);
                    return 2;
                }
            };
            let (want, usage) = match kind {
                CompositionKind::ToupperStrlenMemchrPair => {
                    (4usize, "HAY_HEX:NEEDLE_A_HEX:NEEDLE_B_HEX:N_HEX")
                }
                CompositionKind::ToupperEach => (2usize, "BYTES_HEX:N_HEX"),
                _ => (3usize, "HAY_HEX:NEEDLE_HEX:N_HEX"),
            };
            if args.len() < want {
                eprintln!("port compose: expected {}", usage);
                return 2;
            }
            let hay = &args[0];
            let (needle_a, needle_b, n_bytes): (u8, u8, &Vec<u8>) = match kind {
                CompositionKind::ToupperStrlenMemchrPair => (
                    args[1].first().copied().unwrap_or(0),
                    args[2].first().copied().unwrap_or(0),
                    &args[3],
                ),
                CompositionKind::ToupperEach => (0, 0, &args[1]),
                _ => (args[1].first().copied().unwrap_or(0), 0, &args[2]),
            };
            let n = u64::from_le_bytes(match n_bytes.as_slice().try_into() {
                Ok(b) => b,
                Err(_) => {
                    eprintln!("port compose: N must be an 8-byte little-endian length");
                    return 2;
                }
            }) as usize;

            match porting::run_composition_call(
                kind,
                hay,
                needle_a,
                needle_b,
                n,
                &dispatch_auth,
                phorc.as_deref(),
            ) {
                Ok(c) => {
                    println!("=== Sealed Composition Call ===");
                    println!("Target:        {}", c.target);
                    println!("Input:         {}", framed);
                    for (label, status) in &c.stages {
                        println!("{:<22} {}", format!("{}:", label), status);
                    }
                    println!("Haystack norm: {}", hex_encode(&c.hay_norm));
                    println!(
                        "Derived bound: {}",
                        c.derived_len
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| String::from("-"))
                    );
                    for (label, b) in &c.needles {
                        println!(
                            "{:<22} {}",
                            format!("{} norm:", label),
                            b.map(|v| format!("{:02x}", v))
                                .unwrap_or_else(|| String::from("-"))
                        );
                    }
                    for (label, i) in &c.indexes {
                        println!(
                            "{:<22} {}",
                            format!("{}:", label),
                            i.map(|v| v.to_string())
                                .unwrap_or_else(|| String::from("-"))
                        );
                    }
                    println!("Dispatches:    {}", c.dispatches);
                    0
                }
                Err(e) => {
                    eprintln!("port compose: {}", e);
                    1
                }
            }
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
