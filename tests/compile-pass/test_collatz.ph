// test_collatz.ph — Collatz sequence computation
// Tests: while loops, conditional branching, arithmetic
package collatz

fn collatz_step(n: u64) -> u64 {
    if n % 2 == 0 {
        return n / 2;
    } else {
        return n * 3 + 1;
    };
}

fn collatz_sequence_length(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut length: u64 = 1;
    loop bound 1000 {
        if x == 1 {
            return length;
        };
        x = collatz_step(x);
        length = length + 1;
    };
    return length;
}

fn collatz_max_value(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut max_val: u64 = n;
    loop bound 1000 {
        if x == 1 {
            return max_val;
        };
        x = collatz_step(x);
        if x > max_val {
            max_val = x;
        };
    };
    return max_val;
}

fn collatz_odd_count(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut odd_count: u64 = 0;
    loop bound 1000 {
        if x == 1 {
            return odd_count;
        };
        if x % 2 == 1 {
            odd_count = odd_count + 1;
        };
        x = collatz_step(x);
    };
    return odd_count;
}

fn collatz_even_count(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut even_count: u64 = 0;
    loop bound 1000 {
        if x == 1 {
            return even_count;
        };
        if x % 2 == 0 {
            even_count = even_count + 1;
        };
        x = collatz_step(x);
    };
    return even_count;
}

fn longest_collatz(limit: u64) -> u64 {
    let mut longest: u64 = 0;
    let mut best_n: u64 = 0;
    let mut i: u64 = 1;
    loop bound 1000 {
        if i > limit {
            return best_n;
        };
        let len: u64 = collatz_sequence_length(i);
        if len > longest {
            longest = len;
            best_n = i;
        };
        i = i + 1;
    };
    return best_n;
}

fn main() -> u64 {
    let len_27: u64 = collatz_sequence_length(27);
    let max_27: u64 = collatz_max_value(27);
    let odd_27: u64 = collatz_odd_count(27);
    let even_27: u64 = collatz_even_count(27);
    let best: u64 = longest_collatz(100);
    return len_27 + max_27 + odd_27 + even_27 + best;
}
