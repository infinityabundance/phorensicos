package test

struct Point {
    x: u64,
    y: u64,
}

enum Color {
    Red,
    Green,
    Blue,
}

type Age = u64;

fn match_test(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 10,
        1 => 20,
        _ => 0,
    };
    return result;
}

fn loop_test(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    loop bound 100 {
        if i >= n { return sum; };
        sum = sum + i;
        i = i + 1;
    };
    return sum;
}

fn while_test(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    while i < n {
        sum = sum + i;
        i = i + 1;
    };
    return sum;
}

fn effect_test(x: u64) -> u64
    effect [compute]
{
    return x + 1;
}

fn main() -> u64 {
    let m: u64 = match_test(1);
    let l: u64 = loop_test(5);
    let w: u64 = while_test(5);
    let e: u64 = effect_test(10);
    let a: Age = 25;
    return m + l + w + e + a;
}
