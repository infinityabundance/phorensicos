// test_types.ph — type aliases
// Tests: type aliases, named types
package types

type Age = u64;
type Count = u64;
type Index = u64;
type CoordY = i64;
type Check = u64;

fn use_age(a: Age) -> Count {
    return a;
}

fn convert_age(a: Age) -> u64 {
    let result: Count = a + 10;
    let idx: Index = result;
    return idx;
}

fn coord_ops(x: i64, y: CoordY) -> i64 {
    return x + y;
}

fn type_in_fn_sig(a: Age, b: Age) -> Count {
    return a + b;
}

fn alias_chain(x: Age) -> Count {
    let y: Count = x;
    let z: Index = y;
    let w: Check = z;
    return w;
}

fn alias_comparison(a: Age, b: Age) -> bool {
    return a == b;
}

fn age_operations(a: Age, b: Age) -> Age {
    let sum: Age = a + b;
    let diff: Age = a - b;
    let prod: Age = a * b;
    return sum + diff + prod;
}

fn main() -> u64 {
    let a: Count = use_age(25);
    let b: Count = type_in_fn_sig(10, 20);
    let c: Age = age_operations(5, 3);
    let d: Check = alias_chain(42);
    let e: bool = alias_comparison(7, 7);
    let check: u64 = a + b + c + d;
    if e {
        return check;
    } else {
        return 0;
    };
}
