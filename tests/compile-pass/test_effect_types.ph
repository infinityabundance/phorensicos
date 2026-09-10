// test_effect_types.ph — all effect declaration patterns
// Tests: effect [], effect [single], effect [multi], effect [custom], court annotations
package effect_types;

fn no_effects(x: u64) -> u64
    effect []
{
    return x * 2;
}

fn single_effect(x: u64) -> u64
    effect [compute]
{
    return x + 1;
}

fn io_read_effect() -> u64
    effect [io:read]
{
    return 42;
}

fn io_write_effect(val: u64) -> void
    effect [io:write]
{
    let x: u64 = val;
}

fn io_open_effect() -> u64
    effect [io:open]
{
    return 0;
}

fn blocking_effect_demo() -> u64
    effect [blocking]
{
    return 1;
}

fn residual_effect_demo() -> u64
    effect [residual]
{
    return 2;
}

fn dual_effect() -> u64
    effect [compute, blocking]
{
    return 3;
}

fn tri_effect() -> u64
    effect [compute, blocking, residual]
{
    return 4;
}

fn custom_named_effect(x: u64) -> u64
    effect [custom_effect]
{
    return x;
}

fn effect_and_court_string(x: u64) -> u64
    effect [compute]
    court "arithmetic:v1"
{
    return x + x;
}

fn effect_and_court_bracket(x: u64) -> u64
    effect [io:read]
    court [io_port: v3]
{
    return x;
}

fn four_effects() -> u64
    effect [compute, blocking, residual, custom_effect]
{
    return 5;
}

fn io_rename_effect() -> u64
    effect [io:rename]
{
    return 6;
}

fn io_delete_effect() -> u64
    effect [io:delete]
{
    return 7;
}

fn irq_handle_effect() -> u64
    effect [irq:handle]
{
    return 8;
}

fn machine_ioport_effect() -> u64
    effect [machine:ioport]
{
    return 9;
}

fn memory_mmio_effect() -> u64
    effect [memory:mmio]
{
    return 10;
}

fn cage_translate_effect() -> u64
    effect [cage:translate]
{
    return 11;
}

fn ipc_effects() -> u64
    effect [ipc:send, ipc:receive]
{
    return 12;
}

fn court_request_effect() -> u64
    effect [court:request]
{
    return 13;
}

fn main() -> u64 {
    let a: u64 = no_effects(5);
    let b: u64 = single_effect(5);
    let c: u64 = io_read_effect();
    let d: u64 = blocking_effect_demo();
    let e: u64 = residual_effect_demo();
    let f: u64 = dual_effect();
    let g: u64 = tri_effect();
    let h: u64 = custom_named_effect(7);
    let i: u64 = effect_and_court_string(3);
    let j: u64 = effect_and_court_bracket(9);
    let k: u64 = four_effects();
    let l: u64 = irq_handle_effect();
    let m: u64 = machine_ioport_effect();
    let n: u64 = cage_translate_effect();
    let o: u64 = ipc_effects();
    return a + b + c + d + e + f + g + h + i + j + k + l + m + n + o;
}
