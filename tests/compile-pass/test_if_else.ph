// test_if_else.ph — all if/else patterns
// Tests: if, if/else, if/else if/else, nested if, if in let, if returning value
package if_else;

fn if_only(x: u64) -> u64 {
    let mut result: u64 = 0;
    if x > 5 {
        result = 1;
    };
    return result;
}

fn if_else_basic(x: u64) -> u64 {
    if x > 10 {
        return x;
    } else {
        return 10;
    };
}

fn if_elseif_else(x: u64) -> u64 {
    if x < 10 {
        return 1;
    } else if x < 50 {
        return 2;
    } else if x < 100 {
        return 3;
    } else {
        return 4;
    };
}

fn nested_if(x: u64, y: u64) -> u64 {
    if x > 0 {
        if y > 0 {
            return x + y;
        } else {
            return x;
        };
    } else {
        if y > 0 {
            return y;
        } else {
            return 0;
        };
    };
}

fn if_as_expression(x: u64) -> u64 {
    let result: u64 = if x > 0 {
        x * 2
    } else {
        0
    };
    return result;
}

fn if_chain_expression(x: u64) -> u64 {
    let result: u64 = if x == 0 {
        1
    } else if x == 1 {
        2
    } else if x == 2 {
        4
    } else {
        8
    };
    return result;
}

fn if_boolean_combine(a: u64, b: u64, c: u64) -> u64 {
    if a > b && b > c {
        return a;
    } else if a > c || b > c {
        return b;
    } else {
        return c;
    };
}

fn if_with_compound(x: u64) -> u64 {
    let mut val: u64 = x;
    if x > 0 {
        val = val * 2;
        val = val + 1;
        val = val / 3;
    } else {
        val = val * 3;
        val = val + 2;
        val = val / 2;
    };
    return val;
}

fn if_deeply_nested(a: u64, b: u64, c: u64) -> u64 {
    if a > 0 {
        if b > 0 {
            if c > 0 {
                return a + b + c;
            } else {
                return a + b;
            };
        } else {
            if c > 0 {
                return a + c;
            } else {
                return a;
            };
        };
    } else {
        if b > 0 {
            if c > 0 {
                return b + c;
            } else {
                return b;
            };
        } else {
            if c > 0 {
                return c;
            } else {
                return 0;
            };
        };
    };
}

fn if_with_match(x: u64, flag: bool) -> u64 {
    if flag {
        let result: u64 = match x {
            0 => 10,
            1 => 20,
            _ => 30,
        };
        return result;
    } else {
        let result: u64 = match x {
            0 => 40,
            1 => 50,
            _ => 60,
        };
        return result;
    };
}

fn main() -> u64 {
    let a: u64 = if_only(10);
    let b: u64 = if_else_basic(5);
    let c: u64 = if_elseif_else(30);
    let d: u64 = nested_if(5, 3);
    let e: u64 = if_as_expression(7);
    let f: u64 = if_chain_expression(2);
    let g: u64 = if_boolean_combine(10, 5, 2);
    let h: u64 = if_with_compound(6);
    let i: u64 = if_deeply_nested(1, 1, 1);
    let j: u64 = if_with_match(2, true);
    return a + b + c + d + e + f + g + h + i + j;
}
