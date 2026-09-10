// test_utf8_decode.ph — UTF-8 decoding and validation simulation
// Tests: byte arithmetic, bitwise ops, Array indexing, if/else chains
package utf8_decode;

fn utf8_byte_count(lead: u8) -> u64 {
    if lead < 0x80 {
        return 1;
    } else if lead < 0xE0 {
        return 2;
    } else if lead < 0xF0 {
        return 3;
    } else {
        return 4;
    };
}

fn utf8_decode_codepoint(bytes: Array(u8, 4), len: u64) -> u64 {
    if len == 1 {
        let cp: u64 = bytes[0];
        return cp;
    } else if len == 2 {
        let b0: u64 = bytes[0] & 0x1F;
        let b1: u64 = bytes[1] & 0x3F;
        return (b0 << 6) | b1;
    } else if len == 3 {
        let b0: u64 = bytes[0] & 0x0F;
        let b1: u64 = bytes[1] & 0x3F;
        let b2: u64 = bytes[2] & 0x3F;
        return (b0 << 12) | (b1 << 6) | b2;
    } else {
        let b0: u64 = bytes[0] & 0x07;
        let b1: u64 = bytes[1] & 0x3F;
        let b2: u64 = bytes[2] & 0x3F;
        let b3: u64 = bytes[3] & 0x3F;
        return (b0 << 18) | (b1 << 12) | (b2 << 6) | b3;
    };
}

fn utf8_validate_sequence(bytes: Array(u8, 4), len: u64) -> bool {
    if len == 0 {
        return false;
    };
    let mut i: u64 = 1;
    loop bound 4 {
        if i >= len {
            return true;
        };
        let b: u8 = bytes[i];
        if b & 0xC0 != 0x80 {
            return false;
        };
        i = i + 1;
    };
    return true;
}

fn utf8_continuation_byte(b: u8) -> bool {
    let high: u8 = b & 0xC0;
    return high == 0x80;
}

fn utf8_is_overlong(bytes: Array(u8, 4), len: u64) -> bool {
    if len == 2 {
        return bytes[0] < 0xC2;
    } else if len == 3 {
        return bytes[0] == 0xE0 && bytes[1] < 0xA0;
    } else if len == 4 {
        return bytes[0] == 0xF0 && bytes[1] < 0x90;
    };
    return false;
}

fn utf8_simple_hash(data: Array(u8, 8)) -> u64 {
    let mut hash: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        let byte: u64 = data[i];
        hash = hash * 31 + byte;
        i = i + 1;
    };
    return hash;
}

fn utf8_count_leading_ones(b: u8) -> u64 {
    let mut count: u64 = 0;
    let mut mask: u8 = 0x80;
    loop bound 8 {
        if mask == 0 {
            return count;
        };
        if b & mask != 0 {
            count = count + 1;
            mask = mask >> 1;
        } else {
            return count;
        };
    };
    return count;
}

fn utf8_max_codepoint(len: u64) -> u64 {
    if len == 1 {
        return 0x7F;
    } else if len == 2 {
        return 0x7FF;
    } else if len == 3 {
        return 0xFFFF;
    } else {
        return 0x10FFFF;
    };
}

fn main() -> u64 {
    let ascii_byte: u8 = 0x41;
    let len: u64 = utf8_byte_count(ascii_byte);
    let cp: u64 = utf8_decode_codepoint((0x41, 0, 0, 0), 1);
    let valid: bool = utf8_validate_sequence((0x41, 0, 0, 0), 1);
    let cont: bool = utf8_continuation_byte(0x80);
    let overlong: bool = utf8_is_overlong((0xC0, 0x80, 0, 0), 2);
    let hash: u64 = utf8_simple_hash((0x48, 0x65, 0x6C, 0x6C, 0x6F, 0, 0, 0));
    let ones: u64 = utf8_count_leading_ones(0xF0);
    let max: u64 = utf8_max_codepoint(4);
    let result: u64 = len + cp + hash + ones + max;
    return result;
}
