// test_shift_ops.ph — bit shift operator patterns
// Tests: <<, >>, shift in algorithms
package shift_ops;

fn shl_simple(x: u64, n: u64) -> u64 {
    return x << n;
}

fn shr_simple(x: u64, n: u64) -> u64 {
    return x >> n;
}

fn shl_chained(x: u64) -> u64 {
    let a: u64 = x << 1;
    let b: u64 = a << 2;
    let c: u64 = b << 3;
    return c;
}

fn shr_chained(x: u64) -> u64 {
    let a: u64 = x >> 3;
    let b: u64 = a >> 2;
    let c: u64 = b >> 1;
    return c;
}

fn shift_compose(x: u64) -> u64 {
    let a: u64 = x << 4;
    let b: u64 = a >> 2;
    return b;
}

fn pack_bytes(b0: u8, b1: u8, b2: u8, b3: u8) -> u64 {
    let v0: u64 = b0 << 24;
    let v1: u64 = b1 << 16;
    let v2: u64 = b2 << 8;
    let v3: u64 = b3;
    return v0 | v1 | v2 | v3;
}

fn unpack_byte0(x: u64) -> u8 {
    return x;
}

fn unpack_byte1(x: u64) -> u8 {
    return x >> 8;
}

fn unpack_byte2(x: u64) -> u8 {
    return x >> 16;
}

fn unpack_byte3(x: u64) -> u8 {
    return x >> 24;
}

fn recombine_bytes(x: u64) -> u64 {
    let b0: u64 = x & 0xFF;
    let b1: u64 = (x >> 8) & 0xFF;
    let b2: u64 = (x >> 16) & 0xFF;
    let b3: u64 = (x >> 24) & 0xFF;
    return (b3 << 24) | (b2 << 16) | (b1 << 8) | b0;
}

fn shift_loop_product(base: u64, shift_count: u64) -> u64 {
    let mut result: u64 = base;
    let mut i: u64 = 0;
    loop bound 8 {
        if i >= shift_count {
            return result;
        };
        result = result << 1;
        i = i + 1;
    };
    return result;
}

fn shift_loop_divide(base: u64, shift_count: u64) -> u64 {
    let mut result: u64 = base;
    let mut i: u64 = 0;
    loop bound 8 {
        if i >= shift_count {
            return result;
        };
        result = result >> 1;
        i = i + 1;
    };
    return result;
}

fn uint8_from_be_bytes(b0: u8, b1: u8, b2: u8, b3: u8, b4: u8, b5: u8, b6: u8, b7: u8) -> u64 {
    let p0: u64 = b0 << 56;
    let p1: u64 = b1 << 48;
    let p2: u64 = b2 << 40;
    let p3: u64 = b3 << 32;
    let p4: u64 = b4 << 24;
    let p5: u64 = b5 << 16;
    let p6: u64 = b6 << 8;
    let p7: u64 = b7;
    return p0 | p1 | p2 | p3 | p4 | p5 | p6 | p7;
}

fn main() -> u64 {
    let a: u64 = shl_simple(1, 5);
    let b: u64 = shr_simple(64, 3);
    let c: u64 = shl_chained(1);
    let d: u64 = shr_chained(0xFF);
    let e: u64 = shift_compose(1);
    let p: u64 = pack_bytes(0xDE, 0xAD, 0xBE, 0xEF);
    let r: u64 = recombine_bytes(p);
    let prod: u64 = shift_loop_product(1, 5);
    let div: u64 = shift_loop_divide(256, 4);
    let be: u64 = uint8_from_be_bytes(0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08);
    return a + b + c + d + e + prod + div + be;
}
