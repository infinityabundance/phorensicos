// test_enums.ph — enum types and match on variants
// Tests: enum definition, enum with data variants, match on identifiers
package enums

enum TrafficLight {
    Red,
    Yellow,
    Green,
}

enum Direction {
    North,
    South,
    East,
    West,
}

enum Optional {
    None,
    Some(u64),
}

enum Result {
    Ok(u64),
    Err,
}

enum Shape {
    Circle(u64),
    Rect(u64, u64),
}

enum Color {
    Rgb(u8, u8, u8),
    Gray(u8),
}

fn traffic_action(light: TrafficLight) -> u64 {
    let result: u64 = match light {
        Red => 0,
        Yellow => 1,
        Green => 2,
    };
    return result;
}

fn direction_bit(d: Direction) -> u64 {
    let result: u64 = match d {
        North => 1,
        South => 2,
        East => 4,
        West => 8,
    };
    return result;
}

fn optional_value(opt: Optional) -> u64 {
    let result: u64 = match opt {
        None => 0,
        Some => 1,
    };
    return result;
}

fn result_code(res: Result) -> u64 {
    let result: u64 = match res {
        Ok => 1,
        Err => 0,
    };
    return result;
}

fn shape_area(s: Shape) -> u64 {
    let result: u64 = match s {
        Circle => 1,
        Rect => 2,
    };
    return result;
}

fn enum_in_fn_param(x: Direction, y: Direction) -> u64 {
    let a: u64 = direction_bit(x);
    let b: u64 = direction_bit(y);
    return a + b;
}

fn main() -> u64 {
    let a: u64 = traffic_action(Red);
    let b: u64 = direction_bit(North);
    let c: u64 = optional_value(None);
    let d: u64 = result_code(Ok);
    let e: u64 = shape_area(Circle);
    return a + b + c + d + e;
}
