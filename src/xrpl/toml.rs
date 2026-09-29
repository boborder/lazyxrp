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

/// Fetch xrp-ledger.toml and check whether `expected_pubkey` appears under `[[VALIDATORS]]`.
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
/// appears under `[[VALIDATORS]]`.
pub fn parse_xrpl_toml(
    text: &str,
    expected_pubkey: &str,
    domain: &str,
) -> color_eyre::Result<XrplTomlData> {
    let value: toml::Table = toml::from_str(text)?;
    let mut toml_data = XrplTomlData {
        domain: domain.to_string(),
        ..XrplTomlData::default()
    };

    let validators = value
        .get("VALIDATORS")
        .and_then(|v| v.as_array())
        .map(|arr| arr.as_slice())
        .unwrap_or(&[]);

    toml_data.validator_count = validators.len();

    for v in validators {
        if let Some(table) = v.as_table()
            && let Some(key) = table.get("public_key").and_then(|k| k.as_str())
            && key.eq_ignore_ascii_case(expected_pubkey)
        {
            toml_data.validator_found = true;
            toml_data.attestation = table
                .get("attestation")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string());
            break;
        }
    }

    Ok(toml_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-142: parse_xrpl_toml — a validator whose public_key matches
    /// (case-insensitive) carries its attestation through to the result.
    ///
    /// parse_xrpl_toml — a VALIDATORS table whose public_key matches
    /// (case-insensitive) is reported as found; a non-matching key is
    /// reported as not found.
    #[test]
    fn parse_xrpl_toml_matches_or_misses_validator() {
        struct Case {
            desc: &'static str,
            text: &'static str,
            key: &'static str,
            expected_found: bool,
            expected_count: usize,
            expected_attestation: Option<&'static str>,
        }
        let cases = [
            Case {
                desc: "matching validator is found among multiple",
                text: r#"
[[VALIDATORS]]
public_key = "n9KMm3w8EjqN3qzWGg2xZy Deactivated"
attestation = "Test"

[[VALIDATORS]]
public_key = "ABCDEF123456"
"#,
                key: "abcdef123456",
                expected_found: true,
                expected_count: 2,
                expected_attestation: None,
            },
            Case {
                desc: "non-matching validator is not found",
                text: r#"[[VALIDATORS]]
public_key = "OTHER"
"#,
                key: "MISMATCH",
                expected_found: false,
                expected_count: 1,
                expected_attestation: None,
            },
            Case {
                desc: "matching validator carries its attestation",
                text: r#"
[[VALIDATORS]]
public_key = "KEY1"
attestation = "sig-bytes"

[[VALIDATORS]]
public_key = "KEY2"
attestation = "other-sig"
"#,
                key: "key2",
                expected_found: true,
                expected_count: 2,
                expected_attestation: Some("other-sig"),
            },
        ];
        for case in &cases {
            let toml_data = parse_xrpl_toml(case.text, case.key, "example.com").unwrap();
            assert_eq!(
                toml_data.validator_found, case.expected_found,
                "{}",
                case.desc
            );
            assert_eq!(
                toml_data.validator_count, case.expected_count,
                "{}",
                case.desc
            );
            assert_eq!(
                toml_data.attestation.as_deref(),
                case.expected_attestation,
                "{}",
                case.desc
            );
        }
    }

    /// TC-143: parse_xrpl_toml — malformed TOML is an error and a document
    /// without a VALIDATORS section reports zero validators.
    #[test]
    fn parse_xrpl_toml_invalid_text_and_missing_validators() {
        assert!(parse_xrpl_toml("not [valid toml", "KEY", "example.com").is_err());
        let data = parse_xrpl_toml("domain = \"example.com\"", "KEY", "example.com").unwrap();
        assert!(!data.validator_found);
        assert_eq!(data.validator_count, 0);
        assert!(data.attestation.is_none());
    }
}
