use serde_json::Value;

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

pub(crate) fn extract_json_u32(value: &Value, path: &[&str]) -> u32 {
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
    fn extract_json_u32_returns_number() {
        let v = json!({"a": 42});
        assert_eq!(extract_json_u32(&v, &["a"]), 42);
    }

    #[test]
    fn extract_json_u32_missing_or_non_numeric_returns_zero() {
        let v = json!({"a": "foo"});
        assert_eq!(extract_json_u32(&v, &["a"]), 0);
        assert_eq!(extract_json_u32(&v, &["x"]), 0);
    }

    #[test]
    fn extract_json_u32_parses_string_number() {
        let v = json!({"a": "42"});
        assert_eq!(extract_json_u32(&v, &["a"]), 42);
    }
}
