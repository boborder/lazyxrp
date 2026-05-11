pub(crate) fn next_backoff_secs(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(60)
    }
}
