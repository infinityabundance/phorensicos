// test_nested_fn.ph — nested function call patterns
// Tests: deep call chains, mutual calls, intermediate results, nested fn composition
package nested_fn;

fn add(x: u64, y: u64) -> u64 {
    return x + y;
}

fn sub(x: u64, y: u64) -> u64 {
    return x - y;
}

fn mul(x: u64, y: u64) -> u64 {
    return x * y;
}

fn inc(x: u64) -> u64 {
    return x + 1;
}

fn dec(x: u64) -> u64 {
    return x - 1;
}

fn double(x: u64) -> u64 {
    return x + x;
}

fn square(x: u64) -> u64 {
    return x * x;
}

fn cube(x: u64) -> u64 {
    return x * x * x;
}

fn compose_add_mul(a: u64, b: u64, c: u64) -> u64 {
    let sum: u64 = add(a, b);
    return mul(sum, c);
}

fn compose_sub_add(a: u64, b: u64, c: u64) -> u64 {
    let diff: u64 = sub(a, b);
    return add(diff, c);
}

fn deep_chain_3(x: u64) -> u64 {
    let a: u64 = inc(x);
    let b: u64 = inc(a);
    let c: u64 = inc(b);
    return c;
}

fn deep_chain_5(x: u64) -> u64 {
    return inc(inc(inc(inc(inc(x)))));
}

fn calc_formula(a: u64, b: u64, c: u64) -> u64 {
    let step1: u64 = add(a, b);
    let step2: u64 = mul(step1, c);
    let step3: u64 = sub(step2, a);
    let step4: u64 = cube(step3);
    return step4;
}

fn nested_mutual(x: u64) -> u64 {
    let a: u64 = inc(x);
    let b: u64 = double(a);
    let c: u64 = dec(b);
    let d: u64 = square(c);
    return d;
}

fn quad_depth(a: u64, b: u64, c: u64, d: u64) -> u64 {
    let s1: u64 = add(a, b);
    let s2: u64 = add(s1, c);
    let s3: u64 = add(s2, d);
    let p1: u64 = mul(s3, a);
    return p1;
}

fn call_chain(result_so_far: u64) -> u64 {
    let mut val: u64 = result_so_far;
    val = inc(val);
    val = double(val);
    val = inc(val);
    val = square(val);
    val = dec(val);
    val = cube(val);
    return val;
}

fn main() -> u64 {
    let a: u64 = compose_add_mul(3, 4, 5);
    let b: u64 = compose_sub_add(10, 3, 2);
    let c: u64 = deep_chain_3(0);
    let d: u64 = deep_chain_5(0);
    let e: u64 = calc_formula(2, 3, 4);
    let f: u64 = nested_mutual(2);
    let g: u64 = quad_depth(1, 2, 3, 4);
    let h: u64 = call_chain(1);
    let x: u64 = inc(dec(inc(dec(5))));
    return a + b + c + d + e + f + g + h + x;
}
