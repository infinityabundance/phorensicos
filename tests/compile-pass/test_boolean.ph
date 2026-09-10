// test_boolean.ph — boolean operations and logic
// Tests: &&, ||, !, bool type, de Morgan's laws, conditional logic
package boolean

fn bool_not(b: bool) -> bool {
    return !b;
}

fn bool_and(a: bool, b: bool) -> bool {
    return a && b;
}

fn bool_or(a: bool, b: bool) -> bool {
    return a || b;
}

fn bool_xor(a: bool, b: bool) -> bool {
    return (a && !b) || (!a && b);
}

fn bool_implies(a: bool, b: bool) -> bool {
    return !a || b;
}

fn de_morgan_and(a: bool, b: bool) -> bool {
    let left: bool = !(a && b);
    let right: bool = !a || !b;
    return left == right;
}

fn de_morgan_or(a: bool, b: bool) -> bool {
    let left: bool = !(a || b);
    let right: bool = !a && !b;
    return left == right;
}

fn bool_all_true(a: bool, b: bool, c: bool) -> bool {
    return a && b && c;
}

fn bool_any_true(a: bool, b: bool, c: bool) -> bool {
    return a || b || c;
}

fn bool_chain(a: bool, b: bool, c: bool) -> bool {
    return (a || b) && (!a || c) && (!b || !c);
}

fn in_range(x: u64, low: u64, high: u64) -> bool {
    return x >= low && x <= high;
}

fn is_even(n: u64) -> bool {
    return n % 2 == 0;
}

fn is_odd(n: u64) -> bool {
    return n % 2 == 1;
}

fn both_even(a: u64, b: u64) -> bool {
    return is_even(a) && is_even(b);
}

fn either_odd(a: u64, b: u64) -> bool {
    return is_odd(a) || is_odd(b);
}

fn bool_to_u64(b: bool) -> u64 {
    if b {
        return 1;
    } else {
        return 0;
    };
}

fn main() -> u64 {
    let t: bool = true;
    let f: bool = false;
    let not_t: bool = bool_not(t);
    let and_tf: bool = bool_and(t, f);
    let or_tf: bool = bool_or(t, f);
    let xor_tf: bool = bool_xor(t, f);
    let dm1: bool = de_morgan_and(t, f);
    let dm2: bool = de_morgan_or(t, f);
    let range: bool = in_range(5, 1, 10);
    let ev: bool = is_even(42);
    let od: bool = is_odd(43);
    let all: bool = bool_all_true(t, t, t);
    let any: bool = bool_any_true(f, f, t);
    let chain: bool = bool_chain(t, f, t);
    let b_even: bool = both_even(2, 4);
    let e_odd: bool = either_odd(2, 3);
    return bool_to_u64(not_t) + bool_to_u64(and_tf) + bool_to_u64(or_tf) + bool_to_u64(dm1) + bool_to_u64(range) + bool_to_u64(ev) + bool_to_u64(all) + bool_to_u64(chain);
}
