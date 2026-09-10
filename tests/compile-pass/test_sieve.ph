// test_sieve.ph — prime counting using trial division
// Tests: nested loops, if/else, boolean logic, loop bound
package sieve

fn is_prime_trial(n: u64) -> bool {
    if n < 2 {
        return false;
    };
    if n == 2 {
        return true;
    };
    if n % 2 == 0 {
        return false;
    };
    let mut d: u64 = 3;
    while d * d <= n {
        if n % d == 0 {
            return false;
        };
        d = d + 2;
    };
    return true;
}

fn count_primes_upto(limit: u64) -> u64 {
    let mut count: u64 = 0;
    let mut n: u64 = 2;
    loop bound 1000 {
        if n > limit {
            return count;
        };
        if is_prime_trial(n) {
            count = count + 1;
        };
        n = n + 1;
    };
    return count;
}

fn count_composites_upto(limit: u64) -> u64 {
    let mut count: u64 = 0;
    let mut n: u64 = 2;
    loop bound 1000 {
        if n > limit {
            return count;
        };
        if !is_prime_trial(n) {
            count = count + 1;
        };
        n = n + 1;
    };
    return count;
}

fn sum_of_primes_below(limit: u64) -> u64 {
    let mut sum: u64 = 0;
    let mut n: u64 = 2;
    loop bound 1000 {
        if n >= limit {
            return sum;
        };
        if is_prime_trial(n) {
            sum = sum + n;
        };
        n = n + 1;
    };
    return sum;
}

fn prime_sum_pair_count(limit: u64) -> u64 {
    let mut count: u64 = 0;
    let mut p: u64 = 2;
    loop bound 1000 {
        if p > limit {
            return count;
        };
        if is_prime_trial(p) {
            let q: u64 = limit - p;
            if q >= 2 && is_prime_trial(q) {
                count = count + 1;
            };
        };
        p = p + 1;
    };
    return count;
}

fn main() -> u64 {
    let primes: u64 = count_primes_upto(100);
    let composites: u64 = count_composites_upto(100);
    let sum: u64 = sum_of_primes_below(100);
    let pairs: u64 = prime_sum_pair_count(50);
    return primes + composites + sum + pairs;
}
