use std::time::Duration;

use tracing::debug;

use super::types::XrplTomlData;

/// Result of fetching `/.well-known/xrp-ledger.toml`.
#[derive(Debug, Clone)]
pub struct XrplTomlFetch {
    pub status: u16,
    pub content_type: Option<String>,
    pub raw: Option<String>,
    pub result: Result<XrplTomlData, String>,
}

/// Fetch xrp-ledger.toml and check whether `expected_pubkey` appears under [[VALIDATORS]].
pub async fn fetch_xrpl_toml_with_meta(
    domain: &str,
    expected_pubkey: &str,
    timeout: Duration,
) -> XrplTomlFetch {
    let url = format!("https://{domain}/.well-known/xrp-ledger.toml");
    debug!(%url, %expected_pubkey, "fetching xrp-ledger.toml");

    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => {
            return XrplTomlFetch {
                status: 0,
                content_type: None,
                raw: None,
                result: Err(e.to_string()),
            };
        }
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            return XrplTomlFetch {
                status: 0,
                content_type: None,
                raw: None,
                result: Err(e.to_string()),
            };
        }
    };

    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            return XrplTomlFetch {
                status,
                content_type,
                raw: None,
                result: Err(format!("HTTP {status}, body err: {e}")),
            };
        }
    };

    let result = if (200..300).contains(&status) {
        parse_xrpl_toml(&text, expected_pubkey, domain).map_err(|e| e.to_string())
    } else {
        Err(format!("HTTP {status}"))
    };

    XrplTomlFetch {
        status,
        content_type,
        raw: Some(text),
        result,
    }
}

/// Parse xrp-ledger.toml text and check whether `expected_pubkey`
/// appears under [[VALIDATORS]].
pub fn parse_xrpl_toml(
    text: &str,
    expected_pubkey: &str,
    domain: &str,
) -> color_eyre::Result<XrplTomlData> {
    let value: toml::Table = toml::from_str(text)?;
    let mut data = XrplTomlData {
        domain: domain.to_string(),
        ..XrplTomlData::default()
    };

    let validators = value
        .get("VALIDATORS")
        .and_then(|v| v.as_array())
        .map(|arr| arr.as_slice())
        .unwrap_or(&[]);

    data.validator_count = validators.len();

    for v in validators {
        if let Some(table) = v.as_table()
            && let Some(key) = table.get("public_key").and_then(|k| k.as_str())
            && key.eq_ignore_ascii_case(expected_pubkey)
        {
            data.validator_found = true;
            data.attestation = table
                .get("attestation")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string());
            break;
        }
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_finds_matching_validator() {
        let text = r#"
[[VALIDATORS]]
public_key = "n9KMm3w8EjqN3qzWGg2xZy Deactivated"
attestation = "Test"

[[VALIDATORS]]
public_key = "ABCDEF123456"
"#;
        let data = parse_xrpl_toml(text, "abcdef123456", "example.com").unwrap();
        assert!(data.validator_found);
        assert_eq!(data.validator_count, 2);
        assert!(data.attestation.is_none());
    }

    #[test]
    fn parse_reports_not_found() {
        let text = r#"[[VALIDATORS]]
public_key = "OTHER"
"#;
        let data = parse_xrpl_toml(text, "MISMATCH", "example.com").unwrap();
        assert!(!data.validator_found);
        assert_eq!(data.validator_count, 1);
    }
}
