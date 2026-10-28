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

mod defaults;
mod keybindings;
mod styles;

pub use defaults::{FALLBACK_CURRENCY_CODE, FALLBACK_ISSUER};
use defaults::{
    default_flare_evm_key_env, default_oracle_pairs, default_slippage_bps, fallback_issuer,
};
pub use keybindings::KeyBindings;
pub use styles::Styles;

const CONFIG: &str = include_str!("../../config.json5");
const CONFIG_DIR_BASENAME: &str = "lazyxrp";

#[derive(Clone, Debug, Deserialize, Default)]
pub struct PathConfig {
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub config_dir: PathBuf,
}

/// Flare chain selection. Default: Flare mainnet.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlareDisplay {
    #[default]
    Full,
    Compact,
    Off,
}

/// Remove `name` from the environment and return its trimmed value as a secret; the raw
/// `String` copy is zeroized. Empty / unset → `None`.
fn take_secret_env(name: &str, trim: fn(&str) -> &str) -> Option<secrecy::SecretString> {
    use secrecy::zeroize::Zeroize;
    let mut raw = env::var(name).ok()?;
    // SAFETY: called only from `Config::new()` before any task is spawned; the (idle) runtime
    // worker threads already exist, but nothing reads the env concurrently.
    unsafe { env::remove_var(name) };
    let t = trim(&raw);
    let secret = (!t.is_empty()).then(|| secrecy::SecretString::from(t.to_string()));
    raw.zeroize();
    secret
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
    /// Core Vault XRPL address the operator verified out of band. FXRP Direct Mint Payments are
    /// refused unless the Flare RPC answer equals it (the RPC alone is not trusted for funds).
    #[serde(default)]
    pub expected_core_vault: Option<String>,
}

/// `[flare.smart_accounts]` — Smart Account instruction Payments (XRPL → operator → Flare).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FlareSmartAccountsConfig {
    /// Operator XRPL address verified out of band. Smart Account instruction Payments are
    /// refused unless `getXrplProviderWallets()[0]` from the Flare RPC equals it (the RPC alone
    /// is not trusted for funds).
    #[serde(default)]
    pub expected_operator: Option<String>,
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
    #[serde(default)]
    pub smart_accounts: FlareSmartAccountsConfig,
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
    /// Flare executor key taken from `[flare.fassets] evm_key_env` by `Config::new()`; the env
    /// var is removed at startup so no later `remove_var` runs while async tasks read the env.
    /// `run()` moves it out with [`Config::take_secrets`] before the config is shared.
    #[serde(skip)]
    pub flare_evm_key: Option<secrecy::SecretString>,
}

/// Every secret `Config::new()` collects, moved out of [`Config`] by [`Config::take_secrets`] so
/// the shared `Arc<Config>` (panels, status bar, splash) never holds one. Only the code paths that
/// sign (poll / submit / `flare_cmd`) receive it.
#[derive(Default)]
pub struct Secrets {
    pub seed: Option<secrecy::SecretString>,
    pub mnemonic: Option<secrecy::SecretString>,
    pub flare_evm_key: Option<secrecy::SecretString>,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = |s: &Option<secrecy::SecretString>| s.as_ref().map(|_| "[REDACTED]");
        f.debug_struct("Secrets")
            .field("seed", &set(&self.seed))
            .field("mnemonic", &set(&self.mnemonic))
            .field("flare_evm_key", &set(&self.flare_evm_key))
            .finish()
    }
}

impl Secrets {
    /// The XRPL signing credential (family seed **or** mnemonic); `Err` when both or invalid.
    pub fn credential(&self) -> color_eyre::Result<Option<crate::signing::SigningCredential>> {
        crate::signing::credential_from_secrets(self.seed.as_ref(), self.mnemonic.as_ref())
    }
}

/// Raw signing config as read from `[xrpl.signing]` in config.toml.
/// After `Config::new()`, plaintext `seed`/`mnemonic` are cleared into `secret_seed` /
/// `secret_mnemonic`; `run()` then moves those out with [`Config::take_secrets`].
#[derive(Clone, Default, Deserialize)]
pub struct RawSigningConfig {
    /// Signing seed (family seed format, e.g. `sXXX...`).
    /// ⚠️  Plain text on disk — prefer the `XRPL_SEED` env var instead.
    /// After `Config::new()` this is cleared to `None`; the value lives in [`Self::secret_seed`].
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

#[derive(Clone, Debug, Deserialize)]
pub struct LedgerConfig {
    pub account: String,
    #[serde(default = "fallback_issuer")]
    pub issuer: String,
    pub currency: String,
    /// 160-bit hex (or 3-letter ISO) for `book_offers`. Only needed when it cannot be derived
    /// from `currency` (e.g. an AMM LP token); unset means derive.
    #[serde(default)]
    pub currency_code: Option<String>,
    pub offer_limit: u16,
    pub poll_interval_ms: u64,
    /// Network preset (mainnet / testnet / devnet / xahau / xahau-test). Determines default RPC/WS endpoints.
    #[serde(default)]
    pub network: Network,
    /// Custom RPC endpoint. Overrides the network preset when set.
    #[serde(default)]
    pub rpc_server: Option<String>,
    /// Custom WebSocket endpoint. Overrides the network preset when set.
    #[serde(default)]
    pub ws_server: Option<String>,
    /// Raw signing config. After `Config::new()` the credentials sit in `secret_seed` /
    /// `secret_mnemonic` until [`Config::take_secrets`] moves them out.
    #[serde(default)]
    pub signing: RawSigningConfig,
    /// Oracle identifiers for `get_aggregate_price`.
    #[serde(default)]
    pub oracles: Vec<crate::xrpl::OracleId>,
    /// Price pairs to query via `get_aggregate_price`.
    #[serde(default = "default_oracle_pairs")]
    pub oracle_pairs: Vec<crate::xrpl::OraclePricePair>,
    /// Payment composer quote (`p`) slippage in basis points added to the path cost (100 = 1%).
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u32,
}

impl LedgerConfig {
    /// Quote currency code for the order book: the explicit `currency_code` when set,
    /// otherwise [`currency_code_for`](crate::xrpl::currency_code_for) of `currency`.
    pub fn quote_code(&self) -> String {
        match self.currency_code.as_deref().map(str::trim) {
            Some(code) if !code.is_empty() => code.to_string(),
            _ => crate::xrpl::currency_code_for(&self.currency),
        }
    }
}

/// Security (S-004): reject env-var-supplied paths that contain `..` traversal sequences.
fn validated_path(raw: PathBuf) -> Option<PathBuf> {
    if raw
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        eprintln!(
            "lazyxrp: ignoring env-var path '{}' — contains '..' traversal",
            raw.display()
        );
        return None;
    }
    Some(raw)
}

pub static PROJECT_NAME: &str = env!("CARGO_CRATE_NAME");

/// Network preset override (any `--network` value, case-insensitive). Merged in `Config::new()` so the UI matches the resolved network.
pub const XRPL_NETWORK_ENV: &str = "XRPL_NETWORK";
/// HTTP JSON-RPC endpoint override. Merged in `Config::new()` (env wins over config file) so splash and `[xrpl]` stay aligned with `resolve_rpc_url`.
pub const XRPL_RPC_SERVER_ENV: &str = "XRPL_RPC_SERVER";
/// WebSocket JSON-RPC endpoint override. Same merge rules as [`XRPL_RPC_SERVER_ENV`].
pub const XRPL_WS_SERVER_ENV: &str = "XRPL_WS_SERVER";

/// Expand a leading `~` / `~/` to the home directory (TOML/env values never pass through a shell).
fn expand_tilde(path: PathBuf) -> PathBuf {
    match (path.strip_prefix("~"), directories::BaseDirs::new()) {
        (Ok(rest), Some(base)) => base.home_dir().join(rest),
        _ => path,
    }
}

fn env_data_folder() -> Option<PathBuf> {
    env::var(format!("{}_DATA", PROJECT_NAME.to_uppercase()))
        .ok()
        .map(PathBuf::from)
        .map(expand_tilde)
        .and_then(validated_path)
}

fn env_config_folder() -> Option<PathBuf> {
    env::var(format!("{}_CONFIG", PROJECT_NAME.to_uppercase()))
        .ok()
        .map(PathBuf::from)
        .map(expand_tilde)
        .and_then(validated_path)
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
}

fn non_empty_path_or(path: &Path, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    if path.as_os_str().is_empty() {
        fallback()
    } else {
        expand_tilde(path.to_path_buf())
    }
}

type Credentials = (Option<secrecy::SecretString>, Option<secrecy::SecretString>);

/// Priority layering: if `high` sets either credential kind it replaces `low` entirely (so a
/// lower-priority source can never add the other kind). Both kinds inside one layer are kept
/// and rejected later by `credential_from_secrets`.
fn layer_credentials(low: Credentials, high: Credentials) -> Credentials {
    if high.0.is_some() || high.1.is_some() {
        high
    } else {
        low
    }
}

impl Config {
    /// Effective data directory: the config file's `data_dir`, else [`data_dir`] (env / platform default).
    pub fn resolved_data_dir(&self) -> PathBuf {
        non_empty_path_or(&self.paths.data_dir, data_dir)
    }

    /// Effective config directory: the config file's `config_dir`, else [`config_dir`] (env / XDG / platform default).
    pub fn resolved_config_dir(&self) -> PathBuf {
        non_empty_path_or(&self.paths.config_dir, config_dir)
    }

    /// Apply `--seed` / `--mnemonic` (highest priority: CLI > env > file). A non-empty value
    /// replaces every lower-priority credential of both kinds; an empty value just clears that
    /// kind. Both kinds on the CLI together stay set so `credential_from_secrets` rejects them.
    pub fn apply_cli_credentials(&mut self, seed: Option<&str>, mnemonic: Option<&str>) {
        let secret = |v: Option<&str>| {
            v.map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| secrecy::SecretString::from(t.to_string()))
        };
        let signing = &mut self.xrpl.signing;
        (signing.secret_seed, signing.secret_mnemonic) = layer_credentials(
            (signing.secret_seed.take(), signing.secret_mnemonic.take()),
            (secret(seed), secret(mnemonic)),
        );
        if seed.is_some_and(|s| s.trim().is_empty()) {
            signing.secret_seed = None;
        }
        if mnemonic.is_some_and(|s| s.trim().is_empty()) {
            signing.secret_mnemonic = None;
        }
    }

    /// Move every secret out of the config (leaving `None`) so what gets shared as `Arc<Config>`
    /// carries none. Call after `apply_cli_credentials`, before the config is shared.
    pub fn take_secrets(&mut self) -> Secrets {
        let signing = &mut self.xrpl.signing;
        Secrets {
            seed: signing.secret_seed.take(),
            mnemonic: signing.secret_mnemonic.take(),
            flare_evm_key: self.flare_evm_key.take(),
        }
    }

    pub fn new() -> color_eyre::Result<Self> {
        use color_eyre::eyre::WrapErr as _;
        // Security (S-007): embedded config is a compile-time constant — parse failure
        // indicates a build-time bug, not a runtime condition.
        let default_config: Config = json5::from_str(CONFIG)
            .expect("embedded config.json5 is malformed — this is a build-time bug");
        let config_dir = config_dir();
        let config_file = config_dir.join("config.toml");
        // Empty / missing `paths` fall back to `data_dir()` / `config_dir()` in `resolved_*_dir`.
        let mut config: Self = match std::fs::read_to_string(&config_file) {
            Ok(text) => toml::from_str(&text)
                .wrap_err_with(|| format!("parsing {}", config_file.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Before `logging::init`, tracing may have no subscriber — surface this on stderr.
                eprintln!(
                    "lazyxrp: warning: no configuration file found under {}; defaults apply",
                    config_dir.display()
                );
                Self::default()
            }
            Err(e) => {
                return Err(e).wrap_err_with(|| format!("reading {}", config_file.display()));
            }
        };

        // Security (S-003): warn about permissions of the file actually read, BEFORE the
        // credential leaves the plain-text fields.
        if config.xrpl.signing.seed.is_some() || config.xrpl.signing.mnemonic.is_some() {
            warn_if_config_world_readable(&config_file);
        }

        // Security: promote any plain-text credential (from config file) into the masked
        // fields and wipe the plain ones so Arc<Config> never carries an unmasked credential.
        let file_creds = (
            config
                .xrpl
                .signing
                .seed
                .take()
                .map(secrecy::SecretString::from),
            config
                .xrpl
                .signing
                .mnemonic
                .take()
                .map(secrecy::SecretString::from),
        );

        // Env secrets win over the file. Taken once here, before the poll/UI tasks start; no
        // later `remove_var` runs while tasks may read the env. On Linux `remove_var` does not
        // rewrite the initial `/proc/self/environ` block (same-uid readers still see it).
        let env_creds = (
            take_secret_env(crate::signing::SEED_ENV, str::trim),
            take_secret_env(crate::signing::MNEMONIC_ENV, str::trim),
        );
        (
            config.xrpl.signing.secret_seed,
            config.xrpl.signing.secret_mnemonic,
        ) = layer_credentials(file_creds, env_creds);
        config.flare_evm_key = take_secret_env(&config.flare.fassets.evm_key_env, str::trim);

        if let Ok(v) = env::var(XRPL_NETWORK_ENV)
            && let Ok(n) = <Network as clap::ValueEnum>::from_str(&v, true)
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

/// Security (S-003): message to show when a config file containing a signing credential is
/// group- or world-readable. `None` on non-Unix platforms or when the mode is private.
#[cfg(unix)]
fn config_permission_warning(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = path.metadata().ok()?.permissions().mode();
    // 0o044 = group-read (0o040) | world-read (0o004)
    (mode & 0o044 != 0).then(|| {
        format!(
            "lazyxrp: warning: config file '{}' has mode {:04o} and contains a signing credential. \
             Run: chmod 600 {}",
            path.display(),
            mode & 0o777,
            path.display()
        )
    })
}

#[cfg(not(unix))]
fn config_permission_warning(_path: &std::path::Path) -> Option<String> {
    None
}

/// Printed on stderr: `Config::new` runs before `logging::init`, so a `tracing` event
/// would have no subscriber and the warning would never be shown.
fn warn_if_config_world_readable(path: &std::path::Path) {
    if let Some(msg) = config_permission_warning(path) {
        eprintln!("{msg}");
    }
}

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

#[cfg(test)]
mod tests {
    use super::{keybindings::parse_key_sequence, *};
    use crate::test_support::{TempRootGuard, TestEnvGuard, env_lock};

    /// The quote code follows `currency` unless `currency_code` overrides it: a 3-letter code
    /// stays as is, a longer symbol becomes zero-padded hex, and an explicit LP-token code wins.
    #[test]
    fn quote_code_derives_from_currency_unless_overridden() {
        let mut cfg = LedgerConfig::default();
        assert_eq!(cfg.quote_code(), "524C555344000000000000000000000000000000");
        cfg.currency = "USD".into();
        assert_eq!(cfg.quote_code(), "USD");
        let hex40 = "524C555344000000000000000000000000000000";
        cfg.currency = hex40.into();
        assert_eq!(
            cfg.quote_code(),
            hex40,
            "an existing 160-bit code is not encoded twice"
        );
        cfg.currency = "USD".into();
        cfg.currency_code = Some("03AABB".into());
        assert_eq!(cfg.quote_code(), "03AABB");
        cfg.currency_code = Some("  ".into());
        assert_eq!(
            cfg.quote_code(),
            "USD",
            "blank override falls back to derivation"
        );
    }

    /// TC-181: group/world-readable credential config warns
    /// S-003: a group/world-readable credential-bearing config yields a warning (shown on
    /// stderr, since it runs before the tracing subscriber exists); a 0600 file does not.
    #[cfg(unix)]
    #[test]
    fn config_permission_warning_flags_group_or_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("lazyxrp-perm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "x = 1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let msg = config_permission_warning(&path).expect("0644 must warn");
        assert!(msg.contains("chmod 600"), "{msg}");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(config_permission_warning(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// TC-113/TC-114: Flare config presets load from a config file and default sensibly
    #[test]
    fn flare_config_file_presets() {
        let cfg: FlareConfig =
            toml::from_str("network = \"coston2\"\ndisplay = \"compact\"").unwrap();
        assert_eq!(
            cfg.network.rpc_url(),
            "https://coston2-api.flare.network/ext/C/rpc"
        );
        assert_eq!(cfg.display, FlareDisplay::Compact);
        let cfg: FlareConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.network, FlareNetwork::Flare);
        assert_eq!(cfg.display, FlareDisplay::Full);
    }

    /// TC-246: `[flare.smart_accounts] expected_operator` parses; unset by default (writes refused).
    #[test]
    fn flare_smart_accounts_expected_operator_parses() {
        let cfg: FlareConfig =
            toml::from_str("[smart_accounts]\nexpected_operator = \"rOperator\"").unwrap();
        assert_eq!(
            cfg.smart_accounts.expected_operator.as_deref(),
            Some("rOperator")
        );
        let cfg: FlareConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.smart_accounts.expected_operator, None);
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
        assert!(
            dir.starts_with(&home),
            "{dir:?} must live under HOME {home:?}"
        );
        assert!(dir.ends_with("lazyxrp"), "{dir:?}");
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
            "FLARE_EVM_KEY",
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
        _test_env.set("FLARE_EVM_KEY", " 0xabc ");
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
        // Flare executor key is taken at startup (not lazily under the async runtime) and scrubbed.
        assert_eq!(
            c.flare_evm_key.as_ref().map(|s| s.expose_secret()),
            Some("0xabc")
        );
        assert!(std::env::var("FLARE_EVM_KEY").is_err());
        Ok(())
    }

    /// TC-026: Config::new keeps default keybindings (untouched `<q>` → Quit)
    /// TC-195: user `[keybindings.Splash]` in config.toml adds keys and overrides a default
    /// binding; untouched defaults survive the merge.
    #[test]
    fn user_keybindings_override_and_extend_defaults() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME"]);
        let toml = r#"[xrpl]
account = "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "mainnet"

[keybindings.Splash]
"<x>" = "Help"
"<r>" = "RefreshBook"
"#;
        let (root, _guard) = write_config_dir("tc195", toml)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let c = Config::new()?;
        let get = |k: &str| {
            c.keybindings
                .0
                .get(&parse_key_sequence(k).unwrap())
                .cloned()
        };
        assert_eq!(get("<x>"), Some(Action::Help));
        assert_eq!(get("<r>"), Some(Action::RefreshBook));
        assert_eq!(get("<q>"), Some(Action::Quit));
        Ok(())
    }

    /// TC-127/TC-042: file `network = "xahau-test"` loads; `XRPL_NETWORK` overrides it; an
    /// unparsable value (garbage or a leading-space typo) is ignored, keeping the file value.
    #[test]
    fn xrpl_network_file_and_env_merge() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME", XRPL_NETWORK_ENV]);
        let toml = sample_xrpl_config_toml(5_000).replace("mainnet", "xahau-test");
        let (root, _root_guard) = write_config_dir("tc042", &toml)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        _test_env.remove(XRPL_NETWORK_ENV);
        assert_eq!(Config::new()?.xrpl.network, Network::XahauTest);
        _test_env.set(XRPL_NETWORK_ENV, "testnet");
        assert_eq!(Config::new()?.xrpl.network, Network::Testnet);
        for bad in ["not-a-network", " testnet"] {
            _test_env.set(XRPL_NETWORK_ENV, bad);
            assert_eq!(Config::new()?.xrpl.network, Network::XahauTest, "{bad:?}");
        }
        Ok(())
    }

    /// `XRPL_WS_SERVER` / `XRPL_RPC_SERVER` override the file (trimmed); blank values do not.
    #[test]
    fn xrpl_server_env_merge() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            XRPL_RPC_SERVER_ENV,
            XRPL_WS_SERVER_ENV,
        ]);
        let (root, _guard) = write_config_dir("xrpl-servers", TOML_WITH_RPC_SERVER)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        _test_env.set(XRPL_RPC_SERVER_ENV, "   ");
        _test_env.set(XRPL_WS_SERVER_ENV, " wss://ws-env.example ");
        let c = Config::new()?;
        assert_eq!(
            c.xrpl.rpc_server.as_deref(),
            Some("https://from-file.example")
        );
        assert_eq!(c.xrpl.ws_server.as_deref(), Some("wss://ws-env.example"));
        Ok(())
    }

    /// A malformed config.toml is a hard error, not a silent fallback to defaults.
    #[test]
    fn malformed_config_toml_is_error() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "XDG_CONFIG_HOME"]);
        let (root, _guard) = write_config_dir("malformed", "[xrpl\nnetwork = ")?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let err = Config::new().expect_err("malformed toml must fail");
        assert!(format!("{err:#}").contains("config.toml"), "{err:#}");
        Ok(())
    }

    /// S-004: `..` in `LAZYXRP_CONFIG` / `LAZYXRP_DATA` is rejected (falls through to defaults).
    #[test]
    fn env_paths_with_parent_dir_are_rejected() {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "LAZYXRP_DATA", "XDG_CONFIG_HOME"]);
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", "/tmp/lazyxrp-evil/../etc");
        _test_env.set("LAZYXRP_DATA", "../../etc");
        assert_ne!(config_dir(), PathBuf::from("/tmp/lazyxrp-evil/../etc"));
        assert_ne!(data_dir(), PathBuf::from("../../etc"));
        assert!(
            !config_dir()
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        );
        assert!(
            !data_dir()
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        );
        _test_env.set("LAZYXRP_CONFIG", "/tmp/lazyxrp-ok/cfg");
        assert_eq!(config_dir(), PathBuf::from("/tmp/lazyxrp-ok/cfg"));
    }

    /// TC-359: only bindable actions allowed
    /// Keybindings may only bind payload-free safe actions; write/payload variants are
    /// rejected at load.
    #[test]
    fn keybinding_rejects_payload_and_submit_actions() {
        let bad = r#"[keybindings.Splash]
"<x>" = { AccountDeleteSubmit = { destination = "rEvil", skip_mainnet_prompt = true, network = "mainnet" } }
"#;
        let err = toml::from_str::<Config>(bad).expect_err("submit action must be rejected");
        assert!(err.to_string().contains("cannot be bound"), "{err}");
        let bad_unit = "[keybindings.Splash]\n\"<x>\" = \"NftBurnStart\"\n";
        assert!(toml::from_str::<Config>(bad_unit).is_err());
        let ok = "[keybindings.Splash]\n\"<x>\" = \"Quit\"\n";
        let c: Config = toml::from_str(ok).unwrap();
        assert_eq!(
            c.keybindings.0.get(&parse_key_sequence("<x>").unwrap()),
            Some(&Action::Quit)
        );
    }

    /// TC-359: `~` paths expand
    /// A leading `~` in env / file directory settings resolves to HOME.
    #[test]
    fn tilde_paths_expand_to_home() {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["HOME", "LAZYXRP_CONFIG", "LAZYXRP_DATA"]);
        let home = std::env::temp_dir().join(format!("lazyxrp-tilde-{}", std::process::id()));
        _test_env.set("HOME", home.to_str().unwrap());
        _test_env.set("LAZYXRP_CONFIG", "~/cfg/lazyxrp");
        assert_eq!(config_dir(), home.join("cfg/lazyxrp"));
        let mut c = Config::default();
        c.paths.data_dir = PathBuf::from("~/data/lazyxrp");
        assert_eq!(c.resolved_data_dir(), home.join("data/lazyxrp"));
        c.paths.config_dir = PathBuf::from("~");
        assert_eq!(c.resolved_config_dir(), home);
        c.paths.config_dir = PathBuf::from("~other/x");
        assert_eq!(c.resolved_config_dir(), PathBuf::from("~other/x"));
    }

    /// TC-359: credential precedence matrix
    /// Credential precedence: the highest-priority source wins; the other kind from lower
    /// sources is dropped. Same-level seed+mnemonic stays set (rejected by the caller).
    #[test]
    fn credential_precedence_matrix() -> color_eyre::Result<()> {
        use secrecy::ExposeSecret;
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            crate::signing::SEED_ENV,
            crate::signing::MNEMONIC_ENV,
            "FLARE_EVM_KEY",
        ]);
        let toml = format!(
            "{}\n[xrpl.signing]\nmnemonic = \"file words\"\n",
            sample_xrpl_config_toml(5_000)
        );
        let (root, _guard) = write_config_dir("cred-prec", &toml)?;
        _test_env.remove("XDG_CONFIG_HOME");
        _test_env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let kinds = |c: &Config| {
            (
                c.xrpl
                    .signing
                    .secret_seed
                    .as_ref()
                    .map(|s| s.expose_secret().to_string()),
                c.xrpl
                    .signing
                    .secret_mnemonic
                    .as_ref()
                    .map(|s| s.expose_secret().to_string()),
            )
        };
        _test_env.remove(crate::signing::SEED_ENV);
        _test_env.remove(crate::signing::MNEMONIC_ENV);
        _test_env.remove("FLARE_EVM_KEY");
        // file only
        assert_eq!(kinds(&Config::new()?), (None, Some("file words".into())));
        // env seed beats file mnemonic (file mnemonic dropped)
        _test_env.set(crate::signing::SEED_ENV, "sEnv");
        assert_eq!(kinds(&Config::new()?), (Some("sEnv".into()), None));
        // env seed + env mnemonic: same level, both kept
        _test_env.set(crate::signing::SEED_ENV, "sEnv");
        _test_env.set(crate::signing::MNEMONIC_ENV, "env words");
        assert_eq!(
            kinds(&Config::new()?),
            (Some("sEnv".into()), Some("env words".into()))
        );
        // CLI mnemonic beats env seed
        _test_env.set(crate::signing::SEED_ENV, "sEnv");
        _test_env.remove(crate::signing::MNEMONIC_ENV);
        let mut c = Config::new()?;
        c.apply_cli_credentials(None, Some(" cli words "));
        assert_eq!(kinds(&c), (None, Some("cli words".into())));
        // CLI seed beats env mnemonic; empty CLI value only clears its own kind
        _test_env.set(crate::signing::MNEMONIC_ENV, "env words");
        _test_env.remove(crate::signing::SEED_ENV);
        let mut c = Config::new()?;
        c.apply_cli_credentials(Some("sCli"), None);
        assert_eq!(kinds(&c), (Some("sCli".into()), None));
        _test_env.set(crate::signing::MNEMONIC_ENV, "env words");
        let mut c = Config::new()?;
        c.apply_cli_credentials(Some(""), None);
        assert_eq!(kinds(&c), (None, Some("env words".into())));
        Ok(())
    }

    /// `take_secrets` moves seed, mnemonic and Flare EVM key out of the config; neither the
    /// emptied `Config` nor `Secrets` leaks a value through `Debug`.
    #[test]
    fn take_secrets_empties_config_and_debug_does_not_leak() {
        use secrecy::ExposeSecret;
        let secret = |v: &str| Some(secrecy::SecretString::from(v.to_string()));
        let mut c = Config::default();
        c.xrpl.signing.secret_seed = secret("sSeedLeakMarker");
        c.xrpl.signing.secret_mnemonic = secret("mnemonic leak marker");
        c.flare_evm_key = secret("0xEvmLeakMarker");

        let secrets = c.take_secrets();
        assert!(c.xrpl.signing.secret_seed.is_none());
        assert!(c.xrpl.signing.secret_mnemonic.is_none());
        assert!(c.flare_evm_key.is_none());
        assert_eq!(
            secrets.seed.as_ref().map(|s| s.expose_secret()),
            Some("sSeedLeakMarker")
        );
        assert!(secrets.mnemonic.is_some() && secrets.flare_evm_key.is_some());

        let dump = format!("{c:?}{secrets:?}");
        for marker in ["LeakMarker", "leak marker"] {
            assert!(!dump.contains(marker), "{dump}");
        }
        assert!(dump.contains("[REDACTED]"));
    }
}
