use std::future::Future;
use std::net::{IpAddr, SocketAddr};

use futures::StreamExt;
use reqwest::Url;
use serde_json::Value;
use std::pin::Pin;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_DEPTH: usize = 32;
const MAX_METADATA_NODES: usize = 10_000;

#[derive(Debug)]
pub(crate) struct NftImageBytes {
    pub bytes: Vec<u8>,
}

pub(crate) async fn fetch_nft_image(uri: &str) -> color_eyre::Result<NftImageBytes> {
    let (bytes, content_type) = fetch_resource(uri).await?;
    if content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("image/"))
        || image::guess_format(&bytes).is_ok()
    {
        return Ok(NftImageBytes { bytes });
    }

    let metadata: Value = serde_json::from_slice(&bytes)
        .map_err(|_| color_eyre::eyre::eyre!("NFT URI is not an image or JSON metadata"))?;
    let image_uri = find_image_uri(&metadata)
        .ok_or_else(|| color_eyre::eyre::eyre!("NFT metadata has no image field"))?;
    let (bytes, content_type) = fetch_resource(&image_uri).await?;
    if !content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("image/"))
        && image::guess_format(&bytes).is_err()
    {
        return Err(color_eyre::eyre::eyre!(
            "NFT image response is not a supported image"
        ));
    }
    Ok(NftImageBytes { bytes })
}

#[derive(Debug)]
/// Result of one guarded fetch. Non-2xx statuses are returned, not turned into errors.
pub(crate) struct Fetched {
    pub status: u16,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Fetch one untrusted URL over (possibly redirecting) HTTP hops, reading at most
/// `max_bytes`. Each hop derives its client through `client_for` — in production
/// [`client_for_url`], which re-runs the SSRF guard for every redirect target.
/// Shared by NFT image and `xrp-ledger.toml` fetches.
pub(crate) async fn fetch_via(
    mut current: Url,
    max_bytes: usize,
    client_for: impl for<'a> Fn(
        &'a Url,
    ) -> Pin<
        Box<dyn Future<Output = color_eyre::Result<reqwest::Client>> + Send + 'a>,
    >,
) -> color_eyre::Result<Fetched> {
    let limit_msg = || format!("resource exceeds {} MiB limit", max_bytes / (1024 * 1024));
    for redirect_count in 0..=5 {
        let client = client_for(&current).await?;
        let response = client.get(current.clone()).send().await?;
        if response.status().is_redirection() {
            if redirect_count == 5 {
                return Err(color_eyre::eyre::eyre!("resource exceeded redirect limit"));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| color_eyre::eyre::eyre!("redirect has no location"))?
                .to_str()
                .map_err(|_| color_eyre::eyre::eyre!("redirect location is invalid"))?;
            current = next_hop(&current, location)?;
            continue;
        }
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(color_eyre::eyre::eyre!(limit_msg()));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(color_eyre::eyre::eyre!(limit_msg()));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(Fetched {
            status,
            content_type,
            bytes,
        });
    }
    unreachable!("redirect loop returns on every iteration")
}

/// Public IPFS gateways, tried in order: a single one routinely answers 429/5xx or refuses
/// content it does not host.
const IPFS_GATEWAYS: [&str; 3] = [
    "https://ipfs.io/ipfs/",
    "https://gateway.pinata.cloud/ipfs/",
    "https://dweb.link/ipfs/",
];

/// Display form of an NFT URI: `ipfs://…` (and the other spellings `ipfs_path` accepts) become
/// the first public gateway URL so it can be opened in a browser; anything else is unchanged.
pub(crate) fn display_uri(uri: &str) -> String {
    match ipfs_path(uri) {
        Some(path) => format!("{}{path}", IPFS_GATEWAYS[0]),
        None => uri.trim().to_owned(),
    }
}

/// Gateway path (`<cid>[/sub/path][?query]`) of an IPFS reference. Accepts `ipfs://<cid>`,
/// the doubled `ipfs://ipfs/<cid>`, `/ipfs/<cid>` and a bare CID (`Qm…` / `baf…`); scheme is
/// case-insensitive. `None` for anything else.
fn ipfs_path(uri: &str) -> Option<&str> {
    let uri = uri.trim();
    let rest = if uri
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("ipfs://"))
    {
        &uri[7..]
    } else if let Some(rest) = uri.strip_prefix("/ipfs/") {
        rest
    } else if looks_like_cid(uri) {
        uri
    } else {
        return None;
    };
    let rest = rest.trim_start_matches('/');
    let rest = rest.strip_prefix("ipfs/").unwrap_or(rest);
    (!rest.is_empty()).then_some(rest)
}

/// CIDv0 (`Qm` + 44 base58) or CIDv1 base32 (`baf…`) as the first path segment.
fn looks_like_cid(uri: &str) -> bool {
    let cid = uri.split(['/', '?', '#']).next().unwrap_or("");
    let alnum = cid.bytes().all(|b| b.is_ascii_alphanumeric());
    alnum
        && ((cid.len() == 46 && cid.starts_with("Qm"))
            || (cid.len() >= 50
                && cid.starts_with("baf")
                && cid.bytes().all(|b| !b.is_ascii_uppercase())))
}

/// `data:<mime>;base64,<payload>` (self-contained on-chain metadata / images). Other `data:`
/// forms are refused.
fn decode_data_uri(uri: &str) -> color_eyre::Result<(Vec<u8>, Option<String>)> {
    use base64::Engine;
    let body = &uri[5..];
    let (meta, payload) = body
        .split_once(',')
        .ok_or_else(|| color_eyre::eyre::eyre!("data: URI has no payload"))?;
    let mime = meta
        .strip_suffix(";base64")
        .ok_or_else(|| color_eyre::eyre::eyre!("only base64 data: URIs are supported"))?;
    if payload.len() > MAX_RESPONSE_BYTES / 3 * 4 + 4 {
        return Err(color_eyre::eyre::eyre!(
            "data: URI exceeds {} MiB limit",
            MAX_RESPONSE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|_| color_eyre::eyre::eyre!("data: URI is not valid base64"))?;
    Ok((bytes, (!mime.is_empty()).then(|| mime.to_owned())))
}

async fn fetch_resource(uri: &str) -> color_eyre::Result<(Vec<u8>, Option<String>)> {
    let uri = uri.trim();
    if uri
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
    {
        return decode_data_uri(uri);
    }
    let candidates: Vec<String> = match ipfs_path(uri) {
        Some(path) => IPFS_GATEWAYS
            .iter()
            .map(|gateway| format!("{gateway}{path}"))
            .collect(),
        None => vec![uri.to_owned()],
    };
    let mut last_err = None;
    for candidate in candidates {
        match fetch_one(&candidate).await {
            Ok(found) => return Ok(found),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.expect("candidates is never empty"))
}

async fn fetch_one(uri: &str) -> color_eyre::Result<(Vec<u8>, Option<String>)> {
    let url = resolve_uri(uri)?;
    let fetched = fetch_via(url, MAX_RESPONSE_BYTES, |url| Box::pin(client_for_url(url))).await?;
    if !(200..300).contains(&fetched.status) {
        return Err(color_eyre::eyre::eyre!(
            "NFT resource returned HTTP {}",
            fetched.status
        ));
    }
    Ok((fetched.bytes, fetched.content_type))
}

/// Client for one hop of an untrusted URL: resolves the host, rejects non-public
/// addresses, and pins the connection to the checked address. Proxies are disabled:
/// a proxy would re-resolve the hostname itself and bypass the pin (DNS rebinding).
pub(crate) async fn client_for_url(url: &Url) -> color_eyre::Result<reqwest::Client> {
    let host = url
        .host_str()
        .ok_or_else(|| color_eyre::eyre::eyre!("URL has no host"))?;
    let port = url.port_or_known_default().unwrap_or(443);
    // `host_str` keeps the brackets of an IPv6 literal ("[::1]"); they are not part of the address.
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = if let Ok(ip) = bare_host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port)).await?.collect()
    };
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| is_private_or_loopback(address.ip()))
    {
        return Err(color_eyre::eyre::eyre!("URL resolves to a private host"));
    }
    Ok(reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        // Many CDNs/gateways (Wikimedia, Cloudflare-fronted IPFS gateways) reject UA-less requests.
        .user_agent(concat!("lazyxrp/", env!("CARGO_PKG_VERSION")))
        .no_proxy()
        .resolve(host, addresses[0])
        .build()?)
}

fn resolve_uri(uri: &str) -> color_eyre::Result<Url> {
    let uri = uri.trim();
    let normalized = if let Some(path) = ipfs_path(uri) {
        format!("{}{path}", IPFS_GATEWAYS[0])
    } else if let Some(path) = uri.strip_prefix("ar://") {
        format!("https://arweave.net/{}", path.trim_start_matches('/'))
    } else {
        uri.to_owned()
    };
    let url = Url::parse(&normalized)
        .map_err(|_| color_eyre::eyre::eyre!("NFT URI is not a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(color_eyre::eyre::eyre!(
            "NFT URI scheme is unsupported; use http, https, ipfs, or ar"
        ));
    }
    // url crate keeps brackets in host_str (e.g. "[::ffff:7f00:1]") — strip before parsing.
    let bare = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    if !bare.is_empty()
        && (bare.eq_ignore_ascii_case("localhost")
            || bare.parse::<IpAddr>().is_ok_and(is_private_or_loopback))
    {
        return Err(color_eyre::eyre::eyre!("NFT URI points to a private host"));
    }
    Ok(url)
}

/// Resolve a redirect `Location` against the current hop; an https resource must not
/// be handed to plain http (the body would travel unauthenticated and spoofable).
fn next_hop(current: &Url, location: &str) -> color_eyre::Result<Url> {
    let next = current.join(location)?;
    if current.scheme() == "https" && next.scheme() != "https" {
        return Err(color_eyre::eyre::eyre!(
            "redirect downgrades https to {}",
            next.scheme()
        ));
    }
    Ok(next)
}

pub(crate) fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 shared (CGNAT)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 IETF protocol
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15 benchmarking
                || o[0] >= 240 // 240.0.0.0/4 reserved
                || o[0] == 0 // 0.0.0.0/8 "this network"
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            let low_v4 = || std::net::Ipv4Addr::from((u32::from(s[6]) << 16) | u32::from(s[7]));
            // IPv4 embedded in the low 32 bits: mapped (::ffff:0:0/96), deprecated
            // compatible (::/96, minus :: and ::1) and NAT64 (64:ff9b::/96).
            if ip.to_ipv4_mapped().is_some()
                || (s[..6] == [0; 6] && !ip.is_loopback() && !ip.is_unspecified())
                || s[..6] == [0x64, 0xff9b, 0, 0, 0, 0]
            {
                return is_private_or_loopback(IpAddr::V4(low_v4()));
            }
            // 6to4 (2002::/16) carries the IPv4 in segments 1–2.
            if s[0] == 0x2002 {
                let v4 = std::net::Ipv4Addr::from((u32::from(s[1]) << 16) | u32::from(s[2]));
                return is_private_or_loopback(IpAddr::V4(v4));
            }
            ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
                || (s[0] & 0xffc0) == 0xfec0 // fec0::/10 deprecated site-local
        }
    }
}

fn find_image_uri(value: &Value) -> Option<String> {
    fn visit(value: &Value, depth: usize, nodes: &mut usize) -> Option<String> {
        *nodes = nodes.saturating_add(1);
        if depth > MAX_METADATA_DEPTH || *nodes > MAX_METADATA_NODES {
            return None;
        }
        let object = value.as_object()?;
        for key in ["image", "image_url", "imageUrl"] {
            if let Some(uri) = object.get(key).and_then(Value::as_str)
                && !uri.trim().is_empty()
            {
                return Some(uri.to_owned());
            }
        }
        object
            .values()
            .find_map(|child| visit(child, depth + 1, nodes))
    }
    visit(value, 0, &mut 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-364: IPFS spellings normalize
    /// Real-world IPFS spellings (including the doubled `ipfs://ipfs/`) all reach the same
    /// gateway path.
    #[test]
    fn ipfs_spellings_normalize_to_one_gateway_path() {
        let cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        let v1 = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";
        for uri in [
            format!("ipfs://{cid}/1.png"),
            format!("IPFS://{cid}/1.png"),
            format!("ipfs://ipfs/{cid}/1.png"),
            format!("/ipfs/{cid}/1.png"),
            format!("{cid}/1.png"),
        ] {
            assert_eq!(
                ipfs_path(&uri),
                Some(format!("{cid}/1.png").as_str()),
                "{uri}"
            );
        }
        assert_eq!(ipfs_path(v1), Some(v1));
        assert_eq!(ipfs_path("ipfs://"), None);
        assert_eq!(ipfs_path("https://example.com/Qm"), None);
        assert_eq!(ipfs_path("QmShort"), None);
        assert_eq!(ipfs_path("ipfs://日本語"), Some("日本語"));
    }

    /// TC-364: display URI via public gateway
    #[test]
    fn display_uri_converts_ipfs_to_public_gateway() {
        assert_eq!(
            display_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/1.json"),
            "https://ipfs.io/ipfs/QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/1.json"
        );
        assert_eq!(
            display_uri("https://example.com/a.png"),
            "https://example.com/a.png"
        );
    }

    /// TC-364: data URI base64 only
    #[test]
    fn data_uri_decodes_base64_only() {
        let (bytes, mime) = decode_data_uri("data:image/png;base64,aGk=").unwrap();
        assert_eq!(
            (bytes.as_slice(), mime.as_deref()),
            (b"hi".as_slice(), Some("image/png"))
        );
        assert!(decode_data_uri("data:image/svg+xml,<svg/>").is_err());
        assert!(decode_data_uri("data:image/png;base64,!!").is_err());
        assert!(decode_data_uri("data:nocomma").is_err());
    }

    #[test]
    fn resolves_ipfs_and_ar_uris() {
        assert_eq!(
            resolve_uri("ipfs://QmExample/image.png").unwrap().as_str(),
            "https://ipfs.io/ipfs/QmExample/image.png"
        );
        assert_eq!(
            resolve_uri("ar://abc123").unwrap().as_str(),
            "https://arweave.net/abc123"
        );
    }

    /// TC-177: private / reserved literals refused
    #[test]
    fn rejects_unsafe_or_unsupported_uris() {
        let err = |uri: &str| resolve_uri(uri).unwrap_err().to_string();
        assert!(err("file:///tmp/image.png").contains("scheme"));
        for uri in [
            "http://127.0.0.1/image.png",
            "http://localhost/image.png",
            "http://[::ffff:127.0.0.1]/image.png",
        ] {
            assert!(err(uri).contains("private host"), "{uri}");
        }
        for ip in [
            "10.0.0.1",
            "169.254.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "0.0.0.0",
            "255.255.255.255",
            "::",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "100.64.0.1",
            "100.100.100.200",
            "192.0.0.8",
            "198.18.0.1",
            "240.0.0.1",
            "0.1.2.3",
            "::127.0.0.1",
            "::10.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::",
            "2002:a00:1::1",
            "fec0::1",
            "feff::1",
        ] {
            assert!(
                is_private_or_loopback(ip.parse().unwrap()),
                "unsafe IP: {ip}"
            );
        }
        assert!(!is_private_or_loopback("8.8.8.8".parse().unwrap()));
        assert!(!is_private_or_loopback("100.128.0.1".parse().unwrap()));
        assert!(!is_private_or_loopback("198.20.0.1".parse().unwrap()));
        assert!(!is_private_or_loopback("2606:4700::1111".parse().unwrap()));
        assert!(!is_private_or_loopback("64:ff9b::808:808".parse().unwrap()));
        assert!(!is_private_or_loopback("2002:808:808::".parse().unwrap()));
    }

    /// TC-237: a redirect may not downgrade https → http (absolute or scheme-relative
    /// stays https); http → https and same-scheme relative hops are allowed.
    #[test]
    fn next_hop_rejects_https_downgrade() {
        let https = Url::parse("https://a.example/x/1.json").unwrap();
        assert!(next_hop(&https, "http://b.example/img.png").is_err());
        assert_eq!(
            next_hop(&https, "//b.example/i").unwrap().as_str(),
            "https://b.example/i"
        );
        assert_eq!(
            next_hop(&https, "2.png").unwrap().as_str(),
            "https://a.example/x/2.png"
        );
        let http = Url::parse("http://a.example/").unwrap();
        assert_eq!(
            next_hop(&http, "https://b.example/").unwrap().scheme(),
            "https"
        );
        assert_eq!(
            next_hop(&http, "http://b.example/").unwrap().scheme(),
            "http"
        );
    }

    #[test]
    fn finds_common_metadata_image_fields() {
        let cases = [
            (
                serde_json::json!({"properties": {"image_url": "ipfs://QmExample/image.png"}}),
                Some("ipfs://QmExample/image.png"),
            ),
            (serde_json::json!({"image": "ar://a"}), Some("ar://a")),
            (
                serde_json::json!({"imageUrl": "https://x.example/i.png"}),
                Some("https://x.example/i.png"),
            ),
            // Blank values are skipped in favour of the next usable key.
            (
                serde_json::json!({"image": "  ", "image_url": "", "imageUrl": "ar://b"}),
                Some("ar://b"),
            ),
            (serde_json::json!({"image": "   "}), None),
        ];
        for (metadata, expected) in cases {
            assert_eq!(find_image_uri(&metadata).as_deref(), expected, "{metadata}");
        }
    }

    /// TC-100: NFT metadata traversal stops at depth limit
    #[test]
    fn metadata_search_stops_at_depth_limit() {
        let mut value = serde_json::json!({"image": "ipfs://too-deep"});
        for _ in 0..=MAX_METADATA_DEPTH {
            value = serde_json::json!({"nested": value});
        }
        assert!(find_image_uri(&value).is_none());
    }

    /// Serve one scripted HTTP response per accepted connection.
    async fn serve_scripted_responses(
        listener: tokio::net::TcpListener,
        respond: impl Fn(usize) -> Vec<u8> + Send + 'static,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for index in 0.. {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept()).await
            else {
                return; // client stopped making requests
            };
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await;
            if stream.write_all(&respond(index)).await.is_err() {
                break; // client hung up before the full body was read
            }
        }
    }

    /// A client factory that skips [`client_for_url`] (the loopback SSRF guard
    /// would reject the 127.0.0.1 mock before any request), while counting
    /// per-hop invocations to prove every redirect hop re-derives its client.
    fn mock_client_for(
        hops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> impl for<'a> Fn(
        &'a Url,
    ) -> Pin<
        Box<dyn Future<Output = color_eyre::Result<reqwest::Client>> + Send + 'a>,
    > {
        move |_url: &_| {
            use std::sync::atomic::Ordering;
            hops.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok::<_, color_eyre::Report>(
                    reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()?,
                )
            })
        }
    }

    /// Same as [`mock_client_for`] but without per-hop instrumentation, for
    /// tests that only observe fetch outcomes.
    fn plain_client_for() -> impl for<'a> Fn(
        &'a Url,
    ) -> Pin<
        Box<dyn Future<Output = color_eyre::Result<reqwest::Client>> + Send + 'a>,
    > {
        move |_url: &_| {
            Box::pin(async {
                Ok::<_, color_eyre::Report>(
                    reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()?,
                )
            })
        }
    }

    fn plain_response(head: String, body: &[u8]) -> Vec<u8> {
        let mut response = head.into_bytes();
        response.extend_from_slice(body);
        response
    }

    /// TC-146: a redirect chain is followed hop by hop, each hop gets a fresh
    /// client (per-hop `client_for_url` re-verification), and a chain longer
    /// than the limit fails with the redirect-limit error.
    #[tokio::test]
    async fn fetch_resource_follows_redirects_until_limit() {
        // Boundary first: 5 redirects then success is still accepted.
        {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(serve_scripted_responses(listener, |index| {
                if index < 5 {
                    plain_response(
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: /hop{index}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        ),
                        b"",
                    )
                } else {
                    plain_response(
                            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n".into(),
                            b"ok",
                        )
                }
            }));
            let hops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let url = Url::parse(&format!("http://{address}/start.png")).unwrap();
            let fetched = fetch_via(url, MAX_RESPONSE_BYTES, mock_client_for(hops.clone()))
                .await
                .expect("within limit");
            assert_eq!(fetched.bytes, b"ok");
            assert_eq!(fetched.content_type.as_deref(), Some("image/png"));
            assert_eq!(
                hops.load(std::sync::atomic::Ordering::SeqCst),
                6,
                "one fresh client per hop: 5 redirect hops + the final hop"
            );
            server.abort();
        }

        // One more redirect in the chain trips the limit.
        {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(serve_scripted_responses(listener, |index| {
                plain_response(
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: /hop{index}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    b"",
                )
            }));
            let hops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let url = Url::parse(&format!("http://{address}/start.png")).unwrap();
            let err = fetch_via(url, MAX_RESPONSE_BYTES, mock_client_for(hops.clone()))
                .await
                .expect_err("redirect chain beyond the limit must fail");
            assert!(err.to_string().contains("redirect limit"), "{err}");
            assert_eq!(
                hops.load(std::sync::atomic::Ordering::SeqCst),
                6,
                "the client factory must run once per hop, including the rejected hop"
            );
            server.abort();
        }
    }

    /// TC-147: responses declaring more than 4 MiB (via Content-Length or via
    /// an over-long body without Content-Length) are refused.
    #[tokio::test]
    async fn fetch_resource_rejects_responses_over_4mib() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_scripted_responses(listener, |index| match index {
            // Declared size already over the cap: reject before streaming.
            0 => plain_response(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n",
                    MAX_RESPONSE_BYTES + 1
                ),
                b"",
            ),
            // Undeclared size: the streamed body itself trips the cap.
            _ => {
                let mut response =
                    b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n"
                        .to_vec();
                response.resize(response.len() + MAX_RESPONSE_BYTES + 1, b'x');
                response
            }
        }));

        let url = Url::parse(&format!("http://{address}/huge.png")).unwrap();
        let err = fetch_via(url.clone(), MAX_RESPONSE_BYTES, plain_client_for())
            .await
            .expect_err("declared over-cap body must fail");
        assert!(err.to_string().contains("4 MiB"), "{err}");
        let err = fetch_via(url, MAX_RESPONSE_BYTES, plain_client_for())
            .await
            .expect_err("streamed over-cap body must fail");
        assert!(err.to_string().contains("4 MiB"), "{err}");
        server.abort();
    }

    /// Exactly `MAX_RESPONSE_BYTES` is accepted (the cap is exclusive of the boundary).
    #[tokio::test]
    async fn fetch_via_accepts_body_of_exactly_the_cap() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_scripted_responses(listener, |_| {
            plain_response(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {MAX_RESPONSE_BYTES}\r\nConnection: close\r\n\r\n"
                ),
                &vec![b'x'; MAX_RESPONSE_BYTES],
            )
        }));
        let url = Url::parse(&format!("http://{address}/max.png")).unwrap();
        let fetched = fetch_via(url, MAX_RESPONSE_BYTES, plain_client_for())
            .await
            .expect("exactly the cap is accepted");
        assert_eq!(fetched.bytes.len(), MAX_RESPONSE_BYTES);
        server.abort();
    }

    /// TC-146: the SSRF guard runs on a redirect hop through the real `client_for_url`:
    /// a public-looking first hop that 302s to 127.0.0.1 is refused before any request.
    #[tokio::test]
    async fn redirect_to_loopback_is_refused_by_real_client_factory() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_scripted_responses(listener, |_| {
            plain_response(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                b"",
            )
        }));
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let url = Url::parse(&format!("http://{address}/start.png")).unwrap();
        // Hop 0 uses a permissive client (the mock server is on loopback); every later hop
        // goes through the production guard.
        let err = fetch_via(url, MAX_RESPONSE_BYTES, |url| {
            let first = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            Box::pin(async move {
                if first {
                    Ok(reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()?)
                } else {
                    client_for_url(url).await
                }
            })
        })
        .await
        .expect_err("redirect to loopback must be refused");
        assert!(err.to_string().contains("private host"), "{err}");
        server.abort();
    }

    /// `localhost` resolves (offline) to loopback, so the DNS-resolving guard refuses it.
    #[tokio::test]
    async fn client_for_url_refuses_hostname_resolving_to_loopback() {
        let url = Url::parse("http://localhost:80/x.png").unwrap();
        let err = client_for_url(&url).await.expect_err("localhost refused");
        assert!(err.to_string().contains("private host"), "{err}");
    }

    /// TC-358: IPv6 literal NFT URIs
    /// IPv6 literals keep their brackets in `host_str`: a private one is refused as private (not
    /// a DNS failure) and a public one passes the guard without any lookup.
    #[tokio::test]
    async fn client_for_url_handles_ipv6_literals() {
        for private in ["http://[::1]/x.png", "http://[fd00::1]/x.png"] {
            let err = client_for_url(&Url::parse(private).unwrap())
                .await
                .expect_err(private);
            assert!(err.to_string().contains("private host"), "{private}: {err}");
        }
        let public = Url::parse("http://[2606:4700:4700::1111]/x.png").unwrap();
        assert!(client_for_url(&public).await.is_ok());
    }
}
