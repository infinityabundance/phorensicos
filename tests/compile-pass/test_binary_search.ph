// test_binary_search.ph — binary search over sorted array
// Tests: loop bound, if/else, comparisons, Array indexing, early return
package binary_search;

fn binary_search_found(arr: Array(u64, 16), target: u64) -> bool {
    let mut lo: u64 = 0;
    let mut hi: u64 = 16;
    loop bound 32 {
        if lo >= hi {
            return false;
        };
        let mid: u64 = lo + (hi - lo) / 2;
        let val: u64 = arr[mid];
        if val == target {
            return true;
        } else if val < target {
            lo = mid + 1;
        } else {
            hi = mid;
        };
    };
    return false;
}

fn binary_search_index(arr: Array(u64, 12), target: u64) -> i64 {
    let mut lo: u64 = 0;
    let mut hi: u64 = 12;
    loop bound 24 {
        if lo >= hi {
            return -1;
        };
        let mid: u64 = lo + (hi - lo) / 2;
        let val: u64 = arr[mid];
        if val == target {
            let idx: i64 = mid;
            return idx;
        } else if val < target {
            lo = mid + 1;
        } else {
            hi = mid;
        };
    };
    return -1;
}

fn lower_bound(arr: Array(u64, 10), target: u64) -> u64 {
    let mut lo: u64 = 0;
    let mut hi: u64 = 10;
    loop bound 20 {
        if lo >= hi {
            return lo;
        };
        let mid: u64 = lo + (hi - lo) / 2;
        if arr[mid] < target {
            lo = mid + 1;
        } else {
            hi = mid;
        };
    };
    return lo;
}

fn upper_bound(arr: Array(u64, 10), target: u64) -> u64 {
    let mut lo: u64 = 0;
    let mut hi: u64 = 10;
    loop bound 20 {
        if lo >= hi {
            return lo;
        };
        let mid: u64 = lo + (hi - lo) / 2;
        if arr[mid] <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        };
    };
    return lo;
}

fn check_range(arr: Array(u64, 10), target: u64) -> u64 {
    let lo: u64 = lower_bound(arr, target);
    let hi: u64 = upper_bound(arr, target);
    return hi - lo;
}

fn main() -> u64 {
    let sorted: Array(u64, 16) = (2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32);
    let found1: bool = binary_search_found(sorted, 10);
    let found2: bool = binary_search_found(sorted, 1);
    let sorted12: Array(u64, 12) = (1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23);
    let idx: i64 = binary_search_index(sorted12, 13);
    let sorted10: Array(u64, 10) = (1, 2, 3, 3, 3, 4, 5, 6, 7, 8);
    let lb: u64 = lower_bound(sorted10, 3);
    let ub: u64 = upper_bound(sorted10, 3);
    let cnt: u64 = check_range(sorted10, 3);
    let result: u64 = lb + ub + cnt;
    return result;
}
