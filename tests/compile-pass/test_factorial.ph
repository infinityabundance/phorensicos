// test_factorial.ph — factorial computation
// Tests: while loops, loop bound, arithmetic
package factorial

fn factorial_while(n: u64) -> u64 {
    let mut result: u64 = 1;
    let mut i: u64 = 1;
    while i <= n {
        result = result * i;
        i = i + 1;
    };
    return result;
}

fn factorial_loop(n: u64) -> u64 {
    let mut result: u64 = 1;
    let mut i: u64 = 1;
    loop bound 100 {
        if i > n {
            return result;
        };
        result = result * i;
        i = i + 1;
    };
    return result;
}

fn factorial_range_sum(start: u64, end: u64) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = start;
    loop bound 50 {
        if i > end {
            return sum;
        };
        sum = sum + factorial_while(i);
        i = i + 1;
    };
    return sum;
}

fn binomial_coefficient(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    };
    if k == 0 || k == n {
        return 1;
    };
    let n_fact: u64 = factorial_while(n);
    let k_fact: u64 = factorial_while(k);
    let nk_fact: u64 = factorial_while(n - k);
    return n_fact / (k_fact * nk_fact);
}

fn falling_factorial(n: u64, k: u64) -> u64 {
    let mut result: u64 = 1;
    let mut i: u64 = 0;
    loop bound 50 {
        if i >= k {
            return result;
        };
        result = result * (n - i);
        i = i + 1;
    };
    return result;
}

fn double_factorial(n: u64) -> u64 {
    let mut result: u64 = 1;
    let mut i: u64 = n;
    loop bound 50 {
        if i < 1 {
            return result;
        };
        result = result * i;
        if i < 2 {
            return result;
        };
        i = i - 2;
    };
    return result;
}

fn factorial_ratio(n: u64, m: u64) -> u64 {
    return factorial_while(n) / factorial_while(m);
}

fn main() -> u64 {
    let f5: u64 = factorial_while(5);
    let f7: u64 = factorial_loop(7);
    let fsum: u64 = factorial_range_sum(1, 5);
    let binom: u64 = binomial_coefficient(10, 3);
    let fall: u64 = falling_factorial(10, 3);
    let df: u64 = double_factorial(6);
    let ratio: u64 = factorial_ratio(10, 7);
    return f5 + f7 + fsum + binom + fall + df + ratio;
}
