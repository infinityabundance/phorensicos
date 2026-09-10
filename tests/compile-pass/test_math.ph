// test_math.ph — arithmetic, comparisons, math operations
// Tests: +, -, *, /, %, comparison operators, function calls, return values
package math

fn add_u64(x: u64, y: u64) -> u64 {
    return x + y;
}

fn sub_u64(x: u64, y: u64) -> u64 {
    let result: u64 = x - y;
    return result;
}

fn mul_u64(x: u64, y: u64) -> u64 {
    return x * y;
}

fn div_u64(x: u64, y: u64) -> u64 {
    return x / y;
}

fn rem_u64(x: u64, y: u64) -> u64 {
    return x % y;
}

fn neg_i64(x: i64) -> i64 {
    return -x;
}

fn compare_eq(x: u64, y: u64) -> bool {
    return x == y;
}

fn compare_ne(x: u64, y: u64) -> bool {
    return x != y;
}

fn compare_lt(x: u64, y: u64) -> bool {
    return x < y;
}

fn compare_le(x: u64, y: u64) -> bool {
    return x <= y;
}

fn compare_gt(x: u64, y: u64) -> bool {
    return x > y;
}

fn compare_ge(x: u64, y: u64) -> bool {
    return x >= y;
}

fn arithmetic_chain(a: u64, b: u64, c: u64) -> u64 {
    let result: u64 = a + b * c - a / b + a % b;
    return result;
}

fn main() -> u64 {
    let x: u64 = add_u64(10, 20);
    let y: u64 = sub_u64(100, 30);
    let z: u64 = mul_u64(6, 7);
    let w: u64 = div_u64(42, 7);
    let r: u64 = rem_u64(10, 3);
    let eq: bool = compare_eq(5, 5);
    let ne: bool = compare_ne(5, 3);
    let lt: bool = compare_lt(3, 5);
    let gt: bool = compare_gt(5, 3);
    let total: u64 = x + y + z + w + r;
    return total;
}
