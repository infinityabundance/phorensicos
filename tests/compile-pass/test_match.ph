// test_match.ph — all match expression patterns
// Tests: match on u64, match on bool, match on enum variants, match with block arms, wildcard
package match;

enum Status {
    Active,
    Inactive,
    Pending(u64),
    Error(u64, u64),
}

enum Color {
    Red,
    Green,
    Blue,
    Custom(u8, u8, u8),
}

fn match_u64(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 100,
        1 => 200,
        2 => 300,
        3 => 400,
        _ => 999,
    };
    return result;
}

fn match_u64_range_like(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 9,
        4 => 16,
        5 => 25,
        6 => 36,
        7 => 49,
        _ => 0,
    };
    return result;
}

fn match_bool(b: bool) -> u64 {
    let result: u64 = match b {
        true => 1,
        false => 0,
    };
    return result;
}

fn match_status(s: Status) -> u64 {
    let result: u64 = match s {
        Active => 0,
        Inactive => 1,
        Pending => 2,
        Error => 3,
    };
    return result;
}

fn match_color(c: Color) -> u64 {
    let result: u64 = match c {
        Red => 0xFF0000,
        Green => 0x00FF00,
        Blue => 0x0000FF,
        Custom => 0xFFFFFF,
    };
    return result;
}

fn match_block_arms(x: u64) -> u64 {
    let result: u64 = match x {
        0 => {
            let a: u64 = 10;
            let b: u64 = 20;
            a + b
        },
        1 => {
            let a: u64 = 30;
            let b: u64 = 40;
            a + b
        },
        _ => {
            let a: u64 = 1;
            let b: u64 = 2;
            a * b
        },
    };
    return result;
}

fn match_multi_same(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 0,
        1 => 1,
        2 => 0,
        3 => 1,
        _ => 0,
    };
    return result;
}

fn match_u64_squares(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 9,
        _ => x * x,
    };
    return result;
}

fn match_u64_mod(x: u64) -> u64 {
    let result: u64 = match x % 4 {
        0 => 10,
        1 => 20,
        2 => 30,
        3 => 40,
        _ => 0,
    };
    return result;
}

fn match_chain(x: u64) -> u64 {
    let first: u64 = match x % 3 {
        0 => 100,
        1 => 200,
        _ => 300,
    };
    let second: u64 = match x % 2 {
        0 => 5,
        _ => 10,
    };
    return first + second;
}

fn main() -> u64 {
    let a: u64 = match_u64(2);
    let b: u64 = match_u64_range_like(4);
    let c: u64 = match_bool(true);
    let d: u64 = match_status(Active);
    let e: u64 = match_color(Red);
    let f: u64 = match_block_arms(0);
    let g: u64 = match_u64_squares(5);
    let h: u64 = match_u64_mod(10);
    let i: u64 = match_chain(7);
    return a + b + c + d + e + f + g + h + i;
}
