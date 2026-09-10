// test_compound.ph — compound example with multiple features
// Tests: integration of types, enums, loops, math, effects, match, control flow
package compound

type Value = u64;
type Flag = bool;
type Checksum = u64;

enum Status {
    Active,
    Inactive,
    Error(u64),
}

fn compute_checksum_4(data: Array(u64, 4)) -> Checksum {
    let mut sum: Checksum = 0;
    let mut i: u64 = 0;
    loop bound 4 {
        sum = sum + data[i];
        i = i + 1;
    };
    return sum;
}

fn transform_value(v: Value, multiplier: u64) -> Value {
    return v * multiplier;
}

fn status_code(s: Status) -> u64 {
    let result: u64 = match s {
        Active => 0,
        Inactive => 1,
        Error => 2,
    };
    return result;
}

fn evaluate_flag(f: Flag) -> u64 {
    if f {
        return 1;
    } else {
        return 0;
    };
}

fn process_data(data: Array(u64, 4)) -> u64 {
    let sum: u64 = compute_checksum_4(data);
    let transformed: u64 = transform_value(sum, 2);
    return transformed;
}

fn nested_computation(n: u64) -> u64 {
    let mut result: u64 = 0;
    let mut i: u64 = 0;
    while i < n {
        let mut j: u64 = 0;
        let mut inner_sum: u64 = 0;
        while j < i {
            inner_sum = inner_sum + j;
            j = j + 1;
        };
        result = result + inner_sum;
        i = i + 1;
    };
    return result;
}

fn bool_math_hybrid(x: u64, y: u64) -> u64 {
    let eq: bool = x == y;
    let lt: bool = x < y;
    let gt: bool = x > y;
    if eq {
        return x * y;
    } else if lt {
        return y - x;
    } else {
        return x - y;
    };
}

fn loop_with_match(start: u64) -> u64 {
    let mut x: u64 = start;
    let mut sum: u64 = 0;
    loop bound 50 {
        if x == 0 {
            return sum;
        };
        let step: u64 = match x % 3 {
            0 => x,
            1 => x * 2,
            _ => x * 3,
        };
        sum = sum + step;
        x = x - 1;
    };
    return sum;
}

fn effectful_computation(x: u64) -> u64
    effect [compute]
{
    let mut i: u64 = 0;
    let mut acc: u64 = 1;
    loop bound 20 {
        if i >= x {
            return acc;
        };
        acc = acc * (i + 1);
        i = i + 1;
    };
    return acc;
}

fn main() -> u64 {
    let cs: Checksum = 0;
    let tv: Value = transform_value(5, 3);
    let sc: u64 = status_code(Active);
    let ef: u64 = evaluate_flag(true);
    let nc: u64 = nested_computation(5);
    let bm: u64 = bool_math_hybrid(10, 5);
    let lm: u64 = loop_with_match(10);
    let ec: u64 = effectful_computation(5);
    return cs + tv + sc + ef + nc + bm + lm + ec;
}
