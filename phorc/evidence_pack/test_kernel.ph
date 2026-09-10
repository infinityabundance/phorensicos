package kernel

fn kernel_entry(boot_info: u64, nucleus_cap: u64) -> u64 {
    return boot_info;
}
