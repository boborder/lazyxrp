use std::time::Duration;

use reqwest::Url;
use tracing::debug;

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

/// Raw key bytes of a validator public key in either form: the base58 node format
/// (`nHB…`, as published in xrp-ledger.toml) or hex.
fn key_bytes(key: &str) -> Option<Vec<u8>> {
    let key = key.trim();
    xrpl::core::addresscodec::decode_node_public_key(key)
        .ok()
        .or_else(|| hex::decode(key).ok())
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

    let expected = key_bytes(expected_pubkey);
    for v in validators {
        if let Some(table) = v.as_table()
            && let Some(key) = table.get("public_key").and_then(|k| k.as_str())
            && (key.eq_ignore_ascii_case(expected_pubkey)
                || expected
                    .as_ref()
                    .is_some_and(|e| key_bytes(key).as_ref() == Some(e)))
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

    /// TC-177: toml domain plain hostname only
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

    /// TC-177: private / reserved domains refused with no request
    /// Non-public hosts are refused before any request (no DNS or network needed for literals).
    #[tokio::test]
    async fn fetch_refuses_private_and_invalid_domains() {
        for domain in [
            "127.0.0.1",
            "169.254.169.254",
            "10.0.0.1",
            "100.100.100.200",
            "x@127.0.0.1",
            "localhost",
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

    /// TC-142: parse_xrpl_toml — a validator whose public_key matches (case-insensitive) is
    /// found and carries its attestation through; a non-matching key is not found.
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

#[cfg(test)]
mod node_key_tests {
    use super::*;

    /// TC-355: base58 node key and hex compared as bytes
    /// xrp-ledger.toml lists validators in base58 node format (`nH…`); the expected key (hex from
    /// a manifest) must match it, either way round.
    #[test]
    fn toml_base58_node_key_matches_hex_expected_key() {
        const NODE_KEY: &str = "nHBidG3pZK11zQD6kpNDoAhDxH6WLGui6ZxSbUx7LSqLHsgzMPec";
        let bytes =
            xrpl::core::addresscodec::decode_node_public_key(NODE_KEY).expect("valid nH key");
        let hex_key = hex::encode_upper(&bytes);
        let toml = format!("[[VALIDATORS]]\npublic_key = \"{NODE_KEY}\"\nattestation = \"sig\"\n");
        let found = |text: &str, key: &str| {
            parse_xrpl_toml(text, key, "example.com")
                .unwrap()
                .validator_found
        };
        assert!(found(&toml, &hex_key));
        assert!(found(&toml, &hex_key.to_lowercase()));
        assert!(found(&toml, NODE_KEY));
        let hex_toml = toml.replace(NODE_KEY, &hex_key);
        assert!(found(&hex_toml, NODE_KEY));
        let mut other = bytes.clone();
        other[5] ^= 1;
        assert!(!found(&toml, &hex::encode_upper(other)));
    }
}
