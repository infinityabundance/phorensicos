// test_handle_gen.ph — Handle(T) generation tag patterns
// Tests: Handle(T) type, Handle in params, Handle in struct fields, Handle comparison
package handle_gen;

struct BlockDevice {
    id: u64,
    sector_size: u64,
    block_count: u64,
}

struct NetworkInterface {
    index: u64,
    mac_addr: u64,
    speed: u64,
}

struct Timer {
    interval: u64,
    remaining: u64,
}

struct HandlePair {
    first: Handle(BlockDevice),
    second: Handle(NetworkInterface),
}

fn handle_read(dev: Handle(BlockDevice), sector: u64) -> u64
    effect [io:read]
{
    let id: u64 = dev.id;
    let size: u64 = dev.sector_size;
    return id + size + sector;
}

fn handle_write(dev: Handle(BlockDevice), sector: u64, data: u64) -> u64
    effect [io:write]
{
    dev.id = sector;
    return data;
}

fn handle_net_send(nic: Handle(NetworkInterface), pkt: u64) -> u64
    effect [io:write]
{
    return nic.index + pkt;
}

fn handle_net_recv(nic: Handle(NetworkInterface)) -> u64
    effect [io:read]
{
    return nic.mac_addr;
}

fn handle_pair_sum(pair: HandlePair) -> u64 {
    let a: u64 = pair.first.id;
    let b: u64 = pair.second.index;
    return a + b;
}

fn handle_timer_check(t: Handle(Timer), now: u64) -> bool {
    let remaining: u64 = t.remaining;
    if remaining <= now {
        return true;
    } else {
        return false;
    };
}

fn handle_timer_tick(t: Handle(Timer)) -> u64 {
    let rem: u64 = t.remaining;
    if rem > 0 {
        t.remaining = rem - 1;
    };
    return t.remaining;
}

fn handle_array_param(devs: Handle(Array(BlockDevice, 4))) -> u64 {
    return 0;
}

fn handle_method_param(h: Handle(NetworkInterface)) -> u64 {
    return h.speed;
}

fn handle_cap_mix(c: Cap(u64), h: Handle(BlockDevice)) -> u64
    effect [io:read]
{
    return c + h.id;
}

fn multi_handle(a: Handle(BlockDevice), b: Handle(NetworkInterface), c: Handle(Timer)) -> u64
    effect [io:read, io:write]
{
    return a.id + b.index + c.interval;
}

fn main() -> u64 {
    let r: u64 = handle_read(0, 10);
    let w: u64 = handle_write(0, 5, 42);
    let ns: u64 = handle_net_send(0, 100);
    let nr: u64 = handle_net_recv(0);
    let tc: bool = handle_timer_check(0, 5);
    let tt: u64 = handle_timer_tick(0);
    let hm: u64 = handle_method_param(0);
    let hc: u64 = handle_cap_mix(7, 0);
    let mh: u64 = multi_handle(0, 0, 0);
    return r + w + ns + nr + tt + hm + hc + mh;
}
