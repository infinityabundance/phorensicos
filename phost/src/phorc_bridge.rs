// phorc_bridge.rs — Simulated phorc compiler bridge for the shell demo
// Returns canned compilation metrics for known .phor example files.
// A real kernel would invoke the phorc binary via a process manager or
// link against the compiler library directly.

use alloc::format;

/// Result of a (simulated) compilation attempt
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub source_file: [u8; 256],
    pub source_len: usize,
    pub object_bytes: u64,
    pub functions_emitted: u64,
    pub diagnostics: [u8; 2048],
    pub diag_len: usize,
    pub parse_items: u64,
}

impl CompileResult {
    pub fn new() -> Self {
        Self {
            success: false,
            source_file: [0u8; 256],
            source_len: 0,
            object_bytes: 0,
            functions_emitted: 0,
            diagnostics: [0u8; 2048],
            diag_len: 0,
            parse_items: 0,
        }
    }

    pub fn source(&self) -> &str {
        core::str::from_utf8(&self.source_file[..self.source_len]).unwrap_or("")
    }

    pub fn diagnostics_str(&self) -> &str {
        if self.diag_len == 0 {
            return "";
        }
        core::str::from_utf8(&self.diagnostics[..self.diag_len]).unwrap_or("")
    }

    pub fn record(&mut self, success: bool, source: &str, output: &str) {
        self.success = success;
        self.source_len = source.len().min(255);
        self.source_file[..self.source_len].copy_from_slice(&source.as_bytes()[..self.source_len]);

        // Parse key metrics from output
        for line in output.lines() {
            let line = line.trim();
            if line.contains("Functions emitted:") {
                if let Some(n) = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    self.functions_emitted = n;
                }
            }
            if line.contains("Code size:") {
                if let Some(n) = line.split(':').nth(1).and_then(|s| {
                    s.trim()
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u64>().ok())
                }) {
                    self.object_bytes = n;
                }
            }
            if line.contains("Items parsed:") {
                if let Some(n) = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    self.parse_items = n;
                }
            }
        }

        // Capture diagnostics
        let combined = format!(
            "Source: {}\nFunctions: {}\nCode: {} bytes\nItems: {}\nStatus: {}",
            source,
            self.functions_emitted,
            self.object_bytes,
            self.parse_items,
            if success { "OK" } else { "FAILED" }
        );
        self.diag_len = combined.len().min(2047);
        self.diagnostics[..self.diag_len].copy_from_slice(&combined.as_bytes()[..self.diag_len]);
    }

    /// Simulate a compilation result (used when we can't actually invoke phorc)
    pub fn simulated(name: &str) -> Self {
        let mut result = Self::new();
        result.record(true, name, "Compilation simulated");
        result.success = true;
        result.functions_emitted = 5;
        result.object_bytes = 2048;
        result.parse_items = 10;
        result
    }
}

/// Simulate compiling a .phor file.
///
/// Returns canned metrics for known examples (hello.phor, canvas_demo.phor,
/// serial_port.phor). Unknown names return a failure result.
///
/// In a real system, this would invoke the phorc compiler binary:
///   std::process::Command::new("phorc").arg(source_name).arg("output.o").output()
/// or link against phorc as a library:
///   phorc::compile(source_name, output_path)
pub fn compile_phor(source_name: &str) -> CompileResult {
    // In a real kernel, we would invoke:
    //   std::process::Command::new("phorc")
    //       .arg(source_name)
    //       .arg("output.o")
    //       .output()
    //
    // For the boot-time shell demo, return a simulated result.
    let mut result = CompileResult::new();
    match source_name {
        "hello.phor" | "examples/hello.phor" => {
            result.record(
                true,
                source_name,
                "=== Phorc Compilation Pipeline ===\n\
                 Input: hello.phor\n\
                 Items parsed: 10\n\
                 Checking... No errors.\n\
                 Lowering to PHIR...\n\
                 Functions: 3\n\
                 Generating x86-64 code...\n\
                 Code size: 6912 bytes\n\
                 Functions emitted: 23\n\
                 Wrote object file: hello.o\n\
                 === Compilation Complete ===",
            );
        }
        "canvas_demo.phor" | "examples/canvas_demo.phor" => {
            result.record(
                true,
                source_name,
                "Input: canvas_demo.phor\n\
                 Items parsed: 12\n\
                 Code size: 2800 bytes\n\
                 Functions emitted: 13\n\
                 === Compilation Complete ===",
            );
        }
        "serial_port.phor" | "examples/serial_port.phor" => {
            result.record(
                true,
                source_name,
                "Input: serial_port.phor\n\
                 Items parsed: 8\n\
                 Code size: 1632 bytes\n\
                 Functions emitted: 5\n\
                 === Compilation Complete ===",
            );
        }
        _ => {
            result.record(
                false,
                source_name,
                "Error: source file not found in compilation cache\n\
                 Available: hello.phor, canvas_demo.phor, serial_port.phor",
            );
        }
    }
    result
}
