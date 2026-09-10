// test_base64.ph — Base64 encoding and decoding simulation
// Tests: Array indexing, bitwise ops, table lookup via match, modulus arithmetic
package base64;

fn base64_char(val: u64) -> u8 {
    let result: u8 = match val {
        0 => 0x41,
        1 => 0x42,
        2 => 0x43,
        3 => 0x44,
        4 => 0x45,
        5 => 0x46,
        6 => 0x47,
        7 => 0x48,
        8 => 0x49,
        9 => 0x4A,
        10 => 0x4B,
        11 => 0x4C,
        12 => 0x4D,
        _ => 0x41,
    };
    return result;
}

fn base64_encode_triple(a: u8, b: u8, c: u8) -> Array(u8, 4) {
    let combined: u64 = (a << 16) | (b << 8) | c;
    let c0: u64 = (combined >> 18) & 0x3F;
    let c1: u64 = (combined >> 12) & 0x3F;
    let c2: u64 = (combined >> 6) & 0x3F;
    let c3: u64 = combined & 0x3F;
    return (base64_char(c0), base64_char(c1), base64_char(c2), base64_char(c3));
}

fn base64_decode_quad(a: u8, b: u8, c: u8, d: u8) -> Array(u8, 3) {
    let va: u64 = a;
    let vb: u64 = b;
    let vc: u64 = c;
    let vd: u64 = d;
    let combined: u64 = (va << 18) | (vb << 12) | (vc << 6) | vd;
    let b0: u8 = (combined >> 16) & 0xFF;
    let b1: u8 = (combined >> 8) & 0xFF;
    let b2: u8 = combined & 0xFF;
    return (b0, b1, b2);
}

fn base64_encode_6bytes(data: Array(u8, 6)) -> Array(u8, 8) {
    let t0: Array(u8, 4) = base64_encode_triple(data[0], data[1], data[2]);
    let t1: Array(u8, 4) = base64_encode_triple(data[3], data[4], data[5]);
    return (t0[0], t0[1], t0[2], t0[3], t1[0], t1[1], t1[2], t1[3]);
}

fn base64_decode_8chars(enc: Array(u8, 8)) -> Array(u8, 6) {
    let t0: Array(u8, 3) = base64_decode_quad(enc[0], enc[1], enc[2], enc[3]);
    let t1: Array(u8, 3) = base64_decode_quad(enc[4], enc[5], enc[6], enc[7]);
    return (t0[0], t0[1], t0[2], t1[0], t1[1], t1[2]);
}

fn base64_pad_needed(len: u64) -> u64 {
    let rem: u64 = len % 3;
    if rem == 0 {
        return 0;
    } else {
        return 3 - rem;
    };
}

fn base64_encoded_len(len: u64) -> u64 {
    let mut result: u64 = len / 3;
    result = result * 4;
    let rem: u64 = len % 3;
    if rem != 0 {
        result = result + 4;
    };
    return result;
}

fn base64_roundtrip_3bytes(a: u8, b: u8, c: u8) -> u64 {
    let enc: Array(u8, 4) = base64_encode_triple(a, b, c);
    let dec: Array(u8, 3) = base64_decode_quad(enc[0], enc[1], enc[2], enc[3]);
    let mut sum: u64 = dec[0] + dec[1] + dec[2];
    return sum;
}

fn main() -> u64 {
    let enc: Array(u8, 4) = base64_encode_triple(0x41, 0x42, 0x43);
    let dec: Array(u8, 3) = base64_decode_quad(enc[0], enc[1], enc[2], enc[3]);
    let check: u64 = dec[0] + dec[1] + dec[2];
    let pad: u64 = base64_pad_needed(7);
    let elen: u64 = base64_encoded_len(9);
    let rt: u64 = base64_roundtrip_3bytes(0x48, 0x69, 0x21);
    let enc6: Array(u8, 8) = base64_encode_6bytes((0x48, 0x65, 0x6C, 0x6C, 0x6F, 0));
    let dec6: Array(u8, 6) = base64_decode_8chars(enc6[0], enc6[1], enc6[2], enc6[3], enc6[4], enc6[5], enc6[6], enc6[7]);
    let sum6: u64 = dec6[0] + dec6[1] + dec6[2] + dec6[3] + dec6[4] + dec6[5];
    return check + pad + elen + rt + sum6;
}
