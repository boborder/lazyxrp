//! Shared helpers for unit/integration tests.

use std::path::PathBuf;

/// Remove a temp directory tree on drop (panic-safe cleanup).
pub struct TempRootGuard {
    root: PathBuf,
}

impl TempRootGuard {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Drop for TempRootGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
pub static ENV_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Acquire the env-test lock (sync tests only: plain `#[test]`, no runtime).
#[cfg(test)]
pub fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK.blocking_lock()
}

/// Async variant for `#[tokio::test]` bodies that hold the env lock across
/// `.await` points.
#[cfg(test)]
pub async fn env_lock_async() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK.lock().await
}

/// RAII guard that saves a set of environment variables on creation
/// and restores them (or removes them) when dropped.
/// Use inside a test that holds `env_lock()`.
#[cfg(test)]
pub struct TestEnvGuard {
    saved: Vec<(String, Option<String>)>,
}

#[cfg(test)]
impl TestEnvGuard {
    pub fn new(keys: &[&str]) -> Self {
        let saved = keys
            .iter()
            .map(|&k| (k.to_string(), std::env::var(k).ok()))
            .collect();
        Self { saved }
    }

    pub fn set(&self, key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub fn remove(&self, key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(test)]
impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }
    }
}

/// Renders a widget/panel draw closure into a TestBackend buffer string.
pub(crate) fn render_to_string(
    width: u16,
    height: u16,
    draw: impl FnOnce(&mut ratatui::Frame),
) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("test backend");
    terminal.draw(draw).expect("draw to test backend");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// Deterministic local JSON-RPC server for write/read path tests (no public network).
#[cfg(test)]
pub(crate) mod rpc_mock {
    use std::time::Duration;

    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn read_request(stream: &mut TcpStream) -> color_eyre::Result<Value> {
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                color_eyre::eyre::bail!("local RPC client closed before request");
            }
            bytes.extend_from_slice(&chunk[..read]);
            let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let header_end = end + 4;
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + length {
                return Ok(serde_json::from_slice(
                    &bytes[header_end..header_end + length],
                )?);
            }
        }
    }

    /// Answer `expected` requests with `{"result": result(method)}`; returns request bodies in order.
    pub(crate) async fn serve(
        listener: TcpListener,
        expected: usize,
        result: fn(&str) -> Value,
    ) -> color_eyre::Result<Vec<Value>> {
        serve_raw(listener, expected, move |request| {
            json!({"result": result(request["method"].as_str().unwrap_or_default())})
        })
        .await
    }

    /// Answer `expected` JSON requests with `respond(body)` verbatim (non-JSON-RPC, e.g. faucet REST).
    pub(crate) async fn serve_raw(
        listener: TcpListener,
        expected: usize,
        respond: impl Fn(&Value) -> Value,
    ) -> color_eyre::Result<Vec<Value>> {
        let mut requests = Vec::with_capacity(expected);
        for _ in 0..expected {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(5), listener.accept()).await??;
            let request = read_request(&mut stream).await?;
            let body = respond(&request).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
            requests.push(request);
        }
        Ok(requests)
    }

    /// Method names of captured requests (order-preserving).
    pub(crate) fn methods(requests: &[Value]) -> Vec<&str> {
        requests
            .iter()
            .filter_map(|r| r["method"].as_str())
            .collect()
    }
}
