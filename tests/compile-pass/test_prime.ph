// test_prime.ph — prime number computation
// Tests: while loops, modulo, boolean logic, nested if/else
package prime

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    };
    if n == 2 {
        return true;
    };
    if n % 2 == 0 {
        return false;
    };
    let mut i: u64 = 3;
    while i * i <= n {
        if n % i == 0 {
            return false;
        };
        i = i + 2;
    };
    return true;
}

fn count_primes(limit: u64) -> u64 {
    let mut count: u64 = 0;
    let mut n: u64 = 2;
    loop bound 1000 {
        if n > limit {
            return count;
        };
        if is_prime(n) {
            count = count + 1;
        };
        n = n + 1;
    };
    return count;
}

fn nth_prime(n: u64) -> u64 {
    let mut count: u64 = 0;
    let mut candidate: u64 = 2;
    loop bound 1000 {
        if is_prime(candidate) {
            count = count + 1;
            if count == n {
                return candidate;
            };
        };
        candidate = candidate + 1;
    };
    return 0;
}

fn sum_primes(limit: u64) -> u64 {
    let mut sum: u64 = 0;
    let mut n: u64 = 2;
    while n <= limit {
        if is_prime(n) {
            sum = sum + n;
        };
        n = n + 1;
    };
    return sum;
}

fn largest_prime_factor(n: u64) -> u64 {
    let mut m: u64 = n;
    let mut factor: u64 = 2;
    while factor * factor <= m {
        if m % factor == 0 {
            m = m / factor;
        } else {
            factor = factor + 1;
        };
    };
    if m > 1 {
        return m;
    };
    return factor;
}

fn prime_gap(start: u64, end: u64) -> u64 {
    let mut prev: u64 = 0;
    let mut max_gap: u64 = 0;
    let mut n: u64 = start;
    while n <= end {
        if is_prime(n) {
            if prev != 0 {
                let gap: u64 = n - prev;
                if gap > max_gap {
                    max_gap = gap;
                };
            };
            prev = n;
        };
        n = n + 1;
    };
    return max_gap;
}

fn main() -> u64 {
    let p17: bool = is_prime(17);
    let p25: bool = is_prime(25);
    let cnt: u64 = count_primes(100);
    let nth: u64 = nth_prime(10);
    let sum: u64 = sum_primes(30);
    let lpf: u64 = largest_prime_factor(84);
    let gap: u64 = prime_gap(90, 100);
    return cnt + nth + sum + lpf + gap;
}
