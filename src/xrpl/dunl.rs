//! XRPL Foundation dUNL JSON + validator manifest decoding.

use base64::Engine as _;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use super::format::format_ripple_time_utc;
use super::types::{DunlSummary, DunlValidatorRow};

/// XRPL Foundation decentralized UNL publisher (read-only HTTPS).
pub const XRPLF_DUNL_URL: &str = "https://unl.xrplf.org";

/// dUNL manifest changes infrequently; avoid fetching on every poll tick.
pub const DUNL_CACHE_TTL: Duration = Duration::from_secs(600);

/// After a failed refresh, wait this long before retrying the network;
/// meanwhile stale-on-error serves the cached summary.
pub const DUNL_RETRY_BACKOFF: Duration = Duration::from_secs(60);

struct DunlCacheEntry {
    fetched_at: Instant,
    last_failed_at: Option<Instant>,
    summary: DunlSummary,
}

fn dunl_summary_cache() -> &'static Mutex<Option<DunlCacheEntry>> {
    static CACHE: OnceLock<Mutex<Option<DunlCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Cached dUNL while within [`DUNL_CACHE_TTL`], or (stale-on-error) while a failed refresh is
/// still in its [`DUNL_RETRY_BACKOFF`] cooldown.
pub(crate) fn dunl_cache_get() -> Option<DunlSummary> {
    let guard = dunl_summary_cache().lock().ok()?;
    guard.as_ref().and_then(|entry| {
        let fresh = entry.fetched_at.elapsed() < DUNL_CACHE_TTL;
        let cooling_down = entry
            .last_failed_at
            .is_some_and(|failed_at| failed_at.elapsed() < DUNL_RETRY_BACKOFF);
        (fresh || cooling_down).then(|| entry.summary.clone())
    })
}

/// Record a failed refresh (existing entry only — no entry means nothing to serve stale).
pub(crate) fn dunl_cache_mark_failed() {
    if let Ok(mut guard) = dunl_summary_cache().lock()
        && let Some(entry) = guard.as_mut()
    {
        entry.last_failed_at = Some(Instant::now());
    }
}

pub(crate) fn dunl_cache_store(summary: DunlSummary) {
    if let Ok(mut guard) = dunl_summary_cache().lock() {
        *guard = Some(DunlCacheEntry {
            fetched_at: Instant::now(),
            last_failed_at: None,
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

/// XRPL field ID: `(type, code, next offset)`. One byte when both are < 16; otherwise the
/// big one(s) follow: type ≥ 16 → `0x0c` + type byte, code ≥ 16 → `0xt0` + code byte, both →
/// `0x00`, type, code. Extension bytes must be ≥ 16 (canonical form); truncated input is `None`.
fn read_st_field_header(data: &[u8], off: usize) -> Option<(u16, u16, usize)> {
    let b0 = *data.get(off)?;
    let (hi, lo) = (u16::from(b0 >> 4), u16::from(b0 & 0x0F));
    let ext = |i: usize| data.get(i).copied().filter(|b| *b >= 16).map(u16::from);
    match (hi, lo) {
        (0, 0) => Some((ext(off + 1)?, ext(off + 2)?, off + 3)),
        (0, code) => Some((ext(off + 1)?, code, off + 2)),
        (field_type, 0) => Some((field_type, ext(off + 1)?, off + 2)),
        (field_type, code) => Some((field_type, code, off + 1)),
    }
}

/// XRPL variable-length prefix: `0..=192` → 1 byte; `193..=240` → 2 bytes, length
/// `193 + (b0-193)*256 + b1` (≤ 12480); `241..=254` → 3 bytes, length
/// `12481 + (b0-241)*65536 + b1*256 + b2` (≤ 918744); `255` is invalid.
fn read_st_vl(data: &[u8], off: usize) -> Option<(Vec<u8>, usize)> {
    let b0 = usize::from(*data.get(off)?);
    let byte = |i: usize| data.get(off + i).copied().map(usize::from);
    let (len, pos) = match b0 {
        0..=192 => (b0, off + 1),
        193..=240 => (193 + (b0 - 193) * 256 + byte(1)?, off + 2),
        241..=254 => (
            12481 + (b0 - 241) * 65536 + byte(1)? * 256 + byte(2)?,
            off + 3,
        ),
        _ => return None,
    };
    let blob = data.get(pos..pos.checked_add(len)?)?.to_vec();
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
        fetched_at: SystemTime::now(),
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

    /// Minimal valid dUNL JSON: one validator, sequence 1.
    fn dunl_sample() -> &'static str {
        r#"{"blob":"eyJzZXF1ZW5jZSI6MSwiZXhwaXJhdGlvbiI6MCwidmFsaWRhdG9ycyI6W3sidmFsaWRhdGlvbl9wdWJsaWNfa2V5IjoibiIsIm1hbmlmZXN0IjoibSJ9XX0="}"#
    }

    /// TC-123: dUNL cache — fresh hit within TTL, stale entry served during failure cooldown,
    /// none after clear.
    #[test]
    fn dunl_cache_get_hit_and_miss() {
        let _guard = DunlSummaryCacheGuard;
        let dunl = parse_xrplf_dunl_json(dunl_sample()).expect("parse dUNL");

        // Fresh within TTL: hit; cleared: miss.
        dunl_summary_cache_clear();
        dunl_cache_store(dunl.clone());
        assert_eq!(dunl_cache_get().expect("cache hit"), dunl);
        dunl_summary_cache_clear();
        assert!(dunl_cache_get().is_none());

        // Failure cooldown serves the stale entry; cleared: none.
        dunl_summary_cache_clear();
        dunl_cache_store(dunl.clone());
        dunl_cache_mark_failed();
        assert_eq!(dunl_cache_get().expect("stale served"), dunl);
        dunl_summary_cache_clear();
        dunl_cache_mark_failed();
        assert!(dunl_cache_get().is_none());

        // Expired TTL without a failure cooldown: miss. Expired TTL + live cooldown: stale hit.
        // Expired TTL + expired cooldown: miss.
        let age = |entry_age: Duration, failed_ago: Option<Duration>| {
            let mut guard = dunl_summary_cache().lock().unwrap();
            let entry = guard.as_mut().expect("entry");
            entry.fetched_at = Instant::now().checked_sub(entry_age).expect("clock");
            entry.last_failed_at =
                failed_ago.map(|d| Instant::now().checked_sub(d).expect("clock"));
        };
        dunl_summary_cache_clear();
        dunl_cache_store(dunl.clone());
        let past_ttl = DUNL_CACHE_TTL + Duration::from_secs(1);
        age(past_ttl, None);
        assert!(dunl_cache_get().is_none(), "expired TTL must miss");
        age(past_ttl, Some(Duration::from_secs(1)));
        assert_eq!(dunl_cache_get().expect("cooldown serves stale"), dunl);
        age(past_ttl, Some(DUNL_RETRY_BACKOFF + Duration::from_secs(1)));
        assert!(dunl_cache_get().is_none(), "expired cooldown must miss");
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

    const MANIFEST_B64: &str = "JAAAAAFxIe0Tqvy2qHvLXQk8LvN/BEMcKREm1nQpMwUVLZd2xquk1nMhA9RioHJW8Kz6IjnHOOktbvbaHsZqwJb8otgoIu+46QbWdkYwRAIgE0pz8HpSKrUsJ8E390K8KCwmvExB00jLvqPv9LZr6roCIAl9zLWeIRSsBRIaOl5alblYMYMXrpbxJZ7t+jtbiT9Ldwd4cnAudmV0cBJADEZOQPQJcWj0zPjulcvH1o8WhQ9jrKzWV/mkXSHGjmzIiekkOzUcEnzmJXwJYWZZnA0jTLE30OYmxCRXfCm9Bg==";

    /// Manifest bytes come off the network: no truncation, bit flip or junk may panic the parser
    /// (xrpl-rust's `binarycodec::decode` panics on truncated input, so it is not used here).
    #[test]
    fn parse_validator_manifest_bytes_never_panics_on_bad_input() {
        let bytes = base64_decode(MANIFEST_B64).expect("fixture");
        for n in 0..=bytes.len() {
            let _ = parse_validator_manifest_bytes(&bytes[..n]);
        }
        let mut flipped = bytes.clone();
        for i in 0..flipped.len() {
            flipped[i] ^= 0xFF;
            let _ = parse_validator_manifest_bytes(&flipped);
            flipped[i] ^= 0xFF;
        }
        for junk in [
            &[][..],
            &[0xFF; 64],
            &[0x00; 64],
            &[0x73, 0xC1, 0xFF, 0xFF, 0xFF],
            &[0x24, 0, 0],
            &[0x71, 0xF0, 0xFF],
        ] {
            let _ = parse_validator_manifest_bytes(junk);
        }
    }

    /// Field ID vectors from the XRPL serialization spec (type/code each below or above 16).
    #[test]
    fn field_header_matches_spec_vectors() {
        for (bytes, want) in [
            (&[0x24][..], (2, 4, 1)),               // type<16, code<16
            (&[0x70, 0x12][..], (7, 18, 2)),        // code ≥ 16 (MasterSignature)
            (&[0x03, 0x10][..], (16, 3, 2)),        // type ≥ 16 (TransactionPersistentID-style)
            (&[0x00, 0x10, 0x11][..], (16, 17, 3)), // both ≥ 16
            (&[0x73, 0xFF][..], (7, 3, 1)),         // trailing bytes untouched
        ] {
            assert_eq!(read_st_field_header(bytes, 0), Some(want), "{bytes:x?}");
        }
        // Truncated or non-canonical extension bytes are rejected, never panic.
        for bad in [
            &[][..],
            &[0x70][..],
            &[0x03][..],
            &[0x00, 0x10][..],
            &[0x70, 0x05][..],
        ] {
            assert_eq!(read_st_field_header(bad, 0), None, "{bad:x?}");
        }
    }

    /// VL prefix vectors: 192 / 193 / 12480 / 12481 / 918744 boundaries, 255 invalid.
    #[test]
    fn vl_length_matches_spec_boundaries() {
        let with = |prefix: &[u8], len: usize| {
            let mut v = prefix.to_vec();
            v.extend(vec![0xAB; len]);
            read_st_vl(&v, 0).map(|(blob, next)| (blob.len(), next))
        };
        assert_eq!(with(&[0], 0), Some((0, 1)));
        assert_eq!(with(&[192], 192), Some((192, 193)));
        assert_eq!(with(&[193, 0], 193), Some((193, 195)));
        assert_eq!(with(&[193, 1], 194), Some((194, 196)));
        assert_eq!(with(&[194, 0], 449), Some((449, 451)));
        assert_eq!(with(&[240, 255], 12480), Some((12480, 12482)));
        assert_eq!(with(&[241, 0, 0], 12481), Some((12481, 12484)));
        assert_eq!(with(&[254, 212, 23], 918_744), Some((918_744, 918_747)));
        assert_eq!(with(&[255], 10), None);
        // Declared length longer than the data / truncated prefix.
        assert_eq!(with(&[193, 0], 192), None);
        assert_eq!(with(&[241, 0], 0), None);
    }

    #[test]
    fn parse_validator_manifest_extracts_domain_and_seq() {
        let _guard = ManifestDecodeCacheGuard;
        let manifest_b64 = MANIFEST_B64;
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

        // Minimal fixture: epoch 0, one validator whose manifest is undecodable.
        let dunl = parse_xrplf_dunl_json(dunl_sample()).expect("parse dUNL fixture");
        assert_eq!(dunl.expiration_utc, "2000-01-01 00:00 UTC");
        assert_eq!(dunl.sequence, 1);
        assert_eq!(dunl.validator_count, 1);
        assert_eq!(dunl.validators[0].validation_public_key, "n");
        assert!(dunl.validators[0].has_manifest);
        assert_eq!(dunl.validators[0].domain, None);
    }
}
