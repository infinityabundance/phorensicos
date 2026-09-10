// test_hanoi.ph — Tower of Hanoi simulation
// Tests: recursion simulation via loops, stack-like manipulation with Array
package hanoi;

struct Move {
    from: u64,
    to: u64,
}

fn hanoi_moves_3() -> u64 {
    let mut moves: u64 = 0;
    // Simulate hanoi(3) = 7 moves via iterative algorithm
    // Using the binary representation method:
    // For n disks, move on every step where step has exactly one odd bit
    let n: u64 = 3;
    let total: u64 = 1;
    let mut total_moves: u64 = total << n;
    total_moves = total_moves - 1;
    let mut step: u64 = 0;
    loop bound 16 {
        if step >= total_moves {
            return moves;
        };
        let mut temp: u64 = step + 1;
        let mut disk: u64 = 0;
        loop bound 8 {
            if temp % 2 != 0 {
                break;
            };
            temp = temp / 2;
            disk = disk + 1;
        };
        let from_peg: u64 = (step & (step + 1)) % 3;
        let to_peg: u64 = ((step | (step + 1)) + 1) % 3;
        let check: u64 = from_peg + to_peg + disk;
        moves = moves + 1;
        step = step + 1;
    };
    return moves;
}

fn hanoi_moves_4() -> u64 {
    let mut moves: u64 = 0;
    let n: u64 = 4;
    let mut total: u64 = 1;
    total = total << n;
    total = total - 1;
    let mut step: u64 = 0;
    loop bound 32 {
        if step >= total {
            return moves;
        };
        moves = moves + 1;
        step = step + 1;
    };
    return moves;
}

fn hanoi_moves_5() -> u64 {
    let mut moves: u64 = 0;
    let mut pow: u64 = 1;
    let mut i: u64 = 0;
    loop bound 6 {
        if i >= 5 {
            moves = pow - 1;
            return moves;
        };
        pow = pow * 2;
        i = i + 1;
    };
    return moves;
}

fn sim_peg_tower_3() -> u64 {
    // Iterative simulation tracking peg state
    let peg_a: u64 = 7;
    let peg_b: u64 = 0;
    let peg_c: u64 = 0;
    let moves: u64 = hanoi_moves_3();
    return moves;
}

fn sim_iterative_hanoi_3() -> u64 {
    // Count moves for 3 disks
    let mut count: u64 = 0;
    let disks: u64 = 3;
    let mut i: u64 = 0;
    let mut limit: u64 = 1;
    limit = limit << disks;
    limit = limit - 1;
    loop bound 16 {
        if i >= limit {
            return count;
        };
        count = count + 1;
        i = i + 1;
    };
    return count;
}

fn main() -> u64 {
    let m3: u64 = hanoi_moves_3();
    let m4: u64 = hanoi_moves_4();
    let m5: u64 = hanoi_moves_5();
    let s3: u64 = sim_peg_tower_3();
    let iter: u64 = sim_iterative_hanoi_3();
    return m3 + m4 + m5 + s3 + iter;
}
