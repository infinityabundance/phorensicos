// test_tuple_nested.ph — nested tuple patterns
// Tests: tuple types, tuple field access via .0 .1, nested tuples, tuple returns
package tuple_nested;

type Pair = (u64, u64);
type Triple = (u64, u64, u64);
type Quad = (u64, u64, u64, u64);
type Nested = (u64, (u64, u64), u64);
type DeepNest = ((u64, u64), (u64, (u64, u64)));

fn make_pair(a: u64, b: u64) -> Pair {
    return (a, b);
}

fn make_triple(a: u64, b: u64, c: u64) -> Triple {
    return (a, b, c);
}

fn swap_pair(p: Pair) -> Pair {
    return (p.1, p.0);
}

fn pair_sum(p: Pair) -> u64 {
    return p.0 + p.1;
}

fn triple_sum(t: Triple) -> u64 {
    return t.0 + t.1 + t.2;
}

fn pair_mul(p: Pair) -> u64 {
    return p.0 * p.1;
}

fn nested_pair(p: Nested) -> u64 {
    let outer_first: u64 = p.0;
    let inner: (u64, u64) = p.1;
    let outer_third: u64 = p.2;
    return outer_first + inner.0 + inner.1 + outer_third;
}

fn deep_nest_access(d: DeepNest) -> u64 {
    let first_pair: (u64, u64) = d.0;
    let second_pair: (u64, (u64, u64)) = d.1;
    let inner_val: (u64, u64) = second_pair.1;
    return first_pair.0 + first_pair.1 + second_pair.0 + inner_val.0 + inner_val.1;
}

fn triple_from_fn(a: u64, b: u64, c: u64) -> Triple {
    let t: Triple = (a, b, c);
    return t;
}

fn tuple_chain(a: u64, b: u64) -> u64 {
    let p: Pair = swap_pair(make_pair(a, b));
    let t: Triple = make_triple(p.0, p.1, a);
    return triple_sum(t);
}

fn quad_sum(q: Quad) -> u64 {
    return q.0 + q.1 + q.2 + q.3;
}

fn tuple_math(a: u64, b: u64, c: u64) -> u64 {
    let p: Pair = (a + b, b + c);
    let t: Triple = (p.0 + p.1, p.0 * p.1, c);
    return triple_sum(t);
}

fn pair_compare(a: Pair, b: Pair) -> bool {
    return a.0 == b.0 && a.1 == b.1;
}

fn main() -> u64 {
    let p: Pair = make_pair(3, 7);
    let ps: u64 = pair_sum(p);
    let sp: Pair = swap_pair(p);
    let sps: u64 = pair_sum(sp);
    let t: Triple = make_triple(1, 2, 3);
    let ts: u64 = triple_sum(t);
    let n: Nested = (1, (2, 3), 4);
    let nv: u64 = nested_pair(n);
    let d: DeepNest = ((1, 2), (3, (4, 5)));
    let dv: u64 = deep_nest_access(d);
    let tc: u64 = tuple_chain(5, 9);
    let tm: u64 = tuple_math(2, 3, 4);
    let pm: u64 = pair_mul((6, 7));
    return ps + sps + ts + nv + dv + tc + tm + pm;
}
