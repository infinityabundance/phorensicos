// test_loops.ph — bounded loops, while loops, loop constructs
// Tests: loop bound N, while, nested loops, loop with break pattern
package loops

fn count_to_n(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    loop bound 100 {
        if i >= n {
            return sum;
        };
        i = i + 1;
        sum = sum + i;
    };
    return sum;
}

fn while_count(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    while i < n {
        i = i + 1;
        sum = sum + i;
    };
    return sum;
}

fn while_proven(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    while i < n proven {
        i = i + 1;
        sum = sum + i;
    };
    return sum;
}

fn loop_with_break(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut acc: u64 = 1;
    loop bound 50 {
        i = i + 1;
        acc = acc * 2;
        if i >= n {
            return acc;
        };
    };
    return acc;
}

fn sum_squares(n: u64) -> u64 {
    let mut i: u64 = 0;
    let mut sum: u64 = 0;
    loop bound 100 {
        if i > n {
            return sum;
        };
        sum = sum + i * i;
        i = i + 1;
    };
    return sum;
}

fn nested_loops() -> u64 {
    let mut outer: u64 = 0;
    loop bound 5 {
        let mut inner: u64 = 0;
        loop bound 5 {
            inner = inner + 1;
        };
        outer = outer + inner;
    };
    return outer;
}

fn main() -> u64 {
    let a: u64 = count_to_n(10);
    let b: u64 = while_count(10);
    let c: u64 = loop_with_break(5);
    let d: u64 = sum_squares(3);
    let e: u64 = nested_loops();
    return a + b + c + d + e;
}
