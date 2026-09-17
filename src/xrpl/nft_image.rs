use std::future::Future;
use std::net::{IpAddr, SocketAddr};

use futures::StreamExt;
use serde_json::Value;
use std::pin::Pin;
use url::Url;

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

/// Fetch one resource over (possibly redirecting) HTTP hops. Each hop derives
/// its client through `client_for` — in production [`client_for_url`], which
/// re-runs the SSRF guard for every redirect target.
async fn fetch_via(
    mut current: Url,
    client_for: impl for<'a> Fn(
        &'a Url,
    ) -> Pin<
        Box<dyn Future<Output = color_eyre::Result<reqwest::Client>> + Send + 'a>,
    >,
) -> color_eyre::Result<(Vec<u8>, Option<String>)> {
    for redirect_count in 0..=5 {
        let client = client_for(&current).await?;
        let response = client.get(current.clone()).send().await?;
        if response.status().is_redirection() {
            if redirect_count == 5 {
                return Err(color_eyre::eyre::eyre!(
                    "NFT resource exceeded redirect limit"
                ));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| color_eyre::eyre::eyre!("NFT redirect has no location"))?
                .to_str()
                .map_err(|_| color_eyre::eyre::eyre!("NFT redirect location is invalid"))?;
            current = current.join(location)?;
            continue;
        }
        let status = response.status();
        if !status.is_success() {
            return Err(color_eyre::eyre::eyre!(
                "NFT resource returned HTTP {status}"
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(color_eyre::eyre::eyre!("NFT resource exceeds 4 MiB limit"));
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
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(color_eyre::eyre::eyre!("NFT resource exceeds 4 MiB limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok((bytes, content_type));
    }
    unreachable!("redirect loop returns on every iteration")
}

async fn fetch_resource(uri: &str) -> color_eyre::Result<(Vec<u8>, Option<String>)> {
    let url = resolve_uri(uri)?;
    fetch_via(url, |url| Box::pin(client_for_url(url))).await
}

async fn client_for_url(url: &Url) -> color_eyre::Result<reqwest::Client> {
    let host = url
        .host_str()
        .ok_or_else(|| color_eyre::eyre::eyre!("NFT URI has no host"))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port)).await?.collect()
    };
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| is_private_or_loopback(address.ip()))
    {
        return Err(color_eyre::eyre::eyre!(
            "NFT URI resolves to a private host"
        ));
    }
    Ok(reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .resolve(host, addresses[0])
        .build()?)
}

fn resolve_uri(uri: &str) -> color_eyre::Result<Url> {
    let uri = uri.trim();
    let normalized = if let Some(path) = uri.strip_prefix("ipfs://") {
        format!("https://ipfs.io/ipfs/{}", path.trim_start_matches('/'))
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

fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_private_or_loopback(IpAddr::V4(v4));
            }
            ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
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

    #[test]
    fn rejects_unsafe_or_unsupported_uris() {
        assert!(resolve_uri("file:///tmp/image.png").is_err());
        assert!(resolve_uri("http://127.0.0.1/image.png").is_err());
        assert!(resolve_uri("http://localhost/image.png").is_err());
        assert!(resolve_uri("http://[::ffff:127.0.0.1]/image.png").is_err());
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
        ] {
            assert!(
                is_private_or_loopback(ip.parse().unwrap()),
                "unsafe IP: {ip}"
            );
        }
        assert!(!is_private_or_loopback("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn finds_common_metadata_image_fields_with_limits() {
        let metadata =
            serde_json::json!({"properties": {"image_url": "ipfs://QmExample/image.png"}});
        assert_eq!(
            find_image_uri(&metadata).as_deref(),
            Some("ipfs://QmExample/image.png")
        );
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

    /// Serve one scripted HTTP response per accepted connection (client.rs:535 pattern).
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
            let (bytes, content_type) = fetch_via(url, mock_client_for(hops.clone()))
                .await
                .expect("within limit");
            assert_eq!(bytes, b"ok");
            assert_eq!(content_type.as_deref(), Some("image/png"));
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
            let err = fetch_via(url, mock_client_for(hops.clone()))
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

        let hops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = Url::parse(&format!("http://{address}/huge.png")).unwrap();
        let err = fetch_via(url.clone(), mock_client_for(hops.clone()))
            .await
            .expect_err("declared over-cap body must fail");
        assert!(err.to_string().contains("4 MiB"), "{err}");
        let err = fetch_via(url, mock_client_for(hops.clone()))
            .await
            .expect_err("streamed over-cap body must fail");
        assert!(err.to_string().contains("4 MiB"), "{err}");
        server.abort();
    }
}
