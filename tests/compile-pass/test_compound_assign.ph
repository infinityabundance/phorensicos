// test_compound_assign.ph — compound assignment operators
// Tests: +=, -=, *=, /=, %= operators
package compound_assign;

fn compound_add() -> u64 {
    let mut x: u64 = 10;
    x += 5;
    return x;
}

fn compound_sub() -> u64 {
    let mut x: u64 = 20;
    x -= 7;
    return x;
}

fn compound_mul() -> u64 {
    let mut x: u64 = 6;
    x *= 4;
    return x;
}

fn compound_div() -> u64 {
    let mut x: u64 = 42;
    x /= 6;
    return x;
}

fn compound_rem() -> u64 {
    let mut x: u64 = 17;
    x %= 5;
    return x;
}

fn compound_chain() -> u64 {
    let mut x: u64 = 100;
    x += 10;
    x -= 5;
    x *= 2;
    x /= 3;
    x %= 7;
    return x;
}

fn compound_loop() -> u64 {
    let mut x: u64 = 1;
    let mut i: u64 = 0;
    loop bound 5 {
        x *= 2;
        i += 1;
    };
    return x;
}

fn compound_accumulate() -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 1;
    loop bound 10 {
        sum += i;
        i += 1;
    };
    return sum;
}

fn compound_factorial() -> u64 {
    let mut prod: u64 = 1;
    let mut i: u64 = 1;
    loop bound 8 {
        if i > 5 {
            return prod;
        };
        prod *= i;
        i += 1;
    };
    return prod;
}

fn compound_mixed_ops() -> u64 {
    let mut x: u64 = 1;
    let mut y: u64 = 10;
    y += 5;
    x *= y;
    x += 3;
    x /= 2;
    return x;
}

fn compound_countdown() -> u64 {
    let mut x: u64 = 10;
    let mut sum: u64 = 0;
    loop bound 10 {
        sum += x;
        x -= 1;
        if x == 0 {
            return sum;
        };
    };
    return sum;
}

fn compound_power() -> u64 {
    let mut result: u64 = 1;
    let mut i: u64 = 0;
    loop bound 8 {
        result *= 3;
        i += 1;
        if i >= 4 {
            return result;
        };
    };
    return result;
}

fn main() -> u64 {
    let a: u64 = compound_add();
    let b: u64 = compound_sub();
    let c: u64 = compound_mul();
    let d: u64 = compound_div();
    let e: u64 = compound_rem();
    let f: u64 = compound_chain();
    let g: u64 = compound_loop();
    let h: u64 = compound_accumulate();
    let i: u64 = compound_factorial();
    let j: u64 = compound_mixed_ops();
    let k: u64 = compound_countdown();
    let l: u64 = compound_power();
    return a + b + c + d + e + f + g + h + i + j + k + l;
}
