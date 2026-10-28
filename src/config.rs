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

fn default_flare_evm_key_env() -> String {
    "FLARE_EVM_KEY".to_string()
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

impl Default for FlareFassetsConfig {
    fn default() -> Self {
        Self {
            execute: false,
            evm_key_env: default_flare_evm_key_env(),
            expected_core_vault: None,
        }
    }
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

pub const FALLBACK_CURRENCY_CODE: &str = "USD";
pub const FALLBACK_ISSUER: &str = "rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

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

fn default_slippage_bps() -> u32 {
    100
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
    /// otherwise `currency` as is when it is already a code (3 letters, or 40 hex), else a
    /// symbol of up to 20 bytes as zero-padded 160-bit hex. Anything longer is passed through
    /// unchanged so the node rejects it instead of the book silently following another token.
    pub fn quote_code(&self) -> String {
        match self.currency_code.as_deref().map(str::trim) {
            Some(code) if !code.is_empty() => code.to_string(),
            _ if self.currency.len() == 3 || self.currency.len() > 20 => self.currency.clone(),
            _ => format!("{:0<40}", hex::encode_upper(&self.currency)),
        }
    }
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            account: "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV".to_string(),
            issuer: "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De".to_string(),
            currency: "RLUSD".to_string(),
            currency_code: None,
            offer_limit: 5,
            poll_interval_ms: 5_000,
            network: Network::default(),
            rpc_server: None,
            ws_server: None,
            signing: RawSigningConfig::default(),
            oracles: Vec::new(),
            oracle_pairs: default_oracle_pairs(),
            slippage_bps: default_slippage_bps(),
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

/// Keys may only trigger payload-free navigation/refresh actions. Anything carrying data
/// (incl. every `*Submit` write) must come from a UI flow with its own confirmation.
fn is_bindable(action: &Action) -> bool {
    matches!(
        action,
        Action::Quit
            | Action::Suspend
            | Action::ClearScreen
            | Action::Render
            | Action::Help
            | Action::RefreshAccount
            | Action::RefreshBook
            | Action::RefreshNfts
            | Action::RefreshLines
            | Action::RefreshTxHistory
            | Action::RefreshLedgerObjects
            | Action::TabNext
            | Action::TabPrev
            | Action::FocusNext
            | Action::FocusPrev
            | Action::SelectNext
            | Action::SelectPrev
            | Action::NetworkSwitchCycle
            | Action::TxDetailToggle
    )
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
            if !is_bindable(&cmd) {
                return Err(D::Error::custom(format!(
                    "keybinding \"{key_str}\": action {cmd:?} cannot be bound to a key \
                     (only payload-free navigation/refresh actions are allowed)"
                )));
            }
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
            _ => break,
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
        Some(Color::Indexed(c.checked_add(8)?))
    } else if s.contains("color") {
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("gray") {
        let c = 232u8.checked_add(
            s.trim_start_matches("gray")
                .parse::<u8>()
                .unwrap_or_default(),
        )?;
        Some(Color::Indexed(c))
    } else if let Some(rgb_str) = s.strip_prefix("rgb") {
        if rgb_str.len() >= 3 {
            let bytes = rgb_str.as_bytes();
            let digit = |i: usize| (bytes[i] as char).to_digit(10).unwrap_or_default() as u8;
            let (red, green, blue) = (digit(0), digit(1), digit(2));
            // xterm 6x6x6 cube: each channel is 0..=5
            if red > 5 || green > 5 || blue > 5 {
                return None;
            }
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

    /// TC-019-022: parse_style — empty yields default, fg/bg mapping, modifiers combined, gray==grey
    #[test]
    fn parse_style_spec_mapping() {
        // Empty spec yields the default style.
        assert_eq!(parse_style(""), Style::default(), "empty style spec");

        let cases = [
            ("red", Some(Color::Indexed(1)), None),
            ("on blue", None, Some(Color::Indexed(4))),
            (
                "red on blue",
                Some(Color::Indexed(1)),
                Some(Color::Indexed(4)),
            ),
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
        // Out-of-range user values must not overflow u8 (panic in debug, wrap in release).
        assert_eq!(parse_color("rgb555"), Some(Color::Indexed(231)));
        assert_eq!(parse_color("rgb999"), None);
        assert_eq!(parse_color("gray0"), Some(Color::Indexed(232)));
        assert_eq!(parse_color("gray23"), Some(Color::Indexed(255)));
        assert_eq!(parse_color("gray24"), None);
        assert_eq!(parse_color("bright color7"), Some(Color::Indexed(15)));
        assert_eq!(parse_color("bright color250"), None);
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
            ("shift-a", Some((KeyCode::Char('A'), KeyModifiers::SHIFT))),
            ("backtab", Some((KeyCode::BackTab, KeyModifiers::SHIFT))),
            ("f1", Some((KeyCode::F(1), KeyModifiers::empty()))),
            ("f12", Some((KeyCode::F(12), KeyModifiers::empty()))),
            ("space", Some((KeyCode::Char(' '), KeyModifiers::empty()))),
            ("f13", None),
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

    /// TC-036
    #[test]
    fn parse_key_sequence_unbalanced_brackets_errors() {
        assert!(parse_key_sequence("<<q>").is_err());
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
