// test_control_flow.ph — if/else and match expressions
// Tests: conditional branching, if/else, match, boolean conditions
package control_flow

fn abs(x: i64) -> i64 {
    if x < 0 {
        return -x;
    } else {
        return x;
    };
}

fn max(a: u64, b: u64) -> u64 {
    if a > b {
        return a;
    } else {
        return b;
    };
}

fn min(a: u64, b: u64) -> u64 {
    if a < b {
        return a;
    } else {
        return b;
    };
}

fn classify(x: u64) -> u64 {
    let result: u64 = if x == 0 {
        0;
    } else if x < 10 {
        1;
    } else if x < 100 {
        2;
    } else {
        3;
    };
    return result;
}

fn match_number(x: u64) -> u64 {
    let result: u64 = match x {
        0 => 10,
        1 => 20,
        2 => 30,
        3 => 40,
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

fn ternary_like(cond: bool, a: u64, b: u64) -> u64 {
    if cond {
        return a;
    } else {
        return b;
    };
}

fn nested_if(x: u64, y: u64, z: u64) -> u64 {
    if x > 0 {
        if y > 0 {
            return x + y;
        } else {
            return x;
        };
    } else {
        if z > 0 {
            return z;
        } else {
            return 0;
        };
    };
}

fn main() -> u64 {
    let a: i64 = abs(-5);
    let b: u64 = max(10, 20);
    let c: u64 = min(10, 20);
    let d: u64 = classify(50);
    let e: u64 = match_number(2);
    let f: u64 = match_bool(true);
    let g: u64 = ternary_like(true, 100, 200);
    let h: u64 = nested_if(1, 2, 3);
    return a + b + c + d + e + f + g + h;
}
