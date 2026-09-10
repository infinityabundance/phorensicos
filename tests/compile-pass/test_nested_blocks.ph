// test_nested_blocks.ph — nested blocks and scoping
// Tests: block expressions, nested scopes, variable assignments in blocks
package nested_blocks

fn block_in_if(cond: bool, x: u64) -> u64 {
    if cond {
        let y: u64 = x + 10;
        let z: u64 = y * 2;
        return z;
    } else {
        let y: u64 = x - 10;
        let z: u64 = y / 2;
        return z;
    };
}

fn block_in_while(n: u64) -> u64 {
    let mut x: u64 = n;
    let mut sum: u64 = 0;
    while x > 0 {
        let y: u64 = x * x;
        let z: u64 = y / 2;
        sum = sum + z;
        x = x - 1;
    };
    return sum;
}

fn block_in_loop(n: u64) -> u64 {
    let mut x: u64 = 1;
    let mut result: u64 = 1;
    loop bound 50 {
        if x > n {
            return result;
        };
        let y: u64 = result * x;
        let z: u64 = y + 1;
        result = z;
        x = x + 1;
    };
    return result;
}

fn nested_blocks_in_match(x: u64) -> u64 {
    let result: u64 = match x {
        0 => {
            let a: u64 = 10;
            let b: u64 = a + 20;
            b
        },
        1 => {
            let a: u64 = 30;
            let b: u64 = a + 40;
            b
        },
        _ => 0,
    };
    return result;
}

fn multi_block_expression(a: u64, b: u64) -> u64 {
    let sum: u64 = {
        let x: u64 = a + b;
        let y: u64 = x * x;
        y - x
    };
    let diff: u64 = {
        if a > b {
            a - b
        } else {
            b - a
        };
    };
    return sum + diff;
}

fn deep_nesting(n: u64) -> u64 {
    let a: u64 = {
        let b: u64 = {
            let c: u64 = {
                let d: u64 = n + 1;
                d * 2
            };
            c + 3
        };
        b + 4
    };
    return a;
}

fn main() -> u64 {
    let a: u64 = block_in_if(true, 100);
    let b: u64 = block_in_while(5);
    let c: u64 = block_in_loop(10);
    let d: u64 = nested_blocks_in_match(0);
    let e: u64 = multi_block_expression(10, 3);
    let f: u64 = deep_nesting(7);
    return a + b + c + d + e + f;
}
