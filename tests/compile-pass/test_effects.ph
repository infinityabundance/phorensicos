// test_effects.ph — functions with effect declarations
// Tests: effect annotations, court annotations, effect []
package effects

fn pure_compute(x: u64, y: u64) -> u64
    effect []
{
    return x + y;
}

fn declared_compute() -> u64
    effect [compute]
{
    return 42;
}

fn blocking_effect() -> void
    effect [blocking]
{
    let x: u64 = 0;
    let y: u64 = x + 1;
}

fn court_annotated() -> u64
    effect [compute]
    court "arithmetic:v1"
{
    return 100;
}

fn multi_effect() -> u64
    effect [compute, blocking]
    court "io:standard"
{
    let x: u64 = 1;
    let y: u64 = 2;
    return x + y;
}

fn named_effect_fn(x: u64) -> u64
    effect [custom_effect]
{
    return x * 2;
}

fn effect_with_params(a: u64, b: u64) -> u64
    effect [compute]
    court "math:v2"
{
    let result: u64 = a + b;
    return result;
}

fn empty_effect_chain(x: u64) -> u64
    effect []
{
    let y: u64 = x + 1;
    let z: u64 = y * 2;
    return z;
}

fn three_effects(x: u64) -> u64
    effect [compute, blocking, custom_effect]
{
    return x;
}

fn court_without_effect(x: u64) -> u64
    court "bare:v1"
{
    return x + 1;
}

fn main() -> u64 {
    let a: u64 = pure_compute(3, 4);
    let b: u64 = declared_compute();
    let c: u64 = court_annotated();
    let d: u64 = multi_effect();
    let e: u64 = named_effect_fn(7);
    let f: u64 = effect_with_params(10, 20);
    let g: u64 = empty_effect_chain(5);
    let h: u64 = three_effects(9);
    let i: u64 = court_without_effect(99);
    return a + b + c + d + e + f + g + h + i;
}
