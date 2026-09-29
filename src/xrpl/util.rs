use serde_json::Value;

pub(crate) fn next_backoff_secs(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(60)
    }
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
        .unwrap_or_default() as u32
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
    }
}
