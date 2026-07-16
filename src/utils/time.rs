pub fn gettime() -> u32 {
    use nix::time::{ClockId, clock_gettime};
    let ts = clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();
    (ts.tv_sec() * 1000 + ts.tv_nsec() / 1_000_000) as u32
}
