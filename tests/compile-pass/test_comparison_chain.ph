// test_comparison_chain.ph — comparison chains and boolean logic patterns
// Tests: ==, !=, <, >, <=, >=, &&, ||, ! operators
package comparison_chain;

fn eq_check(a: u64, b: u64) -> bool {
    return a == b;
}

fn ne_check(a: u64, b: u64) -> bool {
    return a != b;
}

fn lt_check(a: u64, b: u64) -> bool {
    return a < b;
}

fn gt_check(a: u64, b: u64) -> bool {
    return a > b;
}

fn le_check(a: u64, b: u64) -> bool {
    return a <= b;
}

fn ge_check(a: u64, b: u64) -> bool {
    return a >= b;
}

fn and_chain(a: bool, b: bool, c: bool) -> bool {
    return a && b && c;
}

fn or_chain(a: bool, b: bool, c: bool) -> bool {
    return a || b || c;
}

fn mixed_bool(a: bool, b: bool, c: bool) -> bool {
    return a && b || c && !a;
}

fn not_operator(x: bool) -> bool {
    return !x;
}

fn not_chain(a: bool, b: bool) -> bool {
    return !a && !b;
}

fn in_range(x: u64, lo: u64, hi: u64) -> bool {
    return x >= lo && x <= hi;
}

fn out_of_range(x: u64, lo: u64, hi: u64) -> bool {
    return x < lo || x > hi;
}

fn is_even(x: u64) -> bool {
    return x % 2 == 0;
}

fn is_odd(x: u64) -> bool {
    return x % 2 != 0;
}

fn compare_bool_eq(a: bool, b: bool) -> bool {
    return a == b;
}

fn compare_u64_chain(a: u64, b: u64) -> bool {
    return a == b && (a < 100 || b < 100) && !(a > 50 && b > 50);
}

fn compare_mixed(a: u64, b: u64, c: u64) -> bool {
    return (a < b && b < c) || (a > b && b > c) || (a == b && b == c);
}

fn compare_with_mod(a: u64, b: u64) -> u64 {
    if a % 3 == 0 && b % 3 == 0 {
        return a + b;
    } else if a % 3 == 0 || b % 3 == 0 {
        return a * b;
    } else {
        let diff: u64 = a - b;
        return diff;
    };
}

fn compare_three_way(a: u64, b: u64) -> i64 {
    if a > b {
        return 1;
    } else if a < b {
        return -1;
    } else {
        return 0;
    };
}

fn main() -> u64 {
    let eq: bool = eq_check(5, 5);
    let ne: bool = ne_check(5, 3);
    let ir: bool = in_range(5, 0, 10);
    let or: bool = out_of_range(15, 0, 10);
    let even: bool = is_even(42);
    let odd: bool = is_odd(43);
    let cc: bool = compare_u64_chain(50, 60);
    let cm: bool = compare_mixed(1, 2, 3);
    let cw: u64 = compare_with_mod(9, 12);
    let tw: i64 = compare_three_way(10, 5);
    let a: u64 = eq + ne + ir + or + cc + cm + cw + tw;
    return a;
}
