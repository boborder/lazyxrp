pub(crate) fn next_backoff_secs(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_from_zero() {
        assert_eq!(next_backoff_secs(0), 2);
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(next_backoff_secs(2), 4);
        assert_eq!(next_backoff_secs(4), 8);
        assert_eq!(next_backoff_secs(8), 16);
        assert_eq!(next_backoff_secs(16), 32);
    }

    #[test]
    fn backoff_caps_at_60() {
        assert_eq!(next_backoff_secs(32), 60);
        assert_eq!(next_backoff_secs(60), 60);
        assert_eq!(next_backoff_secs(100), 60);
    }
}
