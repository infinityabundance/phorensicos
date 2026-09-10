// Phorensic test program: capability move, bounded loops, effects, trusted blocks
// Demonstrates the language features from PHORENSIC_LANGUAGE.md

package test_capabilities;

// A capability type for console access
struct Console {}
struct ConsoleReadOnly {}

// Block device info with explicit layout
struct BlockDeviceInfo
    layout packed
{
    block_size: u32,
    block_count: u64,
}

// Typed error enum
type IoError = enum {
    NotFound,
    PermissionDenied,
    DeviceFault(u64),
    StaleHandle(u64, u64),
}

// Pure compute function (no effects)
fn add(x: u64, y: u64) -> u64
    effect []
{
    return x + y;
}

// Function with declared effects and court citation
fn read_block(dev: Handle(BlockDevice), block: u64, data: Slice(u8))
    effect [io:write, residual]
    court "block:v3"
{
    // Fixed-capacity buffer (compile-time bound)
    let buf: Array(u8, 512) = Array::zeroed();

    // Bounded loop (bound = buf.capacity() = 512)
    for i in 0..buf.capacity() {
        buf[i] = data[i];
    }

    // Residual emission
    residual emit {
        op: "block.write",
        device: dev.id,
        block: block,
        data_hash: hash(data),
    };

    return void;
}

// Function accepting a capability (affine — cannot be copied)
fn consume_console(console: Cap(Console))
    effect []
{
    // console is moved here, cannot be used after
}

// Function demonstrating effect closure
fn outer() effect [io:write, compute] {
    let x: u64 = add(1, 2);   // OK: 'add' has effect [], outer has [io:write, compute]
    read_block(Handle(BlockDevice).new(), 0, Slice::empty());
    // OK: read_block needs [io:write, residual]
    // outer's effect set [io:write, compute] does NOT include residual!
    // COMPILE ERROR expected here — demonstrating E0401
}

// Trusted block with rationale — accepted
fn read_cpu_model() -> u64
    effect [machine:ioport]
{
    trusted reason "CPU model register read is safe: documented at AMD APM v3 §2.1.5"
    {
        return 42;  // machine_read_msr(0x1A);
    }
}

// Demonstrates fail for trusted block without rationale
fn read_msr_bad() -> u64
    effect [machine:ioport]
{
    // COMPILE ERROR E0601: trusted block requires documented rationale
    trusted {
        return 0;
    }
}
