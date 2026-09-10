// test_crc.ph — simple CRC-like hash computation
// Tests: loop bound, arithmetic, Array indexing
package crc

fn simple_hash_8(arr: Array(u64, 8)) -> u64 {
    let mut hash: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        hash = hash * 31 + arr[i];
        i = i + 1;
    };
    return hash;
}

fn polynomial_hash_8(arr: Array(u64, 8), modulus: u64) -> u64 {
    let mut hash: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        hash = hash * modulus + arr[i];
        i = i + 1;
    };
    return hash;
}

fn crc_simple_16(arr: Array(u8, 16)) -> u64 {
    let mut crc: u64 = 0;
    let mut i: u64 = 0;
    loop bound 16 {
        crc = crc + arr[i];
        crc = crc * 7;
        i = i + 1;
    };
    return crc % 256;
}

fn checksum_byte_32(arr: Array(u8, 32)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 32 {
        sum = sum + arr[i];
        i = i + 1;
    };
    return sum;
}

fn checksum_word_8(arr: Array(u64, 8)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        sum = sum + arr[i];
        i = i + 1;
    };
    return sum;
}

fn djb2_hash_8(arr: Array(u64, 8)) -> u64 {
    let mut hash: u64 = 5381;
    let mut i: u64 = 0;
    loop bound 8 {
        hash = hash * 33 + arr[i];
        i = i + 1;
    };
    return hash;
}

fn folding_hash_8(arr: Array(u64, 8), modulus: u64) -> u64 {
    let mut h1: u64 = 0;
    let mut h2: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        if i % 2 == 0 {
            h1 = h1 + arr[i];
        } else {
            h2 = h2 + arr[i];
        };
        i = i + 1;
    };
    return h1 * modulus + h2;
}

fn hash_combine(a: u64, b: u64) -> u64 {
    return a * 31 + b;
}

fn main() -> u64 {
    let check: u64 = 0;
    return check;
}
