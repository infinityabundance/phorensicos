// test_array_ops.ph — Array type operations
// Tests: Array declaration, indexing, assignment, element-wise ops
package array_ops;

fn array_init_4() -> Array(u64, 4) {
    return (10, 20, 30, 40);
}

fn array_sum(arr: Array(u64, 8)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 8 {
        sum = sum + arr[i];
        i = i + 1;
    };
    return sum;
}

fn array_max(arr: Array(u64, 8)) -> u64 {
    let mut max_val: u64 = arr[0];
    let mut i: u64 = 1;
    loop bound 8 {
        if i >= 8 {
            return max_val;
        };
        if arr[i] > max_val {
            max_val = arr[i];
        };
        i = i + 1;
    };
    return max_val;
}

fn array_min(arr: Array(u64, 8)) -> u64 {
    let mut min_val: u64 = arr[0];
    let mut i: u64 = 1;
    loop bound 8 {
        if i >= 8 {
            return min_val;
        };
        if arr[i] < min_val {
            min_val = arr[i];
        };
        i = i + 1;
    };
    return min_val;
}

fn array_reverse(arr: Array(u64, 6)) -> Array(u64, 6) {
    let mut result: Array(u64, 6) = (0, 0, 0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 6 {
        let j: u64 = 5 - i;
        result[j] = arr[i];
        i = i + 1;
    };
    return result;
}

fn array_copy_4(arr: Array(u64, 4)) -> Array(u64, 4) {
    let mut result: Array(u64, 4) = (0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 4 {
        result[i] = arr[i];
        i = i + 1;
    };
    return result;
}

fn array_dot_product(a: Array(u64, 4), b: Array(u64, 4)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 4 {
        sum = sum + a[i] * b[i];
        i = i + 1;
    };
    return sum;
}

fn array_scale(arr: Array(u64, 5), s: u64) -> Array(u64, 5) {
    let mut result: Array(u64, 5) = (0, 0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 5 {
        result[i] = arr[i] * s;
        i = i + 1;
    };
    return result;
}

fn array_concat_3(a: Array(u64, 3), b: Array(u64, 3)) -> Array(u64, 6) {
    let mut result: Array(u64, 6) = (0, 0, 0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 3 {
        result[i] = a[i];
        i = i + 1;
    };
    let mut j: u64 = 0;
    loop bound 3 {
        result[3 + j] = b[j];
        j = j + 1;
    };
    return result;
}

fn array_swap_ends(arr: Array(u64, 6)) -> Array(u64, 6) {
    let mut result: Array(u64, 6) = (0, 0, 0, 0, 0, 0);
    let mut i: u64 = 0;
    loop bound 6 {
        result[i] = arr[5 - i];
        i = i + 1;
    };
    return result;
}

fn array_find(arr: Array(u64, 8), target: u64) -> u64 {
    let mut i: u64 = 0;
    loop bound 8 {
        if arr[i] == target {
            return i;
        };
        i = i + 1;
    };
    return 8;
}

fn main() -> u64 {
    let a: Array(u64, 8) = (1, 2, 3, 4, 5, 6, 7, 8);
    let s: u64 = array_sum(a);
    let mx: u64 = array_max(a);
    let mn: u64 = array_min(a);
    let rev: Array(u64, 6) = array_reverse((1, 2, 3, 4, 5, 6));
    let dp: u64 = array_dot_product((1, 2, 3, 4), (5, 6, 7, 8));
    let scaled: Array(u64, 5) = array_scale((1, 2, 3, 4, 5), 3);
    let sc_sum: u64 = scaled[0] + scaled[4];
    let cat: Array(u64, 6) = array_concat_3((1, 2, 3), (4, 5, 6));
    let idx: u64 = array_find(a, 5);
    return s + mx + mn + dp + sc_sum + idx + cat[0] + cat[5];
}
