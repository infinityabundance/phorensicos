// test_palindrome.ph — palindrome checking with Array types
// Tests: Array index access, while loops, loop bound, boolean logic
package palindrome

fn is_palindrome_8(arr: Array(u64, 8)) -> bool {
    let mut left: u64 = 0;
    let mut right: u64 = 7;
    loop bound 8 {
        if left >= right {
            return true;
        };
        if arr[left] != arr[right] {
            return false;
        };
        left = left + 1;
        right = right - 1;
    };
    return true;
}

fn is_palindrome_u8(arr: Array(u8, 10)) -> bool {
    let mut left: u64 = 0;
    let mut right: u64 = 9;
    loop bound 10 {
        if left >= right {
            return true;
        };
        if arr[left] != arr[right] {
            return false;
        };
        left = left + 1;
        right = right - 1;
    };
    return true;
}

fn reverse_array_8(arr: Array(u64, 8)) -> Array(u64, 8) {
    let mut left: u64 = 0;
    let mut right: u64 = 7;
    loop bound 8 {
        if left >= right {
            return arr;
        };
        let tmp: u64 = arr[left];
        arr[left] = arr[right];
        arr[right] = tmp;
        left = left + 1;
        right = right - 1;
    };
    return arr;
}

fn array_equals_8(a: Array(u64, 8), b: Array(u64, 8)) -> bool {
    let mut i: u64 = 0;
    loop bound 8 {
        if a[i] != b[i] {
            return false;
        };
        i = i + 1;
    };
    return true;
}

fn count_numeric_palindromes(limit: u64) -> u64 {
    let mut count: u64 = 0;
    let mut n: u64 = 0;
    loop bound 1000 {
        if n > limit {
            return count;
        };
        let d0: u64 = n % 10;
        let d1: u64 = n / 10 % 10;
        let d2: u64 = n / 100 % 10;
        let d3: u64 = n / 1000 % 10;
        if n < 10 {
            count = count + 1;
        } else if n < 100 {
            if d0 == d1 {
                count = count + 1;
            };
        } else if n < 1000 {
            if d0 == d2 {
                count = count + 1;
            };
        } else {
            if d0 == d3 && d1 == d2 {
                count = count + 1;
            };
        };
        n = n + 1;
    };
    return count;
}

fn main() -> u64 {
    let count: u64 = count_numeric_palindromes(100);
    return count;
}
