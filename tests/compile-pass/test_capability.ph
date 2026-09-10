// test_capability.ph — capability types, Handle, Slice, Str types
// Tests: Cap(type), Handle(type), Slice, Str type syntax
package capability

struct Device {
    id: u64,
    kind: u64,
}

struct Buffer {
    base: u64,
    size: u64,
}

struct Channel {
    port: u64,
    flags: u64,
}

fn take_capability(console: Cap(u64)) -> u64
    effect []
{
    return console;
}

fn take_handle(dev: Handle(Device)) -> u64 {
    return 0;
}

fn use_slice(data: Slice(u8)) -> u64 {
    return 0;
}

fn multiple_caps(c1: Cap(u64), c2: Cap(u64)) -> u64
    effect [compute]
{
    return c1 + c2;
}

fn cap_and_handle(c: Cap(u64), h: Handle(Device)) -> u64
    effect [compute]
{
    return c;
}

fn cap_with_effect(x: Cap(u64)) -> u64
    effect [compute]
    court "capability:v1"
{
    return x;
}

fn handle_buffer(h: Handle(Buffer), offset: u64) -> u64 {
    return offset;
}

fn cap_in_array(arr: Array(Cap(u64), 4)) -> u64 {
    return 0;
}

fn handle_of_slice(data: Handle(Slice(u8))) -> u64 {
    return 0;
}

fn main() -> u64 {
    let a: u64 = take_capability(42);
    let b: u64 = take_handle(0);
    let c: u64 = use_slice(0);
    let d: u64 = multiple_caps(1, 2);
    let e: u64 = cap_and_handle(3, 0);
    let f: u64 = cap_with_effect(5);
    let g: u64 = handle_buffer(0, 100);
    let val: u64 = a + b + c + d + e + f + g;
    return val;
}
