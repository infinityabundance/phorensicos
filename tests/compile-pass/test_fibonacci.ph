// test_fibonacci.ph — Fibonacci sequence computation
// Tests: bounded loops, mutable state, loop with conditional returns
package fibonacci

fn fib_iterative(n: u64) -> u64 {
    if n == 0 {
        return 0;
    };
    if n == 1 {
        return 1;
    };
    let mut prev: u64 = 0;
    let mut curr: u64 = 1;
    let mut i: u64 = 2;
    loop bound 100 {
        if i > n {
            return curr;
        };
        let next: u64 = prev + curr;
        prev = curr;
        curr = next;
        i = i + 1;
    };
    return curr;
}

fn fib_while(n: u64) -> u64 {
    if n == 0 {
        return 0;
    };
    if n == 1 {
        return 1;
    };
    let mut prev: u64 = 0;
    let mut curr: u64 = 1;
    let mut i: u64 = 2;
    while i <= n {
        let next: u64 = prev + curr;
        prev = curr;
        curr = next;
        i = i + 1;
    };
    return curr;
}

fn fib_sum_sequence(n: u64) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 50 {
        if i > n {
            return sum;
        };
        sum = sum + a;
        let next: u64 = a + b;
        a = b;
        b = next;
        i = i + 1;
    };
    return sum;
}

fn fib_nth_even(n: u64) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let mut count: u64 = 0;
    let mut i: u64 = 0;
    loop bound 50 {
        if count >= n {
            return a;
        };
        if a % 2 == 0 {
            count = count + 1;
            if count == n {
                return a;
            };
        };
        let next: u64 = a + b;
        a = b;
        b = next;
        i = i + 1;
    };
    return 0;
}

fn fib_compare(a: u64, b: u64) -> bool {
    let fa: u64 = fib_iterative(a);
    let fb: u64 = fib_iterative(b);
    return fa == fb;
}

fn main() -> u64 {
    let f10: u64 = fib_iterative(10);
    let f15: u64 = fib_while(15);
    let sum: u64 = fib_sum_sequence(10);
    let even: u64 = fib_nth_even(3);
    return f10 + f15 + sum + even;
}
