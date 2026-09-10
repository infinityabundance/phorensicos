// test_gcd.ph — greatest common divisor and related algorithms
// Tests: while loops, modulo, Euclid's algorithm
package gcd

fn gcd_euclid(a: u64, b: u64) -> u64 {
    let mut x: u64 = a;
    let mut y: u64 = b;
    while y != 0 {
        let tmp: u64 = y;
        y = x % y;
        x = tmp;
    };
    return x;
}

fn gcd_loop(a: u64, b: u64) -> u64 {
    let mut x: u64 = a;
    let mut y: u64 = b;
    loop bound 100 {
        if y == 0 {
            return x;
        };
        let tmp: u64 = y;
        y = x % y;
        x = tmp;
    };
    return x;
}

fn lcm(a: u64, b: u64) -> u64 {
    let g: u64 = gcd_euclid(a, b);
    return a / g * b;
}

fn gcd_of_three(a: u64, b: u64, c: u64) -> u64 {
    let ab: u64 = gcd_euclid(a, b);
    return gcd_euclid(ab, c);
}

fn are_coprime(a: u64, b: u64) -> bool {
    let g: u64 = gcd_euclid(a, b);
    return g == 1;
}

fn extended_gcd_check(a: u64, b: u64) -> u64 {
    let g: u64 = gcd_euclid(a, b);
    if g == 0 {
        return 0;
    };
    return a % g + b % g;
}

fn gcd_range_sum(start: u64, end: u64) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = start;
    loop bound 50 {
        if i >= end {
            return sum;
        };
        sum = sum + gcd_euclid(i, end);
        i = i + 1;
    };
    return sum;
}

fn gcd_iterations(a: u64, b: u64) -> u64 {
    let mut x: u64 = a;
    let mut y: u64 = b;
    let mut count: u64 = 0;
    loop bound 100 {
        if y == 0 {
            return count;
        };
        let tmp: u64 = y;
        y = x % y;
        x = tmp;
        count = count + 1;
    };
    return count;
}

fn main() -> u64 {
    let g1: u64 = gcd_euclid(48, 18);
    let g2: u64 = gcd_loop(1071, 462);
    let l: u64 = lcm(12, 18);
    let cp: bool = are_coprime(7, 13);
    let g3: u64 = gcd_of_three(12, 18, 24);
    let sum: u64 = gcd_range_sum(10, 20);
    let iter: u64 = gcd_iterations(48, 18);
    return g1 + g2 + l + g3 + sum + iter;
}
