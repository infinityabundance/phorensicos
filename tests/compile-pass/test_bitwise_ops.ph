// test_bitwise_ops.ph — bitwise operator patterns
// Tests: &, |, ^, ~, bit manipulation algorithms
package bitwise_ops;

fn bitwise_and(a: u64, b: u64) -> u64 {
    return a & b;
}

fn bitwise_or(a: u64, b: u64) -> u64 {
    return a | b;
}

fn bitwise_xor(a: u64, b: u64) -> u64 {
    return a ^ b;
}

fn bitwise_not(a: u64) -> u64 {
    return ~a;
}

fn parity_check(x: u64) -> bool {
    let mut v: u64 = x;
    v = v ^ (v >> 1);
    v = v ^ (v >> 2);
    v = v ^ (v >> 4);
    v = v ^ (v >> 8);
    v = v ^ (v >> 16);
    v = v ^ (v >> 32);
    return (v & 1) == 1;
}

fn count_bits(x: u64) -> u64 {
    let mut v: u64 = x;
    let mut count: u64 = 0;
    loop bound 64 {
        if v == 0 {
            return count;
        };
        count = count + (v & 1);
        v = v >> 1;
    };
    return count;
}

fn is_power_of_two(x: u64) -> bool {
    if x == 0 {
        return false;
    };
    return (x & (x - 1)) == 0;
}

fn next_power_of_two(x: u64) -> u64 {
    let mut v: u64 = x;
    if v == 0 {
        return 1;
    };
    v = v - 1;
    v = v | (v >> 1);
    v = v | (v >> 2);
    v = v | (v >> 4);
    v = v | (v >> 8);
    v = v | (v >> 16);
    v = v | (v >> 32);
    return v + 1;
}

fn bit_swap_bytes(x: u64) -> u64 {
    let b0: u64 = (x >> 56) & 0xFF;
    let b1: u64 = (x >> 48) & 0xFF;
    let b2: u64 = (x >> 40) & 0xFF;
    let b3: u64 = (x >> 32) & 0xFF;
    let b4: u64 = (x >> 24) & 0xFF;
    let b5: u64 = (x >> 16) & 0xFF;
    let b6: u64 = (x >> 8) & 0xFF;
    let b7: u64 = x & 0xFF;
    return (b7 << 56) | (b6 << 48) | (b5 << 40) | (b4 << 32) | (b3 << 24) | (b2 << 16) | (b1 << 8) | b0;
}

fn bit_extract(x: u64, pos: u64) -> u64 {
    return (x >> pos) & 1;
}

fn bit_set(x: u64, pos: u64) -> u64 {
    return x | (1 << pos);
}

fn bit_clear(x: u64, pos: u64) -> u64 {
    return x & ~(1 << pos);
}

fn bit_toggle(x: u64, pos: u64) -> u64 {
    return x ^ (1 << pos);
}

fn bit_mask(lo: u64, hi: u64) -> u64 {
    let ones: u64 = (1 << (hi - lo + 1)) - 1;
    return ones << lo;
}

fn main() -> u64 {
    let a: u64 = bitwise_and(0xFF, 0x0F);
    let b: u64 = bitwise_or(0xF0, 0x0F);
    let c: u64 = bitwise_xor(0xFF, 0x0F);
    let pop: u64 = count_bits(0xFF);
    let pow: bool = is_power_of_two(16);
    let np2: u64 = next_power_of_two(17);
    let swp: u64 = bit_swap_bytes(0x0102030405060708);
    let ext: u64 = bit_extract(0x08, 3);
    let set: u64 = bit_set(0x00, 2);
    let clr: u64 = bit_clear(0xFF, 3);
    let tog: u64 = bit_toggle(0xF0, 4);
    let mask: u64 = bit_mask(2, 5);
    return a + b + c + pop + np2 + ext + set + clr + tog + mask;
}
