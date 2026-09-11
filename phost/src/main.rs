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
        eprintln!("       phost port native <symbol> [ARGS_HEX] [--no-capability] [--store PATH]");
        eprintln!(
            "       phost port compose [--target toupper_memchr|toupper_strlen_memchr|toupper_strlen_memchr_pair|toupper_each|toupper_each_strlen_memchr|toupper_memchr_suffix|toupper_each_slice_search] [ARGS_HEX] [--no-capability] [--store|--derive] [--ir] [--out DIR] [--phorc PATH]"
        );
        eprintln!("       phost port store [--check|--write] [PATH]");
        eprintln!("       phost port session [--out DIR] [--no-capability]");
        eprintln!("       phost port cross <symbol> [--out DIR] [--probe PATH]");
        return 2;
    }

    let stage = args[0].as_str();

    // The cross-implementation court: the same sealed corpus through a second,
    // independent implementation.
    if stage == "cross" {
        return cross_cli(&args[1..]);
    }

    // The long-lived sealed native service: one verified store load, many consumers.
    if stage == "session" {
        return session_cli(&args[1..]);
    }

    // The persistent sealed port store: load and verify the committed index, or
    // regenerate it from committed evidence.
    if stage == "store" {
        return store_cli(&args[1..]);
    }

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

/// `phost port native <symbol> [ARGS_HEX] [--no-capability] [--store PATH]`
///
/// The runtime call-site path: **load** the committed persistent store (no
/// compiler invocation, no oracle replay), then dispatch **one** call through the
/// sealed-native dispatcher. With `--no-capability` the call site has no `PORTING`
/// authority, so the store is not read and the dispatcher reports a foreign
/// fallback instead of running the native artifact.
fn native_cli(args: &[String]) -> i32 {
    use phost::porting::{self, store, PortingAuthority};
    if args.is_empty() {
        eprintln!("Usage: phost port native <symbol> [ARGS_HEX] [--no-capability] [--store PATH]");
        return 2;
    }

    let symbol = args[0].as_str();
    let mut framed: Option<String> = None;
    let mut store_path = store::STORE_PATH.to_string();
    let mut dispatch_auth = PortingAuthority::granted();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                dispatch_auth = PortingAuthority::none();
                i += 1;
            }
            "--store" if i + 1 < args.len() => {
                store_path = args[i + 1].clone();
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

    match porting::run_native_call(symbol, framed.as_deref(), &store_path, &dispatch_auth) {
        Ok(r) => {
            println!("=== Sealed Native Dispatch: {} ===", symbol);
            println!("Target:        {}", r.target);
            println!("Source:        {}", r.source);
            println!("Store:         {}", store_path);
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
            println!("Promotion:     {}", r.promotion);
            if !r.sealed_package.is_empty() {
                println!("Sealed package: {}", r.sealed_package);
            }
            0
        }
        Err(e) => {
            eprintln!("port native {}: {}", symbol, e);
            1
        }
    }
}

/// `phost port compose [--target NAME] [ARGS_HEX] [--no-capability] [--store|--derive] [--out DIR] [--phorc PATH]`
///
/// The Sealed Composition Dispatch Court.
///
/// With no positional argument it runs the whole composition corpus through the
/// sealed chain and writes the composition evidence; with an argument it runs one
/// composed call and prints each stage. `--target` selects the chain
/// (`toupper_memchr`, `toupper_strlen_memchr`, `toupper_strlen_memchr_pair`,
/// `toupper_each`, `toupper_each_strlen_memchr`, `toupper_memchr_suffix`, or
/// `toupper_each_slice_search`).
///
/// The court **derives** the seal by default (it compiles the leaves and replays
/// nested chains); `--store` makes it load the seal from the committed persistent
/// store instead, so the same verdict must reproduce with no compiler at all. The
/// single composed **call** always runs from the store: that is the runtime path.
fn compose_cli(args: &[String]) -> i32 {
    use phost::porting::{self, CompositionKind, IndexSource, PortingAuthority};

    let mut call: Option<String> = None;
    let mut kind = CompositionKind::ToupperMemchr;
    let mut out: Option<String> = None;
    let mut phorc: Option<String> = None;
    let mut index_source = IndexSource::Derived;
    let mut dispatch_auth = PortingAuthority::granted();
    // `--ir` selects the Phase 2 generic court (the composition is data, evaluated
    // by one interpreter); the default is the historical v1 court.
    let mut ir = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                dispatch_auth = PortingAuthority::none();
                i += 1;
            }
            "--ir" => {
                ir = true;
                i += 1;
            }
            "--store" => {
                index_source = IndexSource::Persistent;
                i += 1;
            }
            "--derive" => {
                index_source = IndexSource::Derived;
                i += 1;
            }
            "--target" if i + 1 < args.len() => {
                match CompositionKind::parse(&args[i + 1]) {
                    Some(k) => kind = k,
                    None => {
                        eprintln!(
                            "port compose: unknown --target {} (expected toupper_memchr|toupper_strlen_memchr|toupper_strlen_memchr_pair|toupper_each|toupper_each_strlen_memchr|toupper_memchr_suffix|toupper_each_slice_search)",
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
    let out = out.unwrap_or_else(|| {
        if ir {
            porting::ir_evidence_dir(kind.name())
        } else {
            kind.evidence_dir().to_string()
        }
    });

    match call {
        None => {
            let result = if ir {
                porting::run_ir_composition_court(
                    kind.name(),
                    &PortingAuthority::granted(),
                    &out,
                    phorc.as_deref(),
                    index_source,
                )
            } else {
                porting::run_composition_court(
                    kind,
                    &PortingAuthority::granted(),
                    &out,
                    phorc.as_deref(),
                    index_source,
                )
            };
            match result {
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
                    for note in &r.notes {
                        println!("Note:          {}", note);
                    }
                    println!(
                        "Index source:  {}",
                        match index_source {
                            IndexSource::Derived => "derived (compiled fresh)",
                            IndexSource::Persistent => "persistent store",
                        }
                    );
                    println!("Evidence:      {}", r.evidence_dir);
                    0
                }
                Err(e) => {
                    eprintln!("port compose: {}", e);
                    1
                }
            }
        }
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
                CompositionKind::ToupperMemchrSuffix => {
                    (4usize, "HAY_HEX:NEEDLE_A_HEX:NEEDLE_B_HEX:N_HEX")
                }
                CompositionKind::ToupperEachSliceSearch => {
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
                CompositionKind::ToupperStrlenMemchrPair
                | CompositionKind::ToupperMemchrSuffix
                | CompositionKind::ToupperEachSliceSearch => (
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

            match porting::run_composition_call(kind, hay, needle_a, needle_b, n, &dispatch_auth) {
                Ok(c) => {
                    println!("=== Sealed Composition Call ===");
                    println!("Target:        {}", c.target);
                    println!("Input:         {}", framed);
                    for (label, status) in &c.stages {
                        println!("{:<22} {}", format!("{}:", label), status);
                    }
                    println!("Haystack norm: {}", hex_encode(&c.hay_norm));
                    if let Some(slice) = &c.slice {
                        println!("Slice consumed: {}", hex_encode(slice));
                    }
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

/// `phost port cross <symbol> [--out DIR] [--probe PATH] [--no-capability]`
///
/// The cross-implementation court: the target's sealed corpus is observed through
/// the host C library (the sealed path) and through musl via a statically linked
/// probe, and the two must agree on every case. The verdict binds the sealed oracle
/// hash, so the claim cannot be separated from the seal it refers to.
fn cross_cli(args: &[String]) -> i32 {
    use phost::porting::{self, cross_impl, PortingAuthority};

    if args.is_empty() {
        eprintln!("Usage: phost port cross <symbol> [--out DIR] [--probe PATH]");
        return 2;
    }

    let symbol = args[0].as_str();
    let mut out = format!("phost/evidence/cross/{}", symbol);
    let mut probe_path: Option<String> = None;
    let mut auth = PortingAuthority::granted();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                auth = PortingAuthority::none();
                i += 1;
            }
            "--out" if i + 1 < args.len() => {
                out = args[i + 1].clone();
                i += 2;
            }
            "--probe" if i + 1 < args.len() => {
                probe_path = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }

    let target = match porting::resolve_target(symbol) {
        Some(t) => t,
        None => {
            eprintln!("port cross: unknown symbol {}", symbol);
            return 2;
        }
    };

    // The probe is an instrument: build it if the caller did not supply one.
    let probe = match probe_path {
        Some(p) => match cross_impl::probe_artifact(&p) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("port cross: {}", e);
                return 1;
            }
        },
        None => {
            let path = format!("/tmp/phorensic-musl-probe-{}", std::process::id());
            match cross_impl::compile_probe(&path) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("port cross: {}", e);
                    return 1;
                }
            }
        }
    };

    match cross_impl::run_cross_court(&target, &probe, &auth) {
        Ok((v, mismatches)) => {
            println!("=== Cross-Implementation Court ===");
            println!("Target:        {}", v.target);
            println!(
                "Dialect:       {} (locale {})",
                v.dialect, v.locale_contract
            );
            println!("Primary:       {} — {}", v.primary, v.primary_mechanism);
            println!("  observed:    {}", v.primary_version_observed);
            println!("Secondary:     {} — {}", v.secondary, v.secondary_mechanism);
            println!("  identity:    {}", v.secondary_identity);
            println!(
                "Probe source:  {} ({})",
                v.probe_source,
                &v.probe_source_hash[..12]
            );
            println!(
                "Probe binary:  {} (observed, toolchain-bound)",
                &v.probe_binary_hash[..12]
            );
            println!("Cases:         {}", v.cases_run);
            println!("Agreements:    {}", v.agreements);
            println!("Disagreements: {}", v.disagreements);
            println!("Primary oracle hash:   {}", v.primary_oracle_hash);
            println!("Secondary oracle hash: {}", v.secondary_oracle_hash);
            if !mismatches.is_empty() {
                for m in mismatches.iter().take(10) {
                    println!(
                        "  [DISAGREE] {}: primary {} secondary {}",
                        m.case_id, m.primary_hex, m.secondary_hex
                    );
                }
            }
            println!("Verdict:       {}", v.verdict.as_str());

            match phost::porting::evidence::write_cross_evidence(&out, &v, &mismatches) {
                Ok(p) => println!("Evidence:      {}", p),
                Err(e) => {
                    eprintln!("port cross: writing evidence failed: {}", e);
                    return 1;
                }
            }
            if v.is_consistent() {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("port cross {}: {}", symbol, e);
            1
        }
    }
}

/// `phost port session [--out DIR] [--no-capability] [PATH]`
///
/// The long-lived sealed native service: the committed store is loaded and
/// verified **once**, then a deterministic plan serves every sealed port in the
/// store (five leaves, five compositions) through that one service. Proves the
/// seal is loaded once and reused — ten ports from five mapped objects — and
/// writes the session residual.
fn session_cli(args: &[String]) -> i32 {
    use phost::porting::{self, store, PortingAuthority};

    let mut out = String::from("phost/evidence/session");
    let mut store_path = store::STORE_PATH.to_string();
    let mut auth = PortingAuthority::granted();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-capability" => {
                auth = PortingAuthority::none();
                i += 1;
            }
            "--out" if i + 1 < args.len() => {
                out = args[i + 1].clone();
                i += 2;
            }
            other => {
                store_path = other.to_string();
                i += 1;
            }
        }
    }

    match porting::service::run_session(&store_path, &auth) {
        Ok((verdict, mismatches, _service)) => {
            println!("=== Sealed Native Session ===");
            println!("Target:         {}", verdict.target());
            println!("Store:          {}", verdict.store_path);
            println!("Store residual: {}", verdict.store_residual_hash);
            println!("Store loads:    {}", verdict.store_loads);
            println!("Ports in store: {}", verdict.ports_in_store);
            println!("Calls:          {}", verdict.calls);
            println!("Native calls:   {}", verdict.native_calls);
            println!(
                "Foreign fallback: {}   Broken seal: {}",
                verdict.fallback_calls, verdict.broken_seal_calls
            );
            println!("Objects mapped: {}", verdict.objects_mapped);
            println!("Dispatches:     {}", verdict.dispatches);
            println!();
            println!("Fan-in (sealed-port resolutions, nested stages included):");
            for (port, count) in &verdict.per_port {
                println!("  {:<58} {}", port, count);
            }
            if !mismatches.is_empty() {
                println!();
                for m in &mismatches {
                    println!(
                        "  [MISMATCH] {}: expected {} got {} ({})",
                        m.label, m.expected_hex, m.actual_hex, m.reason
                    );
                }
            }
            println!();
            println!("Session hash:   {}", verdict.session_hash);
            println!("Verdict:        {}", verdict.verdict_str());

            if !out.is_empty() {
                match phost::porting::evidence::write_session_evidence(&out, &verdict) {
                    Ok(p) => println!("Evidence:       {}", p),
                    Err(e) => {
                        eprintln!("port session: writing evidence failed: {}", e);
                        return 1;
                    }
                }
            }
            0
        }
        Err(e) => {
            eprintln!("port session: {}", e);
            // A capability denial is the expected, fail-closed outcome of
            // `--no-capability`: the store was never read.
            if e == phost::porting::PortError::CapabilityDenied {
                0
            } else {
                1
            }
        }
    }
}

/// `phost port store [--check|--write] [PATH]`
///
/// The persistent sealed port store. With no flags it **loads and verifies** the
/// committed index — hashing every leaf object against its seal and resolving
/// every composition's stages — and prints what it found. `--write` regenerates
/// the index deterministically from the committed evidence (no compiler, no
/// oracle replay). `--check` is the default and is accepted for explicitness.
fn store_cli(args: &[String]) -> i32 {
    use phost::porting::store;

    let mut write = false;
    let mut path = store::STORE_PATH.to_string();

    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--check" => {}
            other => path = other.to_string(),
        }
    }

    if write {
        match store::regenerate(&path) {
            Ok(doc) => {
                println!("=== Persistent Sealed Port Store (regenerated) ===");
                println!("Path:     {}", path);
                println!("Entries:  {}", doc.entries.len());
                println!("Residual: {}", doc.residual_hash());
                println!("Status:   WRITTEN");
                0
            }
            Err(e) => {
                eprintln!("port store --write: {}", e);
                1
            }
        }
    } else {
        match store::load_with_document(&path) {
            Ok((doc, index)) => {
                let leaves = index
                    .entries()
                    .iter()
                    .filter(|e| e.artifact_kind() == "leaf-object")
                    .count();
                let compositions = index.len() - leaves;
                println!("=== Persistent Sealed Port Store ===");
                println!("Path:     {}", path);
                println!(
                    "Entries:  {} ({} leaf objects, {} compositions)",
                    index.len(),
                    leaves,
                    compositions
                );
                println!("Residual: {}", doc.residual_hash());
                println!();
                for entry in index.entries() {
                    let hash = entry.artifact_hash();
                    println!(
                        "  {:<58} {:<12} {}",
                        entry.target,
                        entry.artifact_kind(),
                        &hash[..12.min(hash.len())]
                    );
                }
                println!();
                println!("Status:   VERIFIED");
                0
            }
            Err(e) => {
                eprintln!("port store: {}", e);
                1
            }
        }
    }
}
