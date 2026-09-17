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

    #[test]
    fn json_str_returns_nested_string() {
        let v = json!({"a": {"b": "hello"}});
        assert_eq!(json_str(&v, &["a", "b"]), "hello");
    }

    #[test]
    fn json_str_missing_path_returns_empty() {
        let v = json!({"a": {}});
        assert_eq!(json_str(&v, &["a", "b"]), "");
        assert_eq!(json_str(&v, &["x"]), "");
    }

    #[test]
    fn json_u32_returns_number() {
        let v = json!({"a": 42});
        assert_eq!(json_u32(&v, &["a"]), 42);
    }

    #[test]
    fn json_u32_missing_or_non_numeric_returns_zero() {
        let v = json!({"a": "foo"});
        assert_eq!(json_u32(&v, &["a"]), 0);
        assert_eq!(json_u32(&v, &["x"]), 0);
    }

    #[test]
    fn json_u32_parses_string_number() {
        let v = json!({"a": "42"});
        assert_eq!(json_u32(&v, &["a"]), 42);
    }
}
