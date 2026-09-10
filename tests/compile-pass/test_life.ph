// test_life.ph — Conway's Game of Life on 8x8 grid
// Tests: nested loops, neighbor counting, Array manipulation, boolean logic
package life;

fn count_neighbors(grid: Array(u64, 64), x: u64, y: u64) -> u64 {
    let mut count: u64 = 0;
    let mut dx: i64 = -1;
    loop bound 3 {
        let mut dy: i64 = -1;
        loop bound 3 {
            if dx == 0 && dy == 0 {
                dy = dy + 1;
                if dy >= 2 { dy = 2; };
            };
            let nx: i64 = x + dx;
            let ny: i64 = y + dy;
            if nx >= 0 && nx < 8 && ny >= 0 && ny < 8 {
                let idx: u64 = nx * 8 + ny;
                if grid[idx] != 0 {
                    count = count + 1;
                };
            };
            dy = dy + 1;
        };
        dx = dx + 1;
    };
    return count;
}

fn life_step(grid: Array(u64, 64)) -> Array(u64, 64) {
    let mut next: Array(u64, 64) = (0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);
    let mut x: u64 = 0;
    loop bound 8 {
        let mut y: u64 = 0;
        loop bound 8 {
            let idx: u64 = x * 8 + y;
            let alive: bool = grid[idx] != 0;
            let n: u64 = count_neighbors(grid, x, y);
            if alive {
                if n == 2 || n == 3 {
                    next[idx] = 1;
                } else {
                    next[idx] = 0;
                };
            } else {
                if n == 3 {
                    next[idx] = 1;
                } else {
                    next[idx] = 0;
                };
            };
            y = y + 1;
        };
        x = x + 1;
    };
    return next;
}

fn count_alive(grid: Array(u64, 64)) -> u64 {
    let mut count: u64 = 0;
    let mut i: u64 = 0;
    loop bound 64 {
        if grid[i] != 0 {
            count = count + 1;
        };
        i = i + 1;
    };
    return count;
}

fn life_run_steps(grid: Array(u64, 64), steps: u64) -> u64 {
    let mut current: Array(u64, 64) = grid;
    let mut s: u64 = 0;
    loop bound 20 {
        if s >= steps {
            return count_alive(current);
        };
        current = life_step(current);
        s = s + 1;
    };
    return count_alive(current);
}

fn make_blinker() -> Array(u64, 64) {
    let mut grid: Array(u64, 64) = (0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);
    grid[27] = 1;
    grid[28] = 1;
    grid[29] = 1;
    return grid;
}

fn make_block() -> Array(u64, 64) {
    let mut grid: Array(u64, 64) = (0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);
    grid[9] = 1;
    grid[10] = 1;
    grid[17] = 1;
    grid[18] = 1;
    return grid;
}

fn is_stable_block(grid: Array(u64, 64)) -> bool {
    let step1: Array(u64, 64) = life_step(grid);
    let mut same: bool = true;
    let mut i: u64 = 0;
    loop bound 64 {
        if grid[i] != step1[i] {
            same = false;
        };
        i = i + 1;
    };
    return same;
}

fn main() -> u64 {
    let blinker: Array(u64, 64) = make_blinker();
    let alive0: u64 = count_alive(blinker);
    let step1: Array(u64, 64) = life_step(blinker);
    let alive1: u64 = count_alive(step1);
    let block: Array(u64, 64) = make_block();
    let stable: bool = is_stable_block(block);
    let alive_after: u64 = life_run_steps(blinker, 2);
    return alive0 + alive1 + alive_after;
}
