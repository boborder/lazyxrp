use serde_json::Value;

pub(crate) fn next_backoff_secs(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(60)
    }
}

/// Strip control, bidi-override and invisible format characters (and cap length) from
/// remote-supplied text, so a hostile node cannot inject escape sequences or visually
/// reorder / hide text (Trojan Source style) in the TUI, error reports or CLI output.
pub(crate) fn sanitize_text(s: &str, max_chars: usize) -> String {
    s.chars()
        .filter(|&c| {
            !c.is_control()
                && !matches!(c,
                    '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
                    | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}')
        })
        .take(max_chars)
        .collect()
}

pub(crate) fn sanitize_remote(s: &str) -> String {
    sanitize_text(s, 512)
}

fn json_node_at<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
    let mut node = value;
    for key in path {
        node = node.get(*key).unwrap_or(&Value::Null);
    }
    node
}

pub(crate) fn json_str<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    json_node_at(value, path).as_str().unwrap_or_default()
}

pub(crate) fn json_u32(value: &Value, path: &[&str]) -> u32 {
    let node = json_node_at(value, path);
    node.as_u64()
        .or_else(|| node.as_str().and_then(|s| s.parse().ok()))
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// TC: backoff starts at 2, doubles each step, and caps at 60 secs.
    #[test]
    fn next_backoff_secs_progression() {
        assert_eq!(next_backoff_secs(0), 2);
        assert_eq!(next_backoff_secs(2), 4);
        assert_eq!(next_backoff_secs(4), 8);
        assert_eq!(next_backoff_secs(8), 16);
        assert_eq!(next_backoff_secs(16), 32);
        assert_eq!(next_backoff_secs(32), 60);
        assert_eq!(next_backoff_secs(60), 60);
        assert_eq!(next_backoff_secs(100), 60);
    }

    /// TC: json_str returns the string at the nested path, or "" when missing.
    #[test]
    fn json_str_path_access() {
        let present = json!({"a": {"b": "hello"}});
        assert_eq!(json_str(&present, &["a", "b"]), "hello");

        let missing = json!({"a": {}});
        assert_eq!(json_str(&missing, &["a", "b"]), "");
        assert_eq!(json_str(&missing, &["x"]), "");
    }

    /// TC: json_u32 coerces numbers and string numbers, else returns 0.
    #[test]
    fn json_u32_coercion() {
        let number = json!({"a": 42});
        assert_eq!(json_u32(&number, &["a"]), 42);
        let string_number = json!({"a": "42"});
        assert_eq!(json_u32(&string_number, &["a"]), 42);
        let non_numeric = json!({"a": "foo"});
        assert_eq!(json_u32(&non_numeric, &["a"]), 0);
        assert_eq!(json_u32(&non_numeric, &["x"]), 0);
        // Out of u32 range is "not a u32" (0), never a silently wrapped value (4294967297 → 1).
        assert_eq!(json_u32(&json!({"a": 4_294_967_295u64}), &["a"]), u32::MAX);
        assert_eq!(json_u32(&json!({"a": 4_294_967_297u64}), &["a"]), 0);
        assert_eq!(json_u32(&json!({"a": "4294967297"}), &["a"]), 0);
    }

    /// TC-236: remote text loses controls, bidi overrides and zero-width format chars
    /// (Trojan Source), keeps ordinary Unicode, and is capped by chars not bytes.
    #[test]
    fn sanitize_text_strips_controls_bidi_and_format_chars() {
        assert_eq!(sanitize_text("ab\u{1b}c\nd", 10), "abcd");
        assert_eq!(
            sanitize_text(
                "tes\u{202E}SSECCUS\u{2066}x\u{2069}\u{200B}\u{FEFF}\u{061C}",
                64
            ),
            "tesSSECCUSx"
        );
        assert_eq!(sanitize_text("あいうえお", 3), "あいう");
    }
}
