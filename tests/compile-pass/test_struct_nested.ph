// test_struct_nested.ph — nested struct patterns
// Tests: nested struct definitions, struct field access chains, struct in struct in struct
package struct_nested;

struct Point {
    x: i64,
    y: i64,
}

struct Line {
    start: Point,
    end: Point,
}

struct Triangle {
    a: Point,
    b: Point,
    c: Point,
}

struct Label {
    text: u64,
    position: Point,
}

struct BoundingBox {
    min: Point,
    max: Point,
}

struct Circle {
    center: Point,
    radius: u64,
}

struct Polygon {
    num_sides: u64,
    bounding: BoundingBox,
}

struct Group {
    name: u64,
    first: Shape,
    second: Shape,
}

enum Shape {
    Circle(Circle),
    Rect(BoundingBox),
    Line(Line),
}

fn point_is_origin(p: Point) -> bool {
    return p.x == 0 && p.y == 0;
}

fn line_length_sq(l: Line) -> u64 {
    let dx: i64 = l.end.x - l.start.x;
    let dy: i64 = l.end.y - l.start.y;
    let dx_u: u64 = dx;
    let dy_u: u64 = dy;
    return dx_u * dx_u + dy_u * dy_u;
}

fn triangle_centroid(t: Triangle) -> Point {
    let sum_x: i64 = t.a.x + t.b.x + t.c.x;
    let sum_y: i64 = t.a.y + t.b.y + t.c.y;
    let cx: i64 = sum_x / 3;
    let cy: i64 = sum_y / 3;
    return Point { x: cx, y: cy };
}

fn box_area(b: BoundingBox) -> u64 {
    let w: u64 = b.max.x - b.min.x;
    let h: u64 = b.max.y - b.min.y;
    return w * h;
}

fn circle_area(c: Circle) -> u64 {
    let r: u64 = c.radius;
    return r * r;
}

fn polygon_box_area(p: Polygon) -> u64 {
    return box_area(p.bounding);
}

fn deep_field_access(l: Line) -> u64 {
    let sx: i64 = l.start.x;
    let sy: i64 = l.start.y;
    let ex: i64 = l.end.x;
    let ey: i64 = l.end.y;
    return sx + sy + ex + ey;
}

fn label_shift(l: Label, dx: u64, dy: u64) -> Point {
    let new_x: i64 = l.position.x + dx;
    let new_y: i64 = l.position.y + dy;
    return Point { x: new_x, y: new_y };
}

fn point_add(a: Point, b: Point) -> Point {
    return Point { x: a.x + b.x, y: a.y + b.y };
}

fn point_dot(a: Point, b: Point) -> i64 {
    return a.x * b.x + a.y * b.y;
}

fn main() -> u64 {
    let origin: Point = Point { x: 0, y: 0 };
    let p1: Point = Point { x: 3, y: 4 };
    let p2: Point = Point { x: 6, y: 8 };
    let l: Line = Line { start: origin, end: p1 };
    let len_sq: u64 = line_length_sq(l);
    let is_origin: bool = point_is_origin(origin);
    let tri: Triangle = Triangle { a: origin, b: p1, c: p2 };
    let ct: Point = triangle_centroid(tri);
    let bbox: BoundingBox = BoundingBox { min: origin, max: p2 };
    let area: u64 = box_area(bbox);
    let deep: u64 = deep_field_access(l);
    let dot: i64 = point_dot(p1, p2);
    return len_sq + area + deep + dot;
}
