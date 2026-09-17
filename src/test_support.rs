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
