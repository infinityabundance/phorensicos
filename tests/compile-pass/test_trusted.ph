// test_trusted.ph — trusted block patterns
// Tests: trusted { }, trusted reason "..." { }, trusted with court receipt, trusted in fn
package trusted;

fn trusted_simple(x: u64) -> u64 {
    let result: u64 = trusted {
        x + 1
    };
    return result;
}

fn trusted_reason_demo(x: u64) -> u64 {
    let result: u64 = trusted reason "arithmetic is safe within bounds" {
        x * 2
    };
    return result;
}

fn trusted_reason_block(x: u64, y: u64) -> u64 {
    let sum: u64 = trusted reason "addition of positive integers" {
        let a: u64 = x;
        let b: u64 = y;
        a + b
    };
    return sum;
}

fn trusted_with_court(x: u64) -> u64
    court "arithmetic:v1"
{
    let result: u64 = trusted reason "multiplication via court-approved arithmetic" {
        x * x
    };
    return result;
}

fn trusted_nested(x: u64) -> u64 {
    let inner: u64 = trusted reason "inner computation" {
        let doubled: u64 = trusted reason "doubling" {
            x + x
        };
        doubled * 2
    };
    return inner;
}

fn trusted_with_if(x: u64) -> u64 {
    if x > 10 {
        let result: u64 = trusted reason "large value transformation" {
            x / 2
        };
        return result;
    } else {
        let result: u64 = trusted reason "small value transformation" {
            x * 2
        };
        return result;
    };
}

fn trusted_block_as_stmt(x: u64) -> u64 {
    let mut val: u64 = x;
    trusted reason "mutation is safe in this context" {
        val = val + 1;
    };
    return val;
}

fn trusted_chain(x: u64) -> u64 {
    let a: u64 = trusted reason "step one" {
        x + 1
    };
    let b: u64 = trusted reason "step two" {
        a * 2
    };
    let c: u64 = trusted reason "step three" {
        b - 3
    };
    return c;
}

fn trusted_effect_compat(x: u64) -> u64
    effect [compute]
{
    let result: u64 = trusted reason "compute effect in trusted block" {
        x * x + 1
    };
    return result;
}

fn trusted_court_combo(x: u64) -> u64
    effect [compute]
    court "combined:v1"
{
    let base: u64 = trusted reason "base computation" {
        x * 3
    };
    let adjusted: u64 = trusted reason "adjustment" {
        base + 7
    };
    return adjusted;
}

fn main() -> u64 {
    let a: u64 = trusted_simple(5);
    let b: u64 = trusted_reason_demo(10);
    let c: u64 = trusted_reason_block(3, 4);
    let d: u64 = trusted_with_court(6);
    let e: u64 = trusted_nested(2);
    let f: u64 = trusted_with_if(15);
    let g: u64 = trusted_block_as_stmt(7);
    let h: u64 = trusted_chain(5);
    let i: u64 = trusted_effect_compat(4);
    let j: u64 = trusted_court_combo(8);
    return a + b + c + d + e + f + g + h + i + j;
}
