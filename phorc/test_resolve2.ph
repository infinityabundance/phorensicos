package test

fn main() -> u64 {
    let s: Str(64) = Str::from("hi");
    let n = s.len();
    return n;
}
