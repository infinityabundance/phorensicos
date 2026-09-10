// test_enum_nested.ph — nested enum patterns
// Tests: enum with struct variants, enum with enum variants, deep nesting
package enum_nested;

struct Point {
    x: i64,
    y: i64,
}

struct Size {
    w: u64,
    h: u64,
}

enum Shape {
    Circle(Point, u64),
    Rect(Point, Size),
    Line(Point, Point),
    Polygon(u64, Point),
}

enum Form {
    Simple,
    Complex(Shape),
    Group(Shape, Shape),
}

enum Container {
    Empty,
    Single(Form),
    Double(Form, Form),
}

enum Optional {
    None,
    Some(u64),
}

enum Result {
    Ok(u64),
    Err,
}

fn shape_kind(s: Shape) -> u64 {
    let result: u64 = match s {
        Circle => 0,
        Rect => 1,
        Line => 2,
        Polygon => 3,
    };
    return result;
}

fn form_depth(f: Form) -> u64 {
    let result: u64 = match f {
        Simple => 0,
        Complex => 1,
        Group => 2,
    };
    return result;
}

fn container_count(c: Container) -> u64 {
    let result: u64 = match c {
        Empty => 0,
        Single => 1,
        Double => 2,
    };
    return result;
}

fn opts_has_value(o: Optional) -> bool {
    let result: bool = match o {
        None => false,
        Some => true,
    };
    return result;
}

fn res_is_ok(r: Result) -> bool {
    let result: bool = match r {
        Ok => true,
        Err => false,
    };
    return result;
}

fn multi_nested_enum(c: Container) -> u64 {
    let result: u64 = match c {
        Empty => 0,
        Single => 1,
        Double => 2,
    };
    return result;
}

fn enum_in_enum(x: Container) -> u64 {
    let form_count: u64 = match x {
        Empty => 0,
        Single => 1,
        Double => 2,
    };
    if form_count > 0 {
        return form_count * 10;
    } else {
        return 0;
    };
}

fn shape_area(s: Shape) -> u64 {
    let kind: u64 = shape_kind(s);
    if kind == 0 {
        return 314;
    } else if kind == 1 {
        return 100;
    } else if kind == 2 {
        return 50;
    } else {
        return 20;
    };
}

fn main() -> u64 {
    let sk: u64 = shape_kind(Circle);
    let fd: u64 = form_depth(Simple);
    let cc: u64 = container_count(Empty);
    let hv: bool = opts_has_value(Some);
    let rok: bool = res_is_ok(Ok);
    let mn: u64 = multi_nested_enum(Single);
    let ein: u64 = enum_in_enum(Double);
    let sa: u64 = shape_area(Circle);
    return sk + fd + cc + mn + ein + sa;
}
