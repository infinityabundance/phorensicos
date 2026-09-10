// test_structs.ph — struct declarations and field access
// Tests: struct definitions, field access on struct parameters, nested structs
package structs

struct Point {
    x: u64,
    y: u64,
}

struct Rectangle {
    min: Point,
    max: Point,
}

struct Pair {
    first: u64,
    second: u64,
}

struct Triple {
    a: u64,
    b: u64,
    c: u64,
}

struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

struct Empty {}

fn point_x(p: Point) -> u64 {
    return p.x;
}

fn point_y(p: Point) -> u64 {
    return p.y;
}

fn rect_area(r: Rectangle) -> u64 {
    let width: u64 = r.max.x - r.min.x;
    let height: u64 = r.max.y - r.min.y;
    return width * height;
}

fn swap_pair(p: Pair) -> Pair {
    return p;
}

fn triple_sum(t: Triple) -> u64 {
    return t.a + t.b + t.c;
}

fn color_grayscale(c: Color) -> u64 {
    let r_val: u64 = c.r;
    let g_val: u64 = c.g;
    let b_val: u64 = c.b;
    return r_val + g_val + b_val;
}

fn nested_field_access(r: Rectangle) -> u64 {
    return r.min.x + r.min.y + r.max.x + r.max.y;
}

fn main() -> u64 {
    let check: u64 = 0;
    return check;
}
