package test

struct Point {
    x: u64,
    y: u64,
}

fn point_x(p: Point) -> u64 {
    return p.x;
}

fn if_else_expr(cond: bool, a: u64, b: u64) -> u64 {
    let result: u64 = if cond { a } else { b };
    return result;
}

fn for_loop_test() -> u64 {
    let mut sum: u64 = 0;
    for i in 5 {
        sum = sum + i;
    };
    return sum;
}

fn nested_block(a: u64, b: u64) -> u64 {
    let result: u64 = {
        let x: u64 = a + b;
        let y: u64 = x * 2;
        y
    };
    return result;
}

fn main() -> u64 {
    let val: u64 = if_else_expr(true, 10, 20);
    let nb: u64 = nested_block(3, 4);
    let fl: u64 = for_loop_test();
    return val + nb + fl;
}
