use std::{
    collections::HashMap,
    env, fmt,
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::ProjectDirs;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, de::Deserializer, de::Error as _};

use crate::{action::Action, app::Mode, network::Network};

const CONFIG: &str = include_str!("../config.json5");
const CONFIG_DIR_BASENAME: &str = "lazyxrp";

#[derive(Clone, Debug, Deserialize, Default)]
pub struct PathConfig {
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub config_dir: PathBuf,
}

fn default_flare_evm_key_env() -> String {
    "FLARE_EVM_KEY".to_string()
}

/// `[flare.fassets]` — Direct Mint execute path (C3). Default off: never Flare-writes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FlareFassetsConfig {
    /// When false (default), `executeDirectMinting` is refused.
    #[serde(default)]
    pub execute: bool,
    /// Env var name holding the Flare executor private key (never XRPL seed).
    #[serde(default = "default_flare_evm_key_env")]
    pub evm_key_env: String,
}

impl Default for FlareFassetsConfig {
    fn default() -> Self {
        Self {
            execute: false,
            evm_key_env: default_flare_evm_key_env(),
        }
    }
}

/// Top-level `[flare]` config (FTSO RPC stays env-driven; fassets is file-config).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FlareConfig {
    #[serde(default)]
    pub fassets: FlareFassetsConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default, flatten)]
    pub config: PathConfig,
    #[serde(default)]
    pub keybindings: KeyBindings,
    #[serde(default)]
    pub styles: Styles,
    #[serde(default)]
    pub xrpl: LedgerConfig,
    #[serde(default)]
    pub flare: FlareConfig,
}

/// Raw signing config as read from `[xrpl.signing]` in config.toml.
/// Pass to `SigningConfig::load()` to get memory-masked credentials.
#[derive(Clone, Default, Deserialize)]
pub struct RawSigningConfig {
    /// Signing seed (family seed format, e.g. `sXXX...`).
    /// ⚠️  Plain text on disk — prefer the `XRPL_SEED` env var instead.
    /// After `Config::new()` this is cleared to `None`; use [`secret_seed`] instead.
    #[serde(default)]
    pub seed: Option<String>,
    /// Memory-masked seed (set by `Config::new()` from env/file/CLI).
    #[serde(skip)]
    pub secret_seed: Option<secrecy::SecretString>,
}

/// Security: never print the seed value in debug output.
impl fmt::Debug for RawSigningConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawSigningConfig")
            .field("seed", &self.seed.as_ref().map(|_| "[REDACTED]"))
            .field(
                "secret_seed",
                &self.secret_seed.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

pub const FALLBACK_CURRENCY_CODE: &str = "USD";
pub const FALLBACK_ISSUER: &str = "rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

fn fallback_currency_code() -> String {
    FALLBACK_CURRENCY_CODE.to_string()
}

fn fallback_issuer() -> String {
    FALLBACK_ISSUER.to_string()
}

fn default_oracle_pairs() -> Vec<crate::xrpl::OraclePricePair> {
    vec![
        crate::xrpl::OraclePricePair {
            base_asset: "XRP".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "BTC".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "ETH".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "524C555344000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "5553444300000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "5553445400000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
    ]
}

#[derive(Clone, Debug, Deserialize)]
pub struct LedgerConfig {
    pub account: String,
    #[serde(default = "fallback_issuer")]
    pub issuer: String,
    pub currency: String,
    #[serde(default = "fallback_currency_code")]
    pub currency_code: String,
    pub offer_limit: u16,
    pub poll_interval_ms: u64,
    /// Network preset (mainnet / testnet / devnet). Determines default RPC/WS endpoints.
    #[serde(default)]
    pub network: Network,
    /// Custom RPC endpoint. Overrides the network preset when set.
    #[serde(default)]
    pub rpc_server: Option<String>,
    /// Custom WebSocket endpoint. Overrides the network preset when set.
    #[serde(default)]
    pub ws_server: Option<String>,
    /// Raw signing config (seed). Use `SigningConfig::resolve()` to access.
    #[serde(default)]
    pub signing: RawSigningConfig,
    /// Oracle identifiers for `get_aggregate_price`.
    #[serde(default)]
    pub oracles: Vec<crate::xrpl::OracleId>,
    /// Price pairs to query via `get_aggregate_price`.
    #[serde(default = "default_oracle_pairs")]
    pub oracle_pairs: Vec<crate::xrpl::OraclePricePair>,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            account: "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV".to_string(),
            issuer: "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De".to_string(),
            currency: "RLUSD".to_string(),
            currency_code: "524C555344000000000000000000000000000000".to_string(),
            offer_limit: 5,
            poll_interval_ms: 5_000,
            network: Network::default(),
            rpc_server: None,
            ws_server: None,
            signing: RawSigningConfig::default(),
            oracles: Vec::new(),
            oracle_pairs: default_oracle_pairs(),
        }
    }
}

/// Security (S-004): reject env-var-supplied paths that contain `..` traversal sequences.
fn validated_path(raw: PathBuf) -> Option<PathBuf> {
    if raw
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        // Security: path traversal via environment variable rejected
        eprintln!(
            "lazyxrp: ignoring env-var path '{}' — contains '..' traversal",
            raw.display()
        );
        return None;
    }
    Some(raw)
}

pub static PROJECT_NAME: &str = env!("CARGO_CRATE_NAME");

/// Network preset override (`mainnet` / `testnet` / `devnet`). Merged in `Config::new()` so UI matches `resolve_network` priority.
pub const XRPL_NETWORK_ENV: &str = "XRPL_NETWORK";
/// HTTP JSON-RPC endpoint override. Merged in `Config::new()` (env wins over config file) so splash and `[xrpl]` stay aligned with `resolve_rpc_url`.
pub const XRPL_RPC_SERVER_ENV: &str = "XRPL_RPC_SERVER";
/// WebSocket JSON-RPC endpoint override. Same merge rules as [`XRPL_RPC_SERVER_ENV`].
pub const XRPL_WS_SERVER_ENV: &str = "XRPL_WS_SERVER";

fn env_data_folder() -> Option<PathBuf> {
    env::var(format!("{}_DATA", PROJECT_NAME.to_uppercase()))
        .ok()
        .map(PathBuf::from)
        .and_then(validated_path)
}

fn env_config_folder() -> Option<PathBuf> {
    env::var(format!("{}_CONFIG", PROJECT_NAME.to_uppercase()))
        .ok()
        .map(PathBuf::from)
        .and_then(validated_path)
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
}

fn non_empty_path_or(path: &Path, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    if path.as_os_str().is_empty() {
        fallback()
    } else {
        path.to_path_buf()
    }
}

impl Config {
    /// Effective data directory after merging embedded defaults, env keys, and config file.
    pub fn resolved_data_dir(&self) -> PathBuf {
        non_empty_path_or(&self.config.data_dir, data_dir)
    }

    /// Effective config directory after merging embedded defaults, env keys, and config file.
    pub fn resolved_config_dir(&self) -> PathBuf {
        non_empty_path_or(&self.config.config_dir, config_dir)
    }

    pub fn new() -> color_eyre::Result<Self, config::ConfigError> {
        // Security (S-007): embedded config is a compile-time constant — parse failure
        // indicates a build-time bug, not a runtime condition.
        let default_config: Config = json5::from_str(CONFIG)
            .expect("embedded config.json5 is malformed — this is a build-time bug");
        let data_dir = data_dir();
        let config_dir = config_dir();
        let mut builder = config::Config::builder()
            .set_default("data_dir", data_dir.to_string_lossy().as_ref())?
            .set_default("config_dir", config_dir.to_string_lossy().as_ref())?;

        let config_files = [
            ("config.json5", config::FileFormat::Json5),
            ("config.json", config::FileFormat::Json),
            ("config.yaml", config::FileFormat::Yaml),
            ("config.toml", config::FileFormat::Toml),
            ("config.ini", config::FileFormat::Ini),
        ];
        let mut found_config = false;
        for (file, format) in &config_files {
            let source = config::File::from(config_dir.join(file))
                .format(*format)
                .required(false);
            builder = builder.add_source(source);
            if config_dir.join(file).exists() {
                found_config = true
            }
        }
        if !found_config {
            // Before `logging::init`, tracing may have no subscriber — surface this on stderr.
            eprintln!(
                "lazyxrp: warning: no configuration file found under {}; defaults apply",
                config_dir.display()
            );
        }

        let mut config: Self = builder.build()?.try_deserialize()?;

        // Security (S-003): if a signing seed is present in config, warn about file permissions
        // BEFORE we move it out of the plain-text field.
        let resolved_cfg_dir = config.resolved_config_dir();
        if config.xrpl.signing.seed.is_some() {
            for (file, _) in &config_files {
                let path = resolved_cfg_dir.join(file);
                if path.exists() {
                    warn_if_config_world_readable(&path);
                }
            }
        }

        // Security: promote any plain-text seed (from config file) into secret_seed
        // and wipe the field so Arc<Config> never carries an unmasked credential.
        // Must happen BEFORE the env-var merge so env (XRPL_SEED) can override file.
        if let Some(plain) = config.xrpl.signing.seed.take() {
            config.xrpl.signing.secret_seed = Some(secrecy::SecretString::from(plain));
        }

        // Merge XRPL_SEED env var into signing config (env var takes priority over file)
        if let Ok(env_seed) = env::var(crate::signing::SEED_ENV) {
            // SAFETY: Config::new runs during single-threaded startup before worker threads
            // observe the environment — clear seed from /proc/self/environ immediately.
            unsafe { env::remove_var(crate::signing::SEED_ENV) };
            let t = crate::signing::trim_family_seed(&env_seed);
            if !t.is_empty() {
                config.xrpl.signing.secret_seed = Some(secrecy::SecretString::from(t.to_string()));
            }
        }

        if let Ok(v) = env::var(XRPL_NETWORK_ENV)
            && let Ok(n) = v.parse::<Network>()
        {
            config.xrpl.network = n;
        }
        if let Ok(v) = env::var(XRPL_RPC_SERVER_ENV) {
            let t = v.trim();
            if !t.is_empty() {
                config.xrpl.rpc_server = Some(t.to_string());
            }
        }
        if let Ok(v) = env::var(XRPL_WS_SERVER_ENV) {
            let t = v.trim();
            if !t.is_empty() {
                config.xrpl.ws_server = Some(t.to_string());
            }
        }

        for (mode, default_bindings) in default_config.keybindings.0.iter() {
            let user_bindings = config.keybindings.0.entry(*mode).or_default();
            for (key, cmd) in default_bindings.iter() {
                user_bindings
                    .entry(key.clone())
                    .or_insert_with(|| cmd.clone());
            }
        }
        for (mode, default_styles) in default_config.styles.0.iter() {
            let user_styles = config.styles.0.entry(*mode).or_default();
            for (style_key, style) in default_styles.iter() {
                user_styles.entry(style_key.clone()).or_insert(*style);
            }
        }

        Ok(config)
    }
}

/// Security (S-003): warn when a config file containing a seed is group- or world-readable.
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
fn warn_if_config_world_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = path.metadata() {
        let mode = meta.permissions().mode();
        // 0o044 = group-read (0o040) | world-read (0o004)
        if mode & 0o044 != 0 {
            tracing::warn!(
                // Security: config file with seed should be readable only by owner (0600)
                "Config file '{}' has mode {:04o} and contains a signing seed. \
                 Run: chmod 600 {}",
                path.display(),
                mode & 0o777,
                path.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_config_world_readable(_path: &std::path::Path) {}

pub fn data_dir() -> PathBuf {
    if let Some(s) = env_data_folder() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".data")
    }
}

pub fn config_dir() -> PathBuf {
    if let Some(s) = env_config_folder() {
        s
    } else if let Some(xdg) = xdg_config_home() {
        xdg.join(CONFIG_DIR_BASENAME)
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs
            .config_local_dir()
            .parent()
            .map(|p| p.join(CONFIG_DIR_BASENAME))
            .unwrap_or_else(|| proj_dirs.config_local_dir().to_path_buf())
    } else {
        PathBuf::from(".").join(".config")
    }
}

fn project_directory() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "kdheepak", env!("CARGO_PKG_NAME"))
}

#[derive(Clone, Debug, Default)]
pub struct KeyBindings(pub HashMap<Mode, HashMap<Vec<KeyEvent>, Action>>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> color_eyre::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, Action>>::deserialize(deserializer)?;

        let mut keybindings = HashMap::new();
        for (mode, inner_map) in parsed_map {
            let mut converted = HashMap::new();
            for (key_str, cmd) in inner_map {
                let seq = parse_key_sequence(&key_str).map_err(D::Error::custom)?;
                converted.insert(seq, cmd);
            }
            keybindings.insert(mode, converted);
        }

        Ok(KeyBindings(keybindings))
    }
}

fn parse_key_event(raw: &str) -> color_eyre::Result<KeyEvent, String> {
    let raw_lower = raw.to_ascii_lowercase();
    let (remaining, modifiers) = extract_modifiers(&raw_lower);
    parse_key_code_with_modifiers(remaining, modifiers)
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        match current {
            rest if rest.starts_with("ctrl-") => {
                modifiers.insert(KeyModifiers::CONTROL);
                current = &rest[5..];
            }
            rest if rest.starts_with("alt-") => {
                modifiers.insert(KeyModifiers::ALT);
                current = &rest[4..];
            }
            rest if rest.starts_with("shift-") => {
                modifiers.insert(KeyModifiers::SHIFT);
                current = &rest[6..];
            }
            _ => break, // break out of the loop if no known prefix is detected
        };
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    raw: &str,
    mut modifiers: KeyModifiers,
) -> color_eyre::Result<KeyEvent, String> {
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            // SAFETY: c.len() == 1 guarantees at least one char exists.
            let mut c = c
                .chars()
                .next()
                .expect("single-byte key should have a char");
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

/// Canonical string form for key chords (matches `parse_key_event` grammar).
#[cfg_attr(not(test), allow(dead_code))]
pub fn key_event_to_string(key_event: &KeyEvent) -> String {
    let char;
    let key_code = match key_event.code {
        KeyCode::Backspace => "backspace",
        KeyCode::Enter => "enter",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pageup",
        KeyCode::PageDown => "pagedown",
        KeyCode::Tab => "tab",
        KeyCode::BackTab => "backtab",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::F(c) => {
            char = format!("f({c})");
            &char
        }
        KeyCode::Char(' ') => "space",
        KeyCode::Char(c) => {
            char = c.to_string();
            &char
        }
        KeyCode::Esc => "esc",
        KeyCode::Null => "",
        KeyCode::CapsLock => "",
        KeyCode::Menu => "",
        KeyCode::ScrollLock => "",
        KeyCode::Media(_) => "",
        KeyCode::NumLock => "",
        KeyCode::PrintScreen => "",
        KeyCode::Pause => "",
        KeyCode::KeypadBegin => "",
        KeyCode::Modifier(_) => "",
    };

    let mut modifiers = Vec::with_capacity(3);

    if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
        modifiers.push("ctrl");
    }

    if key_event.modifiers.intersects(KeyModifiers::SHIFT) {
        modifiers.push("shift");
    }

    if key_event.modifiers.intersects(KeyModifiers::ALT) {
        modifiers.push("alt");
    }

    let mut key = modifiers.join("-");

    if !key.is_empty() {
        key.push('-');
    }
    key.push_str(key_code);

    key
}

pub fn parse_key_sequence(raw: &str) -> color_eyre::Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{}`", raw));
    }
    let raw = if !raw.contains("><") {
        let without_open = raw.strip_prefix('<').unwrap_or(raw);
        without_open.strip_prefix('>').unwrap_or(without_open)
    } else {
        raw
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Styles(pub HashMap<Mode, HashMap<String, Style>>);

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, String>>::deserialize(deserializer)?;

        let styles = parsed_map
            .into_iter()
            .map(|(mode, inner_map)| {
                let converted_inner_map = inner_map
                    .into_iter()
                    .map(|(str, style)| (str, parse_style(&style)))
                    .collect();
                (mode, converted_inner_map)
            })
            .collect();

        Ok(Styles(styles))
    }
}

pub fn parse_style(line: &str) -> Style {
    let (foreground, background) =
        line.split_at(line.to_lowercase().find("on ").unwrap_or(line.len()));
    let foreground = extract_color_and_modifiers(foreground);
    let background = extract_color_and_modifiers(&background.replace("on ", ""));

    let mut style = Style::default();
    if let Some(fg) = parse_color(&foreground.0) {
        style = style.fg(fg);
    }
    if let Some(bg) = parse_color(&background.0) {
        style = style.bg(bg);
    }
    style = style.add_modifier(foreground.1 | background.1);
    style
}

fn extract_color_and_modifiers(color_str: &str) -> (String, Modifier) {
    let color = color_str
        .replace("grey", "gray")
        .replace("bright ", "")
        .replace("bold ", "")
        .replace("underline ", "")
        .replace("inverse ", "");

    let mut modifiers = Modifier::empty();
    if color_str.contains("underline") {
        modifiers |= Modifier::UNDERLINED;
    }
    if color_str.contains("bold") {
        modifiers |= Modifier::BOLD;
    }
    if color_str.contains("inverse") {
        modifiers |= Modifier::REVERSED;
    }

    (color, modifiers)
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim_start();
    let s = s.trim_end();
    if s.contains("bright color") {
        let s = s.trim_start_matches("bright ");
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c + 8))
    } else if s.contains("color") {
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("gray") {
        let c = 232
            + s.trim_start_matches("gray")
                .parse::<u8>()
                .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if let Some(rgb_str) = s.strip_prefix("rgb") {
        if rgb_str.len() >= 3 {
            let bytes = rgb_str.as_bytes();
            let red = (bytes[0] as char).to_digit(10).unwrap_or_default() as u8;
            let green = (bytes[1] as char).to_digit(10).unwrap_or_default() as u8;
            let blue = (bytes[2] as char).to_digit(10).unwrap_or_default() as u8;
            let c = 16 + red * 36 + green * 6 + blue;
            Some(Color::Indexed(c))
        } else {
            None
        }
    } else if s == "bold black" {
        Some(Color::Indexed(8))
    } else if s == "bold red" {
        Some(Color::Indexed(9))
    } else if s == "bold green" {
        Some(Color::Indexed(10))
    } else if s == "bold yellow" {
        Some(Color::Indexed(11))
    } else if s == "bold blue" {
        Some(Color::Indexed(12))
    } else if s == "bold magenta" {
        Some(Color::Indexed(13))
    } else if s == "bold cyan" {
        Some(Color::Indexed(14))
    } else if s == "bold white" {
        Some(Color::Indexed(15))
    } else if s == "black" {
        Some(Color::Indexed(0))
    } else if s == "red" {
        Some(Color::Indexed(1))
    } else if s == "green" {
        Some(Color::Indexed(2))
    } else if s == "yellow" {
        Some(Color::Indexed(3))
    } else if s == "blue" {
        Some(Color::Indexed(4))
    } else if s == "magenta" {
        Some(Color::Indexed(5))
    } else if s == "cyan" {
        Some(Color::Indexed(6))
    } else if s == "white" {
        Some(Color::Indexed(7))
    } else {
        None
    }
}

#[cfg(test)]
pub static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire the env-test lock, recovering gracefully from poison.
#[cfg(test)]
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_style_default() {
        let style = parse_style("");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn test_parse_style_foreground() {
        let style = parse_style("red");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
    }

    #[test]
    fn test_parse_style_background() {
        let style = parse_style("on blue");
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_parse_style_modifiers() {
        let style = parse_style("underline red on blue");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_extract_color_and_modifiers() {
        let (color, modifiers) = extract_color_and_modifiers("underline bold inverse gray");
        assert_eq!(color, "gray");
        assert!(modifiers.contains(Modifier::UNDERLINED));
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_parse_color_rgb() {
        let color = parse_color("rgb123");
        let expected = 16 + 36 + 2 * 6 + 3;
        assert_eq!(color, Some(Color::Indexed(expected)));
    }

    #[test]
    fn test_parse_color_unknown() {
        let color = parse_color("unknown");
        assert_eq!(color, None);
    }

    #[test]
    fn test_config() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "LAZYXRP_DATA"]);
        _env.remove("LAZYXRP_CONFIG");
        _env.remove("LAZYXRP_DATA");
        let c = Config::new()?;
        assert_eq!(
            c.keybindings
                .0
                .get(&Mode::Splash)
                .unwrap()
                .get(&parse_key_sequence("<q>").unwrap_or_default())
                .unwrap(),
            &Action::Quit
        );
        Ok(())
    }

    #[test]
    fn test_simple_keys() {
        assert_eq!(
            parse_key_event("a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
        );
    }

    #[test]
    fn test_with_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("alt-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );

        assert_eq!(
            parse_key_event("shift-esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_multiple_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-alt-a").unwrap(),
            KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )
        );

        assert_eq!(
            parse_key_event("ctrl-shift-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_reverse_multiple_modifiers() {
        assert_eq!(
            key_event_to_string(&KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            "ctrl-alt-a".to_string()
        );
    }

    #[test]
    fn test_invalid_keys() {
        assert!(parse_key_event("invalid-key").is_err());
        assert!(parse_key_event("ctrl-invalid-key").is_err());
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(
            parse_key_event("CTRL-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("AlT-eNtEr").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );
    }

    fn xrpl_config_toml(poll_interval_ms: u64) -> String {
        format!(
            r#"[xrpl]
account = "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = {poll_interval_ms}
network = "mainnet"
"#
        )
    }

    /// TC-033
    #[test]
    fn config_merge_user_poll_interval_overrides_default() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME"]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc033-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("config.toml"), xrpl_config_toml(88_888))?;
        _env.remove("XDG_CONFIG_HOME");
        _env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 88_888);
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }

    /// TC-092: Config merge — XRPL_RPC_SERVER overrides rpc_server from file
    #[test]
    fn config_merge_rpc_server_env_overrides_file() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME", XRPL_RPC_SERVER_ENV]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc092-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        let toml = r#"[xrpl]
account = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "mainnet"
rpc_server = "https://from-file.example"
"#;
        std::fs::write(root.join("config.toml"), toml)?;
        _env.remove("XDG_CONFIG_HOME");
        _env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        _env.set(XRPL_RPC_SERVER_ENV, "https://from-env.example");
        let c = Config::new()?;
        assert_eq!(
            c.xrpl.rpc_server.as_deref(),
            Some("https://from-env.example")
        );
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }

    /// TC-034
    #[test]
    fn config_loads_from_xdg_config_home() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME"]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc034-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let xdg = root.join("xdg");
        let lazy = xdg.join("lazyxrp");
        std::fs::create_dir_all(&lazy)?;
        std::fs::write(lazy.join("config.toml"), xrpl_config_toml(77_777))?;
        _env.remove("LAZYXRP_CONFIG");
        _env.set("XDG_CONFIG_HOME", xdg.to_str().unwrap());
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 77_777);
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }

    /// TC-035
    #[test]
    fn config_loads_when_only_home_is_set() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME", "HOME"]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc035-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(&home)?;
        _env.remove("LAZYXRP_CONFIG");
        _env.remove("XDG_CONFIG_HOME");
        _env.set("HOME", home.to_str().unwrap());
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("config.toml"), xrpl_config_toml(66_666))?;
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 66_666);
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }

    /// TC-036
    #[test]
    fn parse_key_sequence_unbalanced_brackets_errors() {
        assert!(parse_key_sequence("<<q>").is_err());
    }
}
