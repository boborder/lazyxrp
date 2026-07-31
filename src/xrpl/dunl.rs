//! XRPL Foundation dUNL JSON + validator manifest decoding.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::format::format_ripple_time_utc;
use super::types::{DunlSummary, DunlValidatorRow};

/// XRPL Foundation decentralized UNL publisher (read-only HTTPS).
pub const XRPLF_DUNL_URL: &str = "https://unl.xrplf.org";

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
pub fn parse_validator_manifest_b64(b64: &str) -> Option<ValidatorManifestMeta> {
    fn manifest_decode_cache() -> &'static Mutex<HashMap<String, Option<ValidatorManifestMeta>>> {
        static MANIFEST_DECODE_CACHE: OnceLock<
            Mutex<HashMap<String, Option<ValidatorManifestMeta>>>,
        > = OnceLock::new();
        MANIFEST_DECODE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

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
    blob.iter().map(|b| format!("{b:02X}")).collect()
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
    const TABLE: &[u8; 256] = &{
        let mut t = [255u8; 256];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0u8;
        while (i as usize) < chars.len() {
            t[chars[i as usize] as usize] = i;
            i += 1;
        }
        t
    };
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in input.as_bytes() {
        if b == b'=' {
            break;
        }
        if b.is_ascii_whitespace() {
            continue;
        }
        let v = TABLE[b as usize];
        if v == 255 {
            return Err(color_eyre::eyre::eyre!("dUNL invalid base64"));
        }
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
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
        assert!(dunl.expiration_utc.contains("UTC"));
        assert_eq!(dunl.validators.len(), 1);
        assert_eq!(dunl.validators[0].validation_public_key, "n");
    }

    #[test]
    fn parse_validator_manifest_extracts_domain_and_seq() {
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
}
