use std::time::Duration;

use tracing::debug;
use url::Url;

use super::nft_image::{client_for_url, fetch_via};
use super::types::XrplTomlData;

/// Real `xrp-ledger.toml` files are a few KiB.
const MAX_TOML_BYTES: usize = 1024 * 1024;

/// Result of fetching `/.well-known/xrp-ledger.toml`.
#[derive(Debug, Clone)]
pub struct XrplTomlFetch {
    pub status: u16,
    pub content_type: Option<String>,
    pub raw: Option<String>,
    pub result: Result<XrplTomlData, String>,
}

fn fetch_error(status: u16, content_type: Option<String>, msg: impl Into<String>) -> XrplTomlFetch {
    XrplTomlFetch {
        status,
        content_type,
        raw: None,
        result: Err(msg.into()),
    }
}

/// The domain comes from a remote validator manifest: accept only a plain ASCII
/// hostname (or IP literal, which the fetch guard then rejects if non-public) so
/// it cannot smuggle userinfo, port, path, or query into the URL.
fn is_plain_hostname(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
}

/// Fetch xrp-ledger.toml and check whether `expected_pubkey` appears under `[[VALIDATORS]]`.
/// Uses the same SSRF-guarded fetch as NFT images: private/loopback hosts rejected, DNS
/// pinned, every redirect hop re-checked, no proxy, body capped at 1 MiB.
pub async fn fetch_xrpl_toml_with_meta(
    domain: &str,
    expected_pubkey: &str,
    timeout: Duration,
) -> XrplTomlFetch {
    if !is_plain_hostname(domain) {
        return fetch_error(0, None, "invalid validator domain");
    }
    let Ok(url) = Url::parse(&format!("https://{domain}/.well-known/xrp-ledger.toml")) else {
        return fetch_error(0, None, "invalid validator domain");
    };
    debug!(%url, %expected_pubkey, "fetching xrp-ledger.toml");

    let fetched = match tokio::time::timeout(
        timeout,
        fetch_via(url, MAX_TOML_BYTES, |u| Box::pin(client_for_url(u))),
    )
    .await
    {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => return fetch_error(0, None, e.to_string()),
        Err(_) => return fetch_error(0, None, "timeout"),
    };

    let status = fetched.status;
    let text = String::from_utf8_lossy(&fetched.bytes).into_owned();
    let result = if (200..300).contains(&status) {
        parse_xrpl_toml(&text, expected_pubkey, domain).map_err(|e| e.to_string())
    } else {
        Err(format!("HTTP {status}"))
    };

    XrplTomlFetch {
        status,
        content_type: fetched.content_type,
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

    /// A manifest-supplied domain cannot smuggle userinfo, port, path or query into the URL.
    #[test]
    fn plain_hostname_rejects_url_metacharacters() {
        for bad in [
            "",
            "x@127.0.0.1:80/p?",
            "a.com:8443",
            "a.com/path",
            "a.com?x=1",
            "a.com#f",
            "a b.com",
            ".a.com",
            "a.com.",
            "-a.com",
            "müller.de",
        ] {
            assert!(!is_plain_hostname(bad), "accepted {bad:?}");
        }
        assert!(is_plain_hostname("example.com"));
        assert!(is_plain_hostname("val-1.example.co.uk"));
    }

    /// Non-public hosts are refused before any request (no DNS or network needed for literals).
    #[tokio::test]
    async fn fetch_refuses_private_and_invalid_domains() {
        for domain in [
            "127.0.0.1",
            "169.254.169.254",
            "10.0.0.1",
            "100.100.100.200",
            "x@127.0.0.1",
        ] {
            let r = fetch_xrpl_toml_with_meta(domain, "ED00", Duration::from_secs(2)).await;
            assert_eq!(r.status, 0, "{domain}");
            assert!(r.raw.is_none(), "{domain}");
            let err = r.result.expect_err(domain);
            assert!(
                err.contains("private") || err.contains("invalid"),
                "{domain}: {err}"
            );
        }
    }

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
