//! XRPL Foundation dUNL JSON + validator manifest decoding.

use base64::Engine as _;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::format::format_ripple_time_utc;
use super::types::{DunlSummary, DunlValidatorRow};

/// XRPL Foundation decentralized UNL publisher (read-only HTTPS).
pub const XRPLF_DUNL_URL: &str = "https://unl.xrplf.org";

/// dUNL manifest changes infrequently; avoid fetching on every poll tick.
pub const DUNL_CACHE_TTL: Duration = Duration::from_secs(600);

struct DunlCacheEntry {
    fetched_at: Instant,
    summary: DunlSummary,
}

fn dunl_summary_cache() -> &'static Mutex<Option<DunlCacheEntry>> {
    static CACHE: OnceLock<Mutex<Option<DunlCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Return cached dUNL when still within [`DUNL_CACHE_TTL`].
pub(crate) fn dunl_cache_get_if_fresh() -> Option<DunlSummary> {
    let guard = dunl_summary_cache().lock().ok()?;
    guard.as_ref().and_then(|entry| {
        if entry.fetched_at.elapsed() < DUNL_CACHE_TTL {
            Some(entry.summary.clone())
        } else {
            None
        }
    })
}

pub(crate) fn dunl_cache_store(summary: DunlSummary) {
    if let Ok(mut guard) = dunl_summary_cache().lock() {
        *guard = Some(DunlCacheEntry {
            fetched_at: Instant::now(),
            summary,
        });
    }
}

#[cfg(test)]
fn dunl_summary_cache_clear() {
    if let Ok(mut guard) = dunl_summary_cache().lock() {
        *guard = None;
    }
}

/// Parsed fields from a validator manifest STObject (blob inside dUNL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatorManifestMeta {
    domain: Option<String>,
    sequence: Option<u32>,
    master_public_key: Option<String>,
}

/// Decode XRPL validator manifest (base64) for domain / sequence / master key.
///
/// Results are memoized by raw base64 (dUNL entries rarely change between polls).
fn manifest_decode_cache() -> &'static Mutex<HashMap<String, Option<ValidatorManifestMeta>>> {
    static MANIFEST_DECODE_CACHE: OnceLock<Mutex<HashMap<String, Option<ValidatorManifestMeta>>>> =
        OnceLock::new();
    MANIFEST_DECODE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
fn manifest_decode_cache_clear() {
    if let Ok(mut guard) = manifest_decode_cache().lock() {
        guard.clear();
    }
}

pub fn parse_validator_manifest_b64(b64: &str) -> Option<ValidatorManifestMeta> {
    if let Ok(guard) = manifest_decode_cache().lock()
        && let Some(hit) = guard.get(b64)
    {
        return hit.clone();
    }

    let meta = base64_decode(b64)
        .ok()
        .and_then(|bytes| parse_validator_manifest_bytes(&bytes));

    if let Ok(mut guard) = manifest_decode_cache().lock() {
        // Bound memory if publisher churns keys; rare in practice.
        if guard.len() >= 512 {
            guard.clear();
        }
        guard.insert(b64.to_string(), meta.clone());
    }
    meta
}

fn parse_validator_manifest_bytes(data: &[u8]) -> Option<ValidatorManifestMeta> {
    let mut byte_offset = 0usize;
    let mut sequence = None;
    let mut domain = None;
    let mut master_public_key = None;

    while byte_offset < data.len() {
        if data[byte_offset] == 0xE1 {
            break;
        }
        let (field_type, field_code, next) = read_st_field_header(data, byte_offset)?;
        byte_offset = next;
        match field_type {
            2 => {
                if byte_offset + 4 > data.len() {
                    return None;
                }
                let value = u32::from_be_bytes(data[byte_offset..byte_offset + 4].try_into().ok()?);
                byte_offset += 4;
                if field_code == 4 {
                    sequence = Some(value);
                }
            }
            7 => {
                let (blob, next) = read_st_vl(data, byte_offset)?;
                byte_offset = next;
                match field_code {
                    1 => master_public_key = Some(validator_key_bytes_to_hex(&blob)),
                    7 => {
                        let text = std::str::from_utf8(&blob).ok()?;
                        if !text.is_empty() {
                            domain = Some(text.to_string());
                        }
                    }
                    _ => {}
                }
            }
            _ => return None,
        }
    }

    Some(ValidatorManifestMeta {
        domain,
        sequence,
        master_public_key,
    })
}

fn read_st_field_header(data: &[u8], off: usize) -> Option<(u8, u16, usize)> {
    if off >= data.len() {
        return None;
    }
    let b0 = data[off];
    let mut pos = off + 1;
    let (field_type, field_code) = if (b0 & 0xF0) == 0 {
        if pos >= data.len() {
            return None;
        }
        let field_type = data[pos] >> 4;
        let mut field_code = u16::from(b0 & 0x0F) << 8 | u16::from(data[pos] & 0x0F);
        pos += 1;
        if field_code == 0 {
            if pos + 1 >= data.len() {
                return None;
            }
            let _field_type = data[pos];
            field_code = u16::from(data[pos + 1]);
            pos += 2;
        }
        (field_type, field_code)
    } else {
        let field_type = b0 >> 4;
        let mut field_code = u16::from(b0 & 0x0F);
        if field_code == 0 {
            if pos >= data.len() {
                return None;
            }
            field_code = u16::from(data[pos]);
            pos += 1;
        }
        (field_type, field_code)
    };
    Some((field_type, field_code, pos))
}

fn read_st_vl(data: &[u8], off: usize) -> Option<(Vec<u8>, usize)> {
    if off >= data.len() {
        return None;
    }
    let b0 = data[off];
    let mut pos = off + 1;
    let len = if b0 <= 192 {
        usize::from(b0)
    } else if b0 == 193 {
        if pos >= data.len() {
            return None;
        }
        let len = 193 + usize::from(data[pos]);
        pos += 1;
        len
    } else if b0 == 194 {
        if pos + 1 >= data.len() {
            return None;
        }
        let len = 193 + usize::from(data[pos]) + usize::from(data[pos + 1]) * 256;
        pos += 2;
        len
    } else {
        if pos >= data.len() {
            return None;
        }
        let len = (usize::from(b0) - 195) * 256 + usize::from(data[pos]);
        pos += 1;
        len
    };
    if pos + len > data.len() {
        return None;
    }
    let blob = data[pos..pos + len].to_vec();
    Some((blob, pos + len))
}

fn validator_key_bytes_to_hex(blob: &[u8]) -> String {
    hex::encode_upper(blob)
}

pub(crate) fn parse_xrplf_dunl_json(text: &str) -> color_eyre::Result<DunlSummary> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| color_eyre::eyre::eyre!("dUNL JSON parse: {e}"))?;
    let blob_b64 = root
        .get("blob")
        .and_then(Value::as_str)
        .ok_or_else(|| color_eyre::eyre::eyre!("dUNL missing blob"))?;
    let blob_bytes = base64_decode(blob_b64)?;
    let blob: Value = serde_json::from_slice(&blob_bytes)
        .map_err(|e| color_eyre::eyre::eyre!("dUNL blob decode: {e}"))?;
    let sequence = blob
        .get("sequence")
        .and_then(Value::as_u64)
        .ok_or_else(|| color_eyre::eyre::eyre!("dUNL blob missing sequence"))?;
    let expiration = blob
        .get("expiration")
        .and_then(Value::as_u64)
        .ok_or_else(|| color_eyre::eyre::eyre!("dUNL blob missing expiration"))?;
    let validators: Vec<DunlValidatorRow> = blob
        .get("validators")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let validation_public_key =
                        v.get("validation_public_key").and_then(Value::as_str)?;
                    let manifest_b64 = v.get("manifest").and_then(Value::as_str);
                    let has_manifest = manifest_b64.is_some();
                    let meta = manifest_b64.and_then(parse_validator_manifest_b64);
                    Some(DunlValidatorRow {
                        validation_public_key: validation_public_key.to_string(),
                        has_manifest,
                        domain: meta.as_ref().and_then(|m| m.domain.clone()),
                        sequence: meta.as_ref().and_then(|m| m.sequence),
                        master_public_key: meta.and_then(|m| m.master_public_key),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let validator_count = validators.len().min(u32::MAX as usize) as u32;
    Ok(DunlSummary {
        validator_count,
        sequence,
        expiration_ripple: expiration,
        expiration_utc: format_ripple_time_utc(expiration),
        validators,
    })
}

fn base64_decode(input: &str) -> color_eyre::Result<Vec<u8>> {
    // Mirror the old decoder's leniency: skip ASCII whitespace before padding.
    let cleaned: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|e| color_eyre::eyre::eyre!("dUNL invalid base64: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xrplf_dunl_fixture() {
        let sample = r#"{"blob":"eyJzZXF1ZW5jZSI6MSwiZXhwaXJhdGlvbiI6MCwidmFsaWRhdG9ycyI6W3sidmFsaWRhdGlvbl9wdWJsaWNfa2V5IjoibiIsIm1hbmlmZXN0IjoibSJ9XX0="}"#;
        let dunl = parse_xrplf_dunl_json(sample).expect("parse dUNL");
        assert_eq!(dunl.validator_count, 1);
        assert_eq!(dunl.sequence, 1);
        assert_eq!(dunl.expiration_utc, "2000-01-01 00:00 UTC");
        assert_eq!(dunl.validators.len(), 1);
        assert_eq!(dunl.validators[0].validation_public_key, "n");
    }

    #[test]
    fn dunl_cache_hit_within_ttl() {
        let _guard = DunlSummaryCacheGuard;
        dunl_summary_cache_clear();
        let sample = r#"{"blob":"eyJzZXF1ZW5jZSI6MSwiZXhwaXJhdGlvbiI6MCwidmFsaWRhdG9ycyI6W3sidmFsaWRhdGlvbl9wdWJsaWNfa2V5IjoibiIsIm1hbmlmZXN0IjoibSJ9XX0="}"#;
        let dunl = parse_xrplf_dunl_json(sample).expect("parse dUNL");
        dunl_cache_store(dunl.clone());
        assert_eq!(dunl_cache_get_if_fresh().expect("cache hit"), dunl);
        dunl_summary_cache_clear();
        assert!(dunl_cache_get_if_fresh().is_none());
    }

    /// Drops with the test: clears the shared manifest decode cache even if an assert panics.
    struct ManifestDecodeCacheGuard;
    impl Drop for ManifestDecodeCacheGuard {
        fn drop(&mut self) {
            manifest_decode_cache_clear();
        }
    }

    /// Drops with the test: clears the shared dUNL summary cache even if an assert panics.
    struct DunlSummaryCacheGuard;

    impl Drop for DunlSummaryCacheGuard {
        fn drop(&mut self) {
            dunl_summary_cache_clear();
        }
    }

    #[test]
    fn parse_validator_manifest_extracts_domain_and_seq() {
        let _guard = ManifestDecodeCacheGuard;
        let manifest_b64 = "JAAAAAFxIe0Tqvy2qHvLXQk8LvN/BEMcKREm1nQpMwUVLZd2xquk1nMhA9RioHJW8Kz6IjnHOOktbvbaHsZqwJb8otgoIu+46QbWdkYwRAIgE0pz8HpSKrUsJ8E390K8KCwmvExB00jLvqPv9LZr6roCIAl9zLWeIRSsBRIaOl5alblYMYMXrpbxJZ7t+jtbiT9Ldwd4cnAudmV0cBJADEZOQPQJcWj0zPjulcvH1o8WhQ9jrKzWV/mkXSHGjmzIiekkOzUcEnzmJXwJYWZZnA0jTLE30OYmxCRXfCm9Bg==";
        let meta = parse_validator_manifest_b64(manifest_b64).expect("manifest");
        assert_eq!(meta.domain.as_deref(), Some("xrp.vet"));
        assert_eq!(meta.sequence, Some(1));
        assert!(
            meta.master_public_key
                .as_ref()
                .is_some_and(|k| k.starts_with("ED"))
        );
    }

    /// TC-148: dUNL JSON decode errors surface as `Err` for a missing blob,
    /// invalid base64, a blob that is not JSON, and a blob missing fields.
    #[test]
    fn dunl_parse_error_paths() {
        let err = parse_xrplf_dunl_json("{}").expect_err("missing blob must fail");
        assert!(err.to_string().contains("missing blob"), "{err}");

        let err = parse_xrplf_dunl_json(r#"{"blob":"not valid base64!!"}"#)
            .expect_err("invalid base64 must fail");
        assert!(err.to_string().contains("invalid base64"), "{err}");

        let not_json = base64::engine::general_purpose::STANDARD.encode(b"definitely not json");
        let err = parse_xrplf_dunl_json(&format!(r#"{{"blob":"{not_json}"}}"#))
            .expect_err("non-JSON blob must fail");
        assert!(err.to_string().contains("blob decode"), "{err}");

        let no_sequence = base64::engine::general_purpose::STANDARD.encode(br#"{"expiration":1}"#);
        let err = parse_xrplf_dunl_json(&format!(r#"{{"blob":"{no_sequence}"}}"#))
            .expect_err("blob without sequence must fail");
        assert!(err.to_string().contains("missing sequence"), "{err}");

        let no_expiration = base64::engine::general_purpose::STANDARD.encode(br#"{"sequence":1}"#);
        let err = parse_xrplf_dunl_json(&format!(r#"{{"blob":"{no_expiration}"}}"#))
            .expect_err("blob without expiration must fail");
        assert!(err.to_string().contains("missing expiration"), "{err}");
    }

    /// TC-149: `expiration_utc` is pinned to the exact Ripple-epoch UTC string.
    #[test]
    fn dunl_expiration_utc_is_exact_ripple_epoch_string() {
        let blob = serde_json::json!({
            "sequence": 2,
            "expiration": 838_204_893_u64,
            "validators": [],
        });
        let b64 = base64::engine::general_purpose::STANDARD.encode(blob.to_string());
        let dunl = parse_xrplf_dunl_json(&format!(r#"{{"blob":"{b64}"}}"#))
            .expect("parse dUNL with known expiration");
        assert_eq!(dunl.expiration_ripple, 838_204_893);
        assert_eq!(dunl.expiration_utc, "2026-07-24 10:41 UTC");
        assert!(dunl.validators.is_empty());
    }
}
