use std::{
    collections::HashMap,
    env, fmt,
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::ProjectDirs;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, de::Deserializer, de::Error as _};

use crate::{action::Action, network::Network};

const CONFIG: &str = include_str!("../config.json5");
const CONFIG_DIR_BASENAME: &str = "lazyxrp";

#[derive(Clone, Debug, Deserialize, Default)]
pub struct PathConfig {
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub config_dir: PathBuf,
}

/// Flare chain selection. Default: Flare mainnet.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
pub enum FlareNetwork {
    #[default]
    Flare,
    Songbird,
    Coston2,
}

impl FlareNetwork {
    #[must_use]
    pub fn rpc_url(&self) -> &'static str {
        match self {
            Self::Flare => crate::flare::DEFAULT_FLARE_RPC,
            Self::Songbird => "https://songbird-api.flare.network/ext/C/rpc",
            Self::Coston2 => "https://coston2-api.flare.network/ext/C/rpc",
        }
    }
}

/// `[flare] display` — panel density on Overview / Market tabs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
pub enum FlareDisplay {
    #[default]
    Full,
    Compact,
    Off,
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

/// `[flare.wallet]` — read-only Flare EVM wallet panel on Overview.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FlareWalletConfig {
    #[serde(default, deserialize_with = "deserialize_optional_evm_address")]
    pub address: Option<String>,
}

#[must_use]
pub fn normalize_evm_address(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: alloy::primitives::Address = trimmed.parse().ok()?;
    Some(format!("{parsed:#x}"))
}

fn deserialize_optional_evm_address<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(raw) if raw.trim().is_empty() => Ok(None),
        Some(raw) => normalize_evm_address(&raw)
            .ok_or_else(|| D::Error::custom(format!("invalid EVM address: {raw}")))
            .map(Some),
    }
}

/// Top-level `[flare]` config.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FlareConfig {
    #[serde(default)]
    pub network: FlareNetwork,
    #[serde(default)]
    pub display: FlareDisplay,
    #[serde(default)]
    pub fassets: FlareFassetsConfig,
    #[serde(default)]
    pub wallet: FlareWalletConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default, flatten)]
    pub paths: PathConfig,
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
/// After `Config::new()`, plaintext `seed`/`mnemonic` are cleared; use `secret_seed` / `secret_mnemonic`.
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
    #[serde(default)]
    pub mnemonic: Option<String>,
    #[serde(skip)]
    pub secret_mnemonic: Option<secrecy::SecretString>,
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
            .field("mnemonic", &self.mnemonic.as_ref().map(|_| "[REDACTED]"))
            .field(
                "secret_mnemonic",
                &self.secret_mnemonic.as_ref().map(|_| "[REDACTED]"),
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
    /// Raw signing config. After `Config::new()`, read `secret_seed` / `secret_mnemonic`.
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
        non_empty_path_or(&self.paths.data_dir, data_dir)
    }

    /// Effective config directory after merging embedded defaults, env keys, and config file.
    pub fn resolved_config_dir(&self) -> PathBuf {
        non_empty_path_or(&self.paths.config_dir, config_dir)
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
        let config_file = config_dir.join("config.toml");
        let found_config = config_file.exists();
        builder = builder.add_source(
            config::File::from(config_file)
                .format(config::FileFormat::Toml)
                .required(false),
        );
        if !found_config {
            // Before `logging::init`, tracing may have no subscriber — surface this on stderr.
            eprintln!(
                "lazyxrp: warning: no configuration file found under {}; defaults apply",
                config_dir.display()
            );
        }

        let mut config: Self = builder.build()?.try_deserialize()?;

        // Security (S-003): if a signing credential is present in config, warn about file permissions
        // BEFORE we move it out of the plain-text fields.
        let resolved_cfg_dir = config.resolved_config_dir();
        if config.xrpl.signing.seed.is_some() || config.xrpl.signing.mnemonic.is_some() {
            let path = resolved_cfg_dir.join("config.toml");
            if path.exists() {
                warn_if_config_world_readable(&path);
            }
        }

        // Security: promote any plain-text seed (from config file) into secret_seed
        // and wipe the field so Arc<Config> never carries an unmasked credential.
        // Must happen BEFORE the env-var merge so env (XRPL_SEED) can override file.
        if let Some(plain) = config.xrpl.signing.seed.take() {
            config.xrpl.signing.secret_seed = Some(secrecy::SecretString::from(plain));
        }
        if let Some(plain) = config.xrpl.signing.mnemonic.take() {
            config.xrpl.signing.secret_mnemonic = Some(secrecy::SecretString::from(plain));
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
        if let Ok(env_mnemonic) = env::var(crate::signing::MNEMONIC_ENV) {
            unsafe { env::remove_var(crate::signing::MNEMONIC_ENV) };
            let t = env_mnemonic.trim();
            if !t.is_empty() {
                config.xrpl.signing.secret_mnemonic =
                    Some(secrecy::SecretString::from(t.to_string()));
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

        for (key, cmd) in default_config.keybindings.0.iter() {
            config
                .keybindings
                .0
                .entry(key.clone())
                .or_insert_with(|| cmd.clone());
        }
        for (style_key, style) in default_config.styles.0.iter() {
            config.styles.0.entry(style_key.clone()).or_insert(*style);
        }

        Ok(config)
    }
}

/// Security (S-003): warn when a config file containing a signing credential is group- or world-readable.
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
fn warn_if_config_world_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = path.metadata() {
        let mode = meta.permissions().mode();
        // 0o044 = group-read (0o040) | world-read (0o004)
        if mode & 0o044 != 0 {
            tracing::warn!(
                // Security: config file with signing credential should be readable only by owner (0600)
                "Config file '{}' has mode {:04o} and contains a signing credential. \
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
pub struct KeyBindings(pub HashMap<Vec<KeyEvent>, Action>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> color_eyre::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Keep the existing on-disk `Splash` section for compatibility while
        // storing one flat map internally until another mode is implemented.
        let parsed_map = HashMap::<String, HashMap<String, Action>>::deserialize(deserializer)?;
        let inner_map = parsed_map
            .into_iter()
            .find(|(mode, _)| mode.eq_ignore_ascii_case("splash"))
            .map(|(_, bindings)| bindings)
            .unwrap_or_default();
        let mut keybindings = HashMap::new();
        for (key_str, cmd) in inner_map {
            let seq = parse_key_sequence(&key_str).map_err(D::Error::custom)?;
            keybindings.insert(seq, cmd);
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
pub struct Styles(pub HashMap<String, Style>);

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Keep the existing on-disk `Splash` section for compatibility; styles
        // are flat internally because no second mode currently exists.
        let parsed_map = HashMap::<String, HashMap<String, String>>::deserialize(deserializer)?;
        let inner_map = parsed_map
            .into_iter()
            .find(|(mode, _)| mode.eq_ignore_ascii_case("splash"))
            .map(|(_, styles)| styles)
            .unwrap_or_default();
        let styles = inner_map
            .into_iter()
            .map(|(name, style)| (name, parse_style(&style)))
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
mod tests {
    use super::*;
    use crate::test_support::{TempRootGuard, TestEnvGuard, env_lock};

    /// TC-113: FlareNetwork parses case-insensitively, defaults to Flare
    #[test]
    fn flare_network_parse() {
        assert_eq!(
            "flare".parse::<FlareNetwork>().unwrap(),
            FlareNetwork::Flare
        );
        assert_eq!(
            "COSTON2".parse::<FlareNetwork>().unwrap(),
            FlareNetwork::Coston2
        );
        assert_eq!(
            "Songbird".parse::<FlareNetwork>().unwrap(),
            FlareNetwork::Songbird
        );
        assert_eq!(FlareNetwork::default(), FlareNetwork::Flare);
        assert_eq!(
            FlareNetwork::Coston2.rpc_url(),
            "https://coston2-api.flare.network/ext/C/rpc"
        );
    }

    /// TC-114: FlareDisplay parses and defaults to full
    #[test]
    fn flare_display_parse() {
        assert_eq!("off".parse::<FlareDisplay>().unwrap(), FlareDisplay::Off);
        assert_eq!(
            "Compact".parse::<FlareDisplay>().unwrap(),
            FlareDisplay::Compact
        );
        assert_eq!(FlareDisplay::default(), FlareDisplay::Full);
    }

    /// TC-120: `[flare.wallet] address` validates EVM hex and normalizes to `0x…`
    #[test]
    fn flare_wallet_address_validation() {
        assert_eq!(
            normalize_evm_address("0xAbCdEf0123456789AbCdEf0123456789AbCdEf01"),
            Some("0xabcdef0123456789abcdef0123456789abcdef01".to_string())
        );
        assert!(normalize_evm_address("not-an-address").is_none());
        let toml = r#"
            [wallet]
            address = "0xabcdef0123456789abcdef0123456789abcdef01"
        "#;
        let cfg: FlareConfig = toml::from_str(toml).expect("valid wallet address");
        assert_eq!(
            cfg.wallet.address.as_deref(),
            Some("0xabcdef0123456789abcdef0123456789abcdef01")
        );
        let bad = r#"
            [wallet]
            address = "0xshort"
        "#;
        assert!(toml::from_str::<FlareConfig>(bad).is_err());
    }

    /// TC-019-022: parse_style — empty yields default, fg/bg mapping, modifiers combined, gray==grey
    #[test]
    fn parse_style_spec_mapping() {
        // Empty spec yields the default style.
        assert_eq!(parse_style(""), Style::default(), "empty style spec");

        let cases = [
            ("red", Some(Color::Indexed(1)), None),
            ("on blue", None, Some(Color::Indexed(4))),
        ];
        for (input, fg, bg) in cases {
            let style = parse_style(input);
            assert_eq!(style.fg, fg, "fg for {input:?}");
            assert_eq!(style.bg, bg, "bg for {input:?}");
        }

        let gray = parse_style("underline bold inverse gray");
        let grey = parse_style("underline bold inverse grey");
        assert_eq!(gray.fg, grey.fg, "gray and grey must map to the same color");
        for style in [gray, grey] {
            assert!(style.add_modifier.contains(Modifier::UNDERLINED));
            assert!(style.add_modifier.contains(Modifier::BOLD));
            assert!(style.add_modifier.contains(Modifier::REVERSED));
            assert!(style.fg.is_some());
        }
    }

    /// TC-024/TC-025: parse_color — RGB shorthand → indexed palette, unknown → None
    #[test]
    fn parse_color_contract() {
        let color = parse_color("rgb123");
        let expected = 16 + 36 + 2 * 6 + 3;
        assert_eq!(color, Some(Color::Indexed(expected)));
        assert_eq!(parse_color("unknown"), None);
    }

    #[test]
    fn splash_default_q_quits() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "LAZYXRP_DATA", "XDG_CONFIG_HOME"]);
        _test_env.remove("LAZYXRP_CONFIG");
        _test_env.remove("LAZYXRP_DATA");
        // Redirect XDG so a real user config.toml (e.g. rebound keys) can't leak in.
        _test_env.set(
            "XDG_CONFIG_HOME",
            std::env::temp_dir()
                .join(format!("lazyxrp-xdg-test-{}", std::process::id()))
                .to_str()
                .expect("temp xdg path"),
        );
        let c = Config::new()?;
        assert_eq!(
            c.keybindings
                .0
                .get(&parse_key_sequence("<q>").unwrap_or_default())
                .unwrap(),
            &Action::Quit
        );
        Ok(())
    }

    /// TC-027-029/TC-031/TC-032: parse_key_event — modifier composition, case-insensitivity, unknown keys error
    #[test]
    fn parse_key_event_contract() {
        let cases = [
            ("a", Some((KeyCode::Char('a'), KeyModifiers::empty()))),
            ("enter", Some((KeyCode::Enter, KeyModifiers::empty()))),
            ("esc", Some((KeyCode::Esc, KeyModifiers::empty()))),
            ("ctrl-a", Some((KeyCode::Char('a'), KeyModifiers::CONTROL))),
            ("alt-enter", Some((KeyCode::Enter, KeyModifiers::ALT))),
            ("shift-esc", Some((KeyCode::Esc, KeyModifiers::SHIFT))),
            ("CTRL-a", Some((KeyCode::Char('a'), KeyModifiers::CONTROL))),
            ("AlT-eNtEr", Some((KeyCode::Enter, KeyModifiers::ALT))),
            (
                "ctrl-alt-a",
                Some((
                    KeyCode::Char('a'),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                )),
            ),
            (
                "ctrl-shift-enter",
                Some((KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
            ),
            ("invalid-key", None),
            ("ctrl-invalid-key", None),
        ];
        for (input, expected) in cases {
            let parsed = parse_key_event(input)
                .ok()
                .map(|ev| (ev.code, ev.modifiers));
            assert_eq!(parsed, expected, "key event for {input:?}");
        }
    }

    /// TC-030: key_event_to_string — modifiers listed before key
    #[test]
    fn key_event_to_string_lists_modifiers_before_key() {
        assert_eq!(
            key_event_to_string(&KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            "ctrl-alt-a".to_string()
        );
    }

    fn sample_xrpl_config_toml(poll_interval_ms: u64) -> String {
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

    /// Creates a temp config dir containing `toml`, returning the root and its cleanup guard.
    fn write_config_dir(
        tag: &str,
        toml: &str,
    ) -> color_eyre::Result<(std::path::PathBuf, TempRootGuard)> {
        let root = std::env::temp_dir().join(format!("lazyxrp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let guard = TempRootGuard::new(root.clone());
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("config.toml"), toml)?;
        Ok((root, guard))
    }

    const TOML_WITH_RPC_SERVER: &str = r#"[xrpl]
account = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "mainnet"
rpc_server = "https://from-file.example"
"#;

    /// TC-033/TC-034/TC-035/TC-092: config source precedence —
    /// file overrides built-in defaults; env var overrides file value;
    /// XDG_CONFIG_HOME resolves the file location; bare HOME is the fallback.
    #[test]
    fn config_source_precedence() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            "HOME",
            XRPL_RPC_SERVER_ENV,
        ]);

        // File value overrides built-in default.
        let (root, _guard1) = write_config_dir("tc033", &sample_xrpl_config_toml(88_888))?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 88_888);

        // Env var overrides the file's value.
        let (root, _guard2) = write_config_dir("tc092", TOML_WITH_RPC_SERVER)?;
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        _test_env.set(XRPL_RPC_SERVER_ENV, "https://from-env.example");
        let c = Config::new()?;
        assert_eq!(
            c.xrpl.rpc_server.as_deref(),
            Some("https://from-env.example")
        );

        // XDG_CONFIG_HOME resolves the file location.
        let root = std::env::temp_dir().join(format!("lazyxrp-tc034-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _guard3 = TempRootGuard::new(root.clone());
        let xdg = root.join("xdg");
        let lazy = xdg.join("lazyxrp");
        std::fs::create_dir_all(&lazy)?;
        std::fs::write(lazy.join("config.toml"), sample_xrpl_config_toml(77_777))?;
        _test_env.remove("LAZYXRP_CONFIG");
        _test_env.remove(XRPL_RPC_SERVER_ENV);
        _test_env.set("XDG_CONFIG_HOME", xdg.to_str().unwrap());
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 77_777);

        // Bare HOME is the fallback.
        let root = std::env::temp_dir().join(format!("lazyxrp-tc035-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _guard4 = TempRootGuard::new(root.clone());
        let home = root.join("home");
        std::fs::create_dir_all(&home)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("HOME", home.to_str().unwrap());
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("config.toml"), sample_xrpl_config_toml(66_666))?;
        let c = Config::new()?;
        assert_eq!(c.xrpl.poll_interval_ms, 66_666);
        Ok(())
    }

    /// TC-049: env `XRPL_SEED` overrides the file's plain seed; the plain field is cleared.
    #[test]
    fn config_env_seed_overrides_file_seed() -> color_eyre::Result<()> {
        use secrecy::ExposeSecret;
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            crate::signing::SEED_ENV,
        ]);
        let toml = r#"[xrpl]
account = "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "mainnet"

[xrpl.signing]
seed = "sEdFileSeedValueNotReal1234567890"
"#;
        let (root, _guard) = write_config_dir("tc049", toml)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        _test_env.set(crate::signing::SEED_ENV, "sEdEnvSeedOverride9876543210");
        let c = Config::new()?;
        assert_eq!(
            c.xrpl
                .signing
                .secret_seed
                .as_ref()
                .map(|s| s.expose_secret()),
            Some("sEdEnvSeedOverride9876543210")
        );
        assert!(
            c.xrpl.signing.seed.is_none(),
            "plain file seed must be cleared"
        );
        Ok(())
    }

    /// TC-036
    #[test]
    fn parse_key_sequence_unbalanced_brackets_errors() {
        assert!(parse_key_sequence("<<q>").is_err());
    }

    /// TC-127: config.toml `network = "xahau-test"` loads as XahauTest
    #[test]
    fn config_file_network_xahau_test_loads() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME"]);
        let toml = r#"[xrpl]
account = "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "xahau-test"
"#;
        let (root, _root_guard) = write_config_dir("tc127", toml)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let c = Config::new()?;
        assert_eq!(c.xrpl.network, Network::XahauTest);
        Ok(())
    }
}
