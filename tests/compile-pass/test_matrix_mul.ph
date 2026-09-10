// test_matrix_mul.ph — matrix multiplication simulation using Array
// Tests: triple nested loops, Array indexing, integer arithmetic
package matrix_mul;

fn mat_mul_2x2(a: Array(u64, 4), b: Array(u64, 4)) -> Array(u64, 4) {
    let mut result: Array(u64, 4) = (0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 2 {
        let mut j: u64 = 0;
        loop bound 2 {
            let mut sum: u64 = 0;
            let mut k: u64 = 0;
            loop bound 2 {
                let a_idx: u64 = i * 2 + k;
                let b_idx: u64 = k * 2 + j;
                sum = sum + a[a_idx] * b[b_idx];
                k = k + 1;
            };
            let r_idx: u64 = i * 2 + j;
            result[r_idx] = sum;
            j = j + 1;
        };
        i = i + 1;
    };
    return result;
}

fn mat_mul_3x3(a: Array(u64, 9), b: Array(u64, 9)) -> Array(u64, 9) {
    let mut result: Array(u64, 9) = (0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 3 {
        let mut j: u64 = 0;
        loop bound 3 {
            let mut sum: u64 = 0;
            let mut k: u64 = 0;
            loop bound 3 {
                let a_idx: u64 = i * 3 + k;
                let b_idx: u64 = k * 3 + j;
                sum = sum + a[a_idx] * b[b_idx];
                k = k + 1;
            };
            let r_idx: u64 = i * 3 + j;
            result[r_idx] = sum;
            j = j + 1;
        };
        i = i + 1;
    };
    return result;
}

fn mat_trace_3x3(m: Array(u64, 9)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 3 {
        let idx: u64 = i * 3 + i;
        sum = sum + m[idx];
        i = i + 1;
    };
    return sum;
}

fn mat_identity_2x2() -> Array(u64, 4) {
    return (1, 0, 0, 1);
}

fn mat_add_2x2(a: Array(u64, 4), b: Array(u64, 4)) -> Array(u64, 4) {
    let mut result: Array(u64, 4) = (0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 4 {
        result[i] = a[i] + b[i];
        i = i + 1;
    };
    return result;
}

fn mat_scale_2x2(m: Array(u64, 4), s: u64) -> Array(u64, 4) {
    let mut result: Array(u64, 4) = (0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 4 {
        result[i] = m[i] * s;
        i = i + 1;
    };
    return result;
}

fn mat_transpose_2x2(m: Array(u64, 4)) -> Array(u64, 4) {
    let a: u64 = m[0];
    let b: u64 = m[1];
    let c: u64 = m[2];
    let d: u64 = m[3];
    return (a, c, b, d);
}

fn mat_2norm_2x2(m: Array(u64, 4)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 4 {
        sum = sum + m[i] * m[i];
        i = i + 1;
    };
    return sum;
}

fn main() -> u64 {
    let a: Array(u64, 4) = (1, 2, 3, 4);
    let b: Array(u64, 4) = (5, 6, 7, 8);
    let c: Array(u64, 4) = mat_mul_2x2(a, b);
    let trace: u64 = mat_trace_3x3((1, 2, 3, 4, 5, 6, 7, 8, 9));
    let id: Array(u64, 4) = mat_identity_2x2();
    let added: Array(u64, 4) = mat_add_2x2(a, id);
    let scaled: Array(u64, 4) = mat_scale_2x2(a, 3);
    let trans: Array(u64, 4) = mat_transpose_2x2(a);
    let norm: u64 = mat_2norm_2x2(a);
    let sum: u64 = c[0] + c[1] + c[2] + c[3] + trace + added[0] + scaled[0] + trans[0] + norm;
    return sum;
}
