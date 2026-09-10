// test_mandelbrot.ph — Mandelbrot set membership computation on a 4x4 grid
// Tests: nested loops, complex arithmetic simulation with i64, if/else, loop bound
package mandelbrot;

fn mandelbrot_check(cx: i64, cy: i64, max_iter: u64) -> u64 {
    let mut zx: i64 = 0;
    let mut zy: i64 = 0;
    let mut iter: u64 = 0;
    loop bound 256 {
        if iter >= max_iter {
            return max_iter;
        };
        let zx2: i64 = zx * zx;
        let zy2: i64 = zy * zy;
        if zx2 + zy2 > 400 {
            return iter;
        };
        let new_zx: i64 = zx2 - zy2 + cx;
        let new_zy: i64 = 2 * zx * zy + cy;
        zx = new_zx;
        zy = new_zy;
        iter = iter + 1;
    };
    return max_iter;
}

fn mandelbrot_grid_4x4() -> u64 {
    let mut sum: u64 = 0;
    let mut px: u64 = 0;
    loop bound 4 {
        let mut py: u64 = 0;
        loop bound 4 {
            let cx: i64 = px * 2 - 3;
            let cy: i64 = py * 2 - 3;
            let iters: u64 = mandelbrot_check(cx, cy, 16);
            sum = sum + iters;
            py = py + 1;
        };
        px = px + 1;
    };
    return sum;
}

fn mandelbrot_classify(cx: i64, cy: i64) -> u64 {
    let iters: u64 = mandelbrot_check(cx, cy, 10);
    if iters >= 10 {
        return 0;
    } else if iters >= 5 {
        return 1;
    } else if iters >= 3 {
        return 2;
    } else {
        return 3;
    };
}

fn mandelbrot_escape_velocity(cx: i64, cy: i64) -> u64 {
    let iters: u64 = mandelbrot_check(cx, cy, 32);
    return iters;
}

fn mandelbrot_bounded_region() -> u64 {
    let mut count: u64 = 0;
    let mut px: i64 = -2;
    loop bound 10 {
        let mut py: i64 = -2;
        loop bound 10 {
            let iters: u64 = mandelbrot_check(px, py, 8);
            if iters >= 8 {
                count = count + 1;
            };
            py = py + 1;
        };
        px = px + 1;
    };
    return count;
}

fn julia_check(zx: i64, zy: i64, cx: i64, cy: i64, max_iter: u64) -> u64 {
    let mut x: i64 = zx;
    let mut y: i64 = zy;
    let mut iter: u64 = 0;
    loop bound 64 {
        if iter >= max_iter {
            return max_iter;
        };
        let x2: i64 = x * x;
        let y2: i64 = y * y;
        if x2 + y2 > 400 {
            return iter;
        };
        let new_x: i64 = x2 - y2 + cx;
        let new_y: i64 = 2 * x * y + cy;
        x = new_x;
        y = new_y;
        iter = iter + 1;
    };
    return max_iter;
}

fn mandelbrot_is_member(cx: i64, cy: i64) -> bool {
    let iters: u64 = mandelbrot_check(cx, cy, 100);
    return iters >= 100;
}

fn main() -> u64 {
    let grid_sum: u64 = mandelbrot_grid_4x4();
    let cls: u64 = mandelbrot_classify(1, 1);
    let ev: u64 = mandelbrot_escape_velocity(-1, 0);
    let region: u64 = mandelbrot_bounded_region();
    let member: bool = mandelbrot_is_member(0, 0);
    let julia: u64 = julia_check(0, 0, -1, 0, 16);
    return grid_sum + cls + ev + region + julia;
}
