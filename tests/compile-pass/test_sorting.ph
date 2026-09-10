// test_sorting.ph — sorting algorithms with loops and comparisons
// Tests: nested loops, comparison, swap via assignment, loop bound
package sorting

fn bubble_sort_10(arr: Array(u64, 10)) -> Array(u64, 10) {
    let mut i: u64 = 0;
    loop bound 10 {
        let mut j: u64 = 0;
        let n: u64 = 10;
        loop bound 10 {
            let next: u64 = j + 1;
            if next < n {
                let a: u64 = arr[j];
                let b: u64 = arr[next];
                if a > b {
                    arr[j] = b;
                    arr[next] = a;
                };
            };
            j = j + 1;
        };
        i = i + 1;
    };
    return arr;
}

fn selection_sort_8(arr: Array(u64, 8)) -> Array(u64, 8) {
    let mut i: u64 = 0;
    let n: u64 = 8;
    loop bound 8 {
        if i >= n {
            return arr;
        };
        let mut min_idx: u64 = i;
        let mut j: u64 = i + 1;
        loop bound 8 {
            if j >= n {
                break;
            };
            if arr[j] < arr[min_idx] {
                min_idx = j;
            };
            j = j + 1;
        };
        if min_idx != i {
            let tmp: u64 = arr[i];
            arr[i] = arr[min_idx];
            arr[min_idx] = tmp;
        };
        i = i + 1;
    };
    return arr;
}

fn insertion_sort_8(arr: Array(u64, 8)) -> Array(u64, 8) {
    let mut i: u64 = 1;
    let n: u64 = 8;
    loop bound 8 {
        if i >= n {
            return arr;
        };
        let key: u64 = arr[i];
        let mut j: u64 = i;
        loop bound 8 {
            if j == 0 {
                break;
            };
            let prev: u64 = j - 1;
            if arr[prev] > key {
                arr[j] = arr[prev];
                j = prev;
            } else {
                break;
            };
        };
        arr[j] = key;
        i = i + 1;
    };
    return arr;
}

fn is_sorted_5(arr: Array(u64, 5)) -> bool {
    let mut i: u64 = 0;
    let n: u64 = 5;
    loop bound 5 {
        let next: u64 = i + 1;
        if next >= n {
            return true;
        };
        if arr[i] > arr[next] {
            return false;
        };
        i = i + 1;
    };
    return true;
}

fn array_sum_10(arr: Array(u64, 10)) -> u64 {
    let mut sum: u64 = 0;
    let mut i: u64 = 0;
    loop bound 10 {
        sum = sum + arr[i];
        i = i + 1;
    };
    return sum;
}

fn main() -> u64 {
    let check: u64 = 0;
    return check;
}
