// test_cap_types.ph — all capability type patterns
// Tests: Cap(T), Handle(T), Cap nested types, cap with effects, multi-cap
package cap_types;

struct Console {
    id: u64,
    baud: u64,
}

struct Port {
    addr: u16,
    direction: u8,
}

struct MemoryRegion {
    base: u64,
    size: u64,
}

struct DmaChannel {
    ch: u64,
    flags: u64,
}

fn cap_console(console: Cap(Console)) -> u64
    effect [io:write]
    court "console:v1"
{
    return console.id;
}

fn cap_port(p: Cap(Port)) -> u64
    effect [machine:ioport]
{
    let addr: u64 = p.addr;
    return addr;
}

fn handle_region(region: Handle(MemoryRegion)) -> u64
    effect [memory:mmio]
{
    return region.size;
}

fn dual_cap(c1: Cap(Console), c2: Cap(Port)) -> u64
    effect [io:write, machine:ioport]
{
    let a: u64 = c1.baud;
    let b: u64 = c2.addr;
    return a + b;
}

fn cap_handle_mix(c: Cap(Port), h: Handle(MemoryRegion)) -> u64
    effect [machine:ioport, memory:mmio]
{
    return c.addr + h.size;
}

fn cap_tuple_param(p: Cap((u64, u64))) -> u64 {
    let val: u64 = p.0;
    return val;
}

fn handle_array_param(devs: Handle(Array(Console, 4))) -> u64 {
    return 0;
}

fn cap_effect_only(c: Cap(Console)) -> u64
    effect [io:write]
{
    return c.id;
}

fn handle_no_params(h: Handle(Port)) -> u64 {
    return h.direction;
}

fn cap_move_demo(c: Cap(Console)) -> Cap(Console) {
    return c;
}

fn multi_effect_cap(c: Cap(DmaChannel)) -> u64
    effect [compute, dma]
{
    return c.ch;
}

fn cap_with_court_named(c: Cap(Console)) -> u64
    effect [io:read]
    court [io_port: v1]
{
    return c.id;
}

fn cap_four_params(c: Cap(Console), p: Cap(Port), m: Handle(MemoryRegion), d: Cap(DmaChannel)) -> u64
    effect [io:read, machine:ioport, memory:mmio, dma]
{
    let sum: u64 = c.baud + p.addr + m.size + d.ch;
    return sum;
}

fn main() -> u64 {
    let cc: u64 = cap_console(42);
    let cp: u64 = cap_port(0);
    let hr: u64 = handle_region(0);
    let dc: u64 = dual_cap(0, 0);
    let chm: u64 = cap_handle_mix(0, 0);
    let ct: u64 = cap_tuple_param(0);
    let ha: u64 = handle_array_param(0);
    let ce: u64 = cap_effect_only(0);
    let hn: u64 = handle_no_params(0);
    let me: u64 = multi_effect_cap(0);
    let cf: u64 = cap_four_params(0, 0, 0, 0);
    return cc + cp + hr + dc + chm + ct + ha + ce + hn + me + cf;
}
