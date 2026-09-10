// test_bound_loops.ph — all loop bound patterns
// Tests: loop bound N, while, while proven, for in range, nested bound loops
package bound_loops;

fn loop_bound_simple() -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 10 {
        sum = sum + i;
        i = i + 1;
    };
    return sum;
}

fn loop_bound_large() -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 1000 {
        if i >= 100 {
            return sum;
        };
        sum = sum + 1;
        i = i + 1;
    };
    return sum;
}

fn loop_bound_with_break() -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 50 {
        i = i + 1;
        if i > 7 {
            return sum;
        };
        sum = sum + i;
    };
    return sum;
}

fn while_loop_simple(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    while i < n {
        sum = sum + i;
        i = i + 1;
    };
    return sum;
}

fn while_loop_proven(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    while i < n proven {
        sum = sum + i * i;
        i = i + 1;
    };
    return sum;
}

fn while_countdown(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut sum: u64 = 0;
    while x > 0 {
        sum = sum + x;
        x = x - 1;
    };
    return sum;
}

fn nested_while_loops() -> u64 {
    let mut outer: u64 = 0;
    let mut i: u64 = 0;
    while i < 4 {
        let mut j: u64 = 0;
        let mut inner_sum: u64 = 0;
        while j < 3 {
            inner_sum = inner_sum + j;
            j = j + 1;
        };
        outer = outer + inner_sum;
        i = i + 1;
    };
    return outer;
}

fn nested_loop_bound() -> u64 {
    let mut total: u64 = 0;
    let mut i: u64 = 0;
    loop bound 5 {
        let mut j: u64 = 0;
        loop bound 5 {
            total = total + i * j;
            j = j + 1;
        };
        i = i + 1;
    };
    return total;
}

fn triple_nested_bound() -> u64 {
    let mut total: u64 = 0;
    let mut i: u64 = 0;
    loop bound 3 {
        let mut j: u64 = 0;
        loop bound 3 {
            let mut k: u64 = 0;
            loop bound 3 {
                total = total + i + j + k;
                k = k + 1;
            };
            j = j + 1;
        };
        i = i + 1;
    };
    return total;
}

fn loop_bound_with_early() -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 20 {
        if i >= 10 {
            return sum;
        };
        if i % 2 == 0 {
            sum = sum + i;
        };
        i = i + 1;
    };
    return sum;
}

fn while_proven_countdown(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut prod: u64 = 1;
    while x > 1 proven {
        prod = prod * x;
        x = x - 1;
    };
    return prod;
}

fn main() -> u64 {
    let a: u64 = loop_bound_simple();
    let b: u64 = loop_bound_large();
    let c: u64 = loop_bound_with_break();
    let d: u64 = while_loop_simple(5);
    let e: u64 = while_loop_proven(5);
    let f: u64 = while_countdown(10);
    let g: u64 = nested_while_loops();
    let h: u64 = nested_loop_bound();
    let i: u64 = triple_nested_bound();
    let j: u64 = loop_bound_with_early();
    let k: u64 = while_proven_countdown(5);
    return a + b + c + d + e + f + g + h + i + j + k;
}
