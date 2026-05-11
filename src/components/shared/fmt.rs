//! Display formatters: thousands separators, drops/XRP conversion.

/// Insert thousands separators into the integer part of a numeric string.
/// Preserves any decimal portion and a leading minus.
pub fn group_digits(s: &str) -> String {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s),
    };
    let (int_part, frac_part) = match rest.find('.') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let bytes = int_part.as_bytes();
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{sign}{grouped}{frac_part}")
}

/// Format an XRP amount given as f64. Uses up to 6 decimals, trimming trailing zeros.
pub fn fmt_xrp(xrp: f64) -> String {
    let s = format!("{xrp:.6}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    let final_str = if trimmed.is_empty() || trimmed == "-" {
        "0"
    } else {
        trimmed
    };
    group_digits(final_str)
}

/// Format an integer drops value as a thousands-separated drops string.
pub fn fmt_drops(drops: u64) -> String {
    group_digits(&drops.to_string())
}

/// Format a `SystemTime` as local-time `HH:MM:SS`.
pub fn fmt_local_hms(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Convert via libc::localtime_r so we honor the user's TZ.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t_ts: libc::time_t = secs as libc::time_t;
    let result = unsafe { libc::localtime_r(&t_ts, &mut tm) };
    if result.is_null() {
        return "--:--:--".to_string();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_basic() {
        assert_eq!(group_digits("0"), "0");
        assert_eq!(group_digits("1234"), "1,234");
        assert_eq!(group_digits("1234567"), "1,234,567");
        assert_eq!(group_digits("123.45"), "123.45");
        assert_eq!(group_digits("1234567.890123"), "1,234,567.890123");
        assert_eq!(group_digits("-1234"), "-1,234");
    }

    #[test]
    fn xrp_format() {
        assert_eq!(fmt_xrp(0.0), "0");
        assert_eq!(fmt_xrp(1.5), "1.5");
        assert_eq!(fmt_xrp(55660.60415), "55,660.60415");
        assert_eq!(fmt_xrp(1_000_000.123456), "1,000,000.123456");
    }

    #[test]
    fn drops_format() {
        assert_eq!(fmt_drops(0), "0");
        assert_eq!(fmt_drops(12), "12");
        assert_eq!(fmt_drops(1_234_567), "1,234,567");
    }
}
