// test_ringbuf_ops.ph — RingBuf type patterns
// Tests: RingBuf(T, N) type, RingBuf in params, operations simulation
package ringbuf_ops;

type Ring16 = RingBuf(u64, 16);
type Ring8 = RingBuf(u8, 8);
type RingBuf64 = RingBuf(u64, 64);

struct RingState {
    head: u64,
    tail: u64,
    count: u64,
    capacity: u64,
}

fn ring_init() -> RingState {
    return RingState { head: 0, tail: 0, count: 0, capacity: 16 };
}

fn ring_is_empty(state: RingState) -> bool {
    return state.count == 0;
}

fn ring_is_full(state: RingState) -> bool {
    return state.count >= state.capacity;
}

fn ring_advance(pos: u64, cap: u64) -> u64 {
    let next: u64 = pos + 1;
    if next >= cap {
        return 0;
    } else {
        return next;
    };
}

fn ring_prev_pos(pos: u64, cap: u64) -> u64 {
    if pos == 0 {
        return cap - 1;
    } else {
        return pos - 1;
    };
}

fn ring_count_between(begin: u64, end: u64, cap: u64) -> u64 {
    if end >= begin {
        return end - begin;
    } else {
        return cap - begin + end;
    };
}

fn ring_available_read(state: RingState) -> u64 {
    return state.count;
}

fn ring_available_write(state: RingState) -> u64 {
    return state.capacity - state.count;
}

fn ring_push_sim(state: RingState) -> RingState {
    let mut new_state: RingState = state;
    if new_state.count < new_state.capacity {
        new_state.head = ring_advance(new_state.head, new_state.capacity);
        new_state.count = new_state.count + 1;
    };
    return new_state;
}

fn ring_pop_sim(state: RingState) -> RingState {
    let mut new_state: RingState = state;
    if new_state.count > 0 {
        new_state.tail = ring_advance(new_state.tail, new_state.capacity);
        new_state.count = new_state.count - 1;
    };
    return new_state;
}

fn ring_process_sequence() -> u64 {
    let mut state: RingState = ring_init();
    let mut total: u64 = 0;
    let mut i: u64 = 0;
    loop bound 20 {
        if i >= 20 {
            return total;
        };
        if i < 10 {
            state = ring_push_sim(state);
        } else {
            state = ring_pop_sim(state);
        };
        total = total + state.count;
        i = i + 1;
    };
    return total;
}

fn ringbuf_type_param(buf: RingBuf(u64, 8)) -> u64 {
    return 0;
}

fn ringbuf_nested(buf: RingBuf(Cap(u64), 4)) -> u64 {
    return 0;
}

fn main() -> u64 {
    let state: RingState = ring_init();
    let empty: bool = ring_is_empty(state);
    let avail_r: u64 = ring_available_read(state);
    let avail_w: u64 = ring_available_write(state);
    let pushed: RingState = ring_push_sim(state);
    let popped: RingState = ring_pop_sim(pushed);
    let seq: u64 = ring_process_sequence();
    let adv: u64 = ring_advance(15, 16);
    let prev: u64 = ring_prev_pos(0, 16);
    let cnt: u64 = ring_count_between(5, 10, 16);
    let rtp: u64 = ringbuf_type_param(0);
    let rtn: u64 = ringbuf_nested(0);
    return seq + adv + prev + cnt + rtp + rtn;
}
