//! **lazyxrp** — terminal UI and script CLI for the XRP Ledger (plus Xahau and Flare reads).
//!
//! This crate is an **application**. The library target only exists so the `lazyxrp` and `rp`
//! binaries share code; [`run`] and [`run_rp`] are its whole public API, with no stability
//! guarantee beyond them. Usage, keys, configuration and the script CLI (`lazyxrp -x …`) are
//! documented in the [README](https://github.com/boborder/lazyxrp#readme).
//!
//! ```text
//! cargo install lazyxrp --locked
//! lazyxrp                                   # TUI (mainnet by default)
//! lazyxrp --network testnet -x faucet       # fresh funded testnet wallet
//! lazyxrp -x send <r-address> --amount 1    # simulate → sign → submit → wait for validation
//! ```
//!
//! Safety model: every write is simulated before signing, mainnet/Xahau writes need `--yes` or a
//! confirmation, the endpoint's `network_id` must match the selected network, and signing over
//! plain `http://`/`ws://` is refused. See
//! [SECURITY.md](https://github.com/boborder/lazyxrp/blob/main/SECURITY.md).
use clap::Parser;
use cli::{Cli, Cmd, RpCli};
use config::Config;
use network::Network;
use std::env;
use std::path::Path;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod coverage;
mod errors;
mod fdc;
mod flare;
mod flare_cmd;
mod logging;
mod network;
mod signing;
#[cfg(test)]
mod test_support;
mod tui;
mod uninstall;
mod xrpl;

#[derive(Clone, Copy)]
enum EndpointKind {
    Http,
    Ws,
}

fn ensure_secure_endpoint(
    url: &str,
    kind: EndpointKind,
    allow_insecure: bool,
) -> color_eyre::Result<String> {
    let trimmed = url.trim();
    let ok = match kind {
        EndpointKind::Http => {
            trimmed.starts_with("https://") || (allow_insecure && trimmed.starts_with("http://"))
        }
        EndpointKind::Ws => {
            trimmed.starts_with("wss://") || (allow_insecure && trimmed.starts_with("ws://"))
        }
    };
    if ok {
        return Ok(trimmed.to_string());
    }
    let expected = match kind {
        EndpointKind::Http => "https:// (or http:// with --allow-insecure-rpc)",
        EndpointKind::Ws => "wss:// (or ws:// with --allow-insecure-rpc)",
    };
    color_eyre::eyre::bail!("refusing insecure RPC/WS URL '{trimmed}'; expected {expected}")
}

fn is_insecure_endpoint(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("ws://")
}

fn refuses_insecure_signing(rpc_url: &str, ws_url: &str, has_signing_credential: bool) -> bool {
    has_signing_credential && (is_insecure_endpoint(rpc_url) || is_insecure_endpoint(ws_url))
}

/// Flare RPC answers choose the Core Vault Payment destination and the EVM tx
/// target/fees, so the XRPL endpoint rules apply to it too.
fn check_flare_endpoint(
    url: &str,
    allow_insecure: bool,
    has_signing_credential: bool,
) -> color_eyre::Result<String> {
    let url = ensure_secure_endpoint(url, EndpointKind::Http, allow_insecure)?;
    if has_signing_credential && is_insecure_endpoint(&url) {
        color_eyre::eyre::bail!(
            "refusing signing operations over insecure Flare RPC; use an https:// endpoint"
        );
    }
    Ok(url)
}

fn sanitize_rate(rate: f64, default: f64) -> f64 {
    if rate.is_finite() && rate > 0.0 {
        rate.clamp(0.1, 120.0)
    } else {
        default
    }
}

fn resolve_ws_url(args: &Cli, config: &Config, network: &Network) -> String {
    args.ws_server
        .clone()
        .or_else(|| env::var(config::XRPL_WS_SERVER_ENV).ok())
        .or_else(|| config.xrpl.ws_server.clone())
        .unwrap_or_else(|| network.ws_url().to_string())
}

fn argv0_basename() -> String {
    env::args_os()
        .next()
        .map(|a| {
            Path::new(&a)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

fn is_rp_alias_bin(name: &str) -> bool {
    let stem = name.strip_suffix(".exe").unwrap_or(name);
    stem.eq_ignore_ascii_case("rp")
}

fn resolve_rpc_url(cli_server: Option<String>, config: &Config, network: &Network) -> String {
    cli_server
        .or_else(|| env::var(config::XRPL_RPC_SERVER_ENV).ok())
        .or_else(|| config.xrpl.rpc_server.clone())
        .unwrap_or_else(|| network.rpc_url().to_string())
}

async fn run_rp_inner() -> color_eyre::Result<()> {
    let args = RpCli::parse();
    let config = Config::new()?;
    crate::logging::init(config.resolved_data_dir())?;
    let network = args.network.unwrap_or(config.xrpl.network);
    let rpc_url = ensure_secure_endpoint(
        &resolve_rpc_url(args.server.clone(), &config, &network),
        EndpointKind::Http,
        args.allow_insecure_rpc,
    )?;
    let target = args.resolved_target()?;
    xrpl::execute_rp_lookup(&rpc_url, target).await
}

/// Entry for the `rp` binary (and argv0 `rp` symlink via [`run`]).
pub async fn run_rp() -> color_eyre::Result<()> {
    crate::errors::init()?;
    run_rp_inner().await
}

fn ensure_uninstall_standalone(args: &Cli) -> color_eyre::Result<()> {
    if args.command.is_some() {
        color_eyre::eyre::bail!("`--self-uninstall` cannot be combined with a subcommand");
    }
    Ok(())
}

/// Entry for the `lazyxrp` binary (TUI + full CLI). Also accepts argv0 `rp` symlink.
pub async fn run() -> color_eyre::Result<()> {
    crate::errors::init()?;

    if is_rp_alias_bin(&argv0_basename()) {
        return run_rp_inner().await;
    }

    let args = Cli::parse();
    if args.self_uninstall {
        ensure_uninstall_standalone(&args)?;
        let config = Config::new()?;
        return uninstall::perform_self_uninstall(&config, args.yes);
    }

    let mut config = Config::new()?;
    crate::logging::init(config.resolved_data_dir())?;
    if let Some(cli_seed) = args.seed.as_ref() {
        eprintln!(
            "lazyxrp: warning: --seed exposes the family seed in process argv and shell history; prefer XRPL_SEED or config"
        );
        let family_seed = cli_seed.trim();
        config.xrpl.signing.seed = None;
        config.xrpl.signing.secret_seed = if family_seed.is_empty() {
            None
        } else {
            Some(secrecy::SecretString::from(family_seed.to_string()))
        };
    }
    if let Some(cli_mnemonic) = args.mnemonic.as_ref() {
        eprintln!(
            "lazyxrp: warning: --mnemonic exposes the BIP39 phrase in process argv and shell history; prefer XRPL_MNEMONIC or config"
        );
        let mnemonic = cli_mnemonic.trim();
        config.xrpl.signing.mnemonic = None;
        config.xrpl.signing.secret_mnemonic = if mnemonic.is_empty() {
            None
        } else {
            Some(secrecy::SecretString::from(mnemonic.to_string()))
        };
    }
    let mut credential = signing::credential_from_secrets(
        config.xrpl.signing.secret_seed.as_ref(),
        config.xrpl.signing.secret_mnemonic.as_ref(),
    )?;
    let watch_address = if args.watch {
        let address = enter_watch_mode(&mut config, credential.as_ref())?;
        credential = None;
        address
    } else {
        None
    };
    let network = args.network.unwrap_or(config.xrpl.network);
    let rpc_url = ensure_secure_endpoint(
        &resolve_rpc_url(args.server.clone(), &config, &network),
        EndpointKind::Http,
        args.allow_insecure_rpc,
    )?;
    let ws_url = ensure_secure_endpoint(
        &resolve_ws_url(&args, &config, &network),
        EndpointKind::Ws,
        args.allow_insecure_rpc,
    )?;
    if refuses_insecure_signing(&rpc_url, &ws_url, credential.is_some()) {
        color_eyre::eyre::bail!(
            "refusing signing operations over insecure RPC/WS; use https:// and wss:// endpoints"
        );
    }
    let tick_rate = sanitize_rate(args.tick_rate, 4.0);
    let yes = args.yes;
    match resolve_lazyxrp_command(&args)? {
        None => {
            if let Some(url) = app::resolve_flare_rpc_url(&config.flare) {
                let evm_key_set = config.flare_evm_key.is_some();
                check_flare_endpoint(
                    &url,
                    args.allow_insecure_rpc,
                    credential.is_some() || evm_key_set,
                )?;
            }
            let mut app = app::App::new(
                tick_rate,
                rpc_url,
                ws_url,
                args.account.clone().or(watch_address),
                network,
                config,
                None,
                yes,
            )?;
            app.run().await?;
        }
        Some(
            cmd @ (Cmd::Fxrp { .. }
            | Cmd::FxrpExecute { .. }
            | Cmd::FxrpRedeem { .. }
            | Cmd::FxrpTagReserve
            | Cmd::FxrpTagSet { .. }
            | Cmd::FxrpTagTransfer { .. }
            | Cmd::SaVault { .. }
            | Cmd::SaMemo { .. }),
        ) => run_flare_command(cmd, &config, &args, &rpc_url, &network, credential).await?,
        Some(cmd) => {
            let wait = !args.no_wait;
            xrpl::execute_cli_command(cmd, &rpc_url, &network, credential, yes, wait).await?;
        }
    }
    Ok(())
}

/// XRPL Mainnet ↔ Flare, XRPL Testnet ↔ Coston2; anything else is refused before any RPC.
fn check_network_pair(xrpl: &Network, flare: config::FlareNetwork) -> color_eyre::Result<()> {
    match (xrpl, flare) {
        (Network::Mainnet, config::FlareNetwork::Flare)
        | (Network::Testnet, config::FlareNetwork::Coston2) => Ok(()),
        (x, f) => color_eyre::eyre::bail!(
            "XRPL {x:?} does not pair with [flare] network {f:?} (Mainnet ↔ flare, Testnet ↔ coston2)"
        ),
    }
}

fn check_xrpl_tx_hash(h: &str) -> color_eyre::Result<()> {
    let h = h.trim().trim_start_matches("0x");
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        color_eyre::eyre::bail!("tx_hash must be 64 hex characters (the XRPL transaction hash)");
    }
    Ok(())
}

/// Flare subcommands (`-x fxrp*`, `-x sa-vault`): handled before the XRPL client because they
/// need the Flare RPC / EVM key. Writes need `[flare.fassets] execute = true`, the Flare EVM key,
/// an https RPC and `--yes` on Flare mainnet; every EVM write estimates gas (= simulates) first.
async fn run_flare_command(
    cmd: Cmd,
    config: &Config,
    args: &Cli,
    xrpl_rpc_url: &str,
    xrpl_network: &Network,
    credential: Option<signing::SigningCredential>,
) -> color_eyre::Result<()> {
    use secrecy::ExposeSecret;

    let url = app::resolve_flare_rpc_url(&config.flare).ok_or_else(|| {
        color_eyre::eyre::eyre!("no Flare RPC: set [flare] network or FLARE_RPC_URL")
    })?;
    let writes = cmd.signs();
    match &cmd {
        Cmd::FxrpExecute { tx_hash, .. } => {
            check_network_pair(xrpl_network, config.flare.network)?;
            check_xrpl_tx_hash(tx_hash)?;
        }
        Cmd::FxrpRedeem { .. } | Cmd::SaVault { .. } | Cmd::SaMemo { .. } => {
            check_network_pair(xrpl_network, config.flare.network)?;
        }
        _ => {}
    }
    let url = check_flare_endpoint(&url, args.allow_insecure_rpc, writes)?;
    let flare_mainnet = matches!(config.flare.network, config::FlareNetwork::Flare);
    let say = |m: &str| eprintln!("{m}");
    let yes_gate = || {
        if flare_mainnet && !args.yes {
            color_eyre::eyre::bail!("Flare mainnet write: re-run with --yes to allow it");
        }
        Ok(())
    };
    // EVM writes: opt-in + key + mainnet confirmation, in that order.
    let evm_key = |cmd: &Cmd| -> color_eyre::Result<Option<String>> {
        if matches!(
            cmd,
            Cmd::Fxrp { .. } | Cmd::SaVault { .. } | Cmd::SaMemo { .. }
        ) {
            return Ok(None);
        }
        flare::ensure_fassets_execute_enabled(config.flare.fassets.execute)?;
        let key = config.flare_evm_key.as_ref().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "env {} was not set at startup (Flare EVM key)",
                config.flare.fassets.evm_key_env
            )
        })?;
        yes_gate()?;
        Ok(Some(key.expose_secret().to_string()))
    };
    let key = evm_key(&cmd)?;
    let key = key.as_deref().unwrap_or_default();

    match cmd {
        Cmd::Fxrp { tag } => {
            let report = flare::fetch_fxrp_report(&url, tag).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Cmd::FxrpExecute { tx_hash, data } => {
            let data = data
                .map(|d| {
                    hex::decode(d.trim().trim_start_matches("0x"))
                        .map(alloy::primitives::Bytes::from)
                })
                .transpose()
                .map_err(|e| color_eyre::eyre::eyre!("--data must be hex: {e}"))?;
            let cfg = fdc::fdc_config(config.flare.network, |v| env::var(v).ok())?;
            say(&format!(
                "FDC verifier {} · DA layer {} (attestation fee + gas are paid in FLR)",
                cfg.verifier_base, cfg.da_base
            ));
            let proof = fdc::prove_xrp_payment(&url, key, &cfg, &tx_hash, say).await?;
            say("proof ready; submitting executeDirectMinting…");
            let (hash, outcome) =
                flare::execute_direct_minting_with(&url, key, &proof, data).await?;
            if let flare::DirectMintOutcome::PaymentTooSmall { .. } = outcome {
                color_eyre::eyre::bail!("{hash} · {outcome}");
            }
            println!("{hash} · {outcome}");
        }
        Cmd::FxrpRedeem { amount, to, tag } => {
            let drops = xrpl::xrp_to_drops(amount.trim())?;
            let hash = flare_cmd::redeem_fxrp(&url, key, u128::from(drops), &to, tag, say).await?;
            println!("{hash}");
        }
        Cmd::FxrpTagReserve => {
            let (hash, tag) = flare_cmd::reserve_tag(&url, key, say).await?;
            println!("{hash} · reserved tag {tag}");
        }
        Cmd::FxrpTagSet {
            tag,
            recipient,
            executor,
        } => {
            let parse = |v: Option<String>| {
                v.map(|s| {
                    signing::parse_smart_account_recipient(&s).map(alloy::primitives::Address::from)
                })
                .transpose()
            };
            let hashes =
                flare_cmd::set_tag(&url, key, tag, parse(recipient)?, parse(executor)?, say)
                    .await?;
            println!("{}", hashes.join("\n"));
        }
        Cmd::FxrpTagTransfer { tag, to } => {
            let to = alloy::primitives::Address::from(signing::parse_smart_account_recipient(&to)?);
            let hash = flare_cmd::transfer_tag(&url, key, tag, to, say).await?;
            println!("{hash}");
        }
        Cmd::SaVault {
            protocol,
            action,
            vault_id,
            value,
        } => {
            let id = protocol.instruction_id(action);
            let kind = u8::from(matches!(protocol, cli::VaultProtocol::Upshift)) + 1;
            flare_cmd::ensure_vault_registered(&url, vault_id, kind).await?;
            let (operator, fee_drops) = flare_cmd::instruction_route(&url, id).await?;
            let reference = signing::encode_vault_instruction(id, 0, value, vault_id)?;
            say(&format!(
                "instruction 0x{id:02x} · vault {vault_id} · value {value} · memo {} · fee {} XRP → operator {operator}",
                signing::smart_account_memo_data(&reference),
                flare::uba_to_xrp_display(u128::from(fee_drops)),
            ));
            let params = xrpl::SmartAccountInstructionParams {
                redeem: false,
                value,
                recipient: String::new(),
                operator_xrpl: operator,
                fee_drops,
                vault: Some(xrpl::SmartAccountVaultOp {
                    instruction_id: id,
                    vault_id,
                }),
                skip_mainnet_prompt: args.yes,
            };
            let hash = xrpl::run_smart_account_instruction(
                xrpl_rpc_url,
                xrpl_network,
                params,
                config.flare.smart_accounts.expected_operator.as_deref(),
                credential.as_ref(),
            )
            .await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            println!("{hash}");
        }
        Cmd::SaMemo {
            op,
            amount,
            fee,
            target,
            nonce,
            new_fee,
            executor,
        } => {
            let info = flare::fetch_fxrp_direct_mint_info(&url).await?;
            if info.minting_paused == Some(true)
                || info.emergency_pause_level.is_some_and(|l| l > 0)
            {
                color_eyre::eyre::bail!(
                    "FXRP minting is paused on Flare; not sending XRP to the Core Vault"
                );
            }
            let me = credential
                .as_ref()
                .ok_or_else(|| color_eyre::eyre::eyre!("no signing key"))?
                .address()?;
            let target_id = target.as_deref().map(flare_cmd::parse_tx_id).transpose()?;
            let state = flare::read_memo_state(&url, &me, target_id).await?;
            flare_cmd::check_target_state(op, state.target_used)?;
            let current_nonce = Some(state.nonce);
            if state.executor != alloy::primitives::Address::ZERO {
                say(&format!(
                    "executor pinned to {:#x}: only it can finalize this payment (0xD0 / 0xD1 bypass the pin)",
                    state.executor
                ));
            } else if matches!(op, cli::MemoOp::SetExecutor) {
                say(
                    "no executor is pinned yet; after this, only the pinned one can finalize memo payments",
                );
            }
            let memo = flare_cmd::recovery_memo(
                op,
                fee,
                target.as_deref(),
                nonce,
                new_fee,
                executor.as_deref(),
                current_nonce,
            )?;
            say(&format!(
                "memo {memo} · {amount} XRP → Core Vault {} (finalize afterwards with `fxrp-execute <tx> --data 0x`)",
                info.core_vault_xrpl
            ));
            let params = xrpl::FxrpDirectMintPaymentParams {
                core_vault_xrpl: info.core_vault_xrpl.clone(),
                min_fee_uba: info.min_fee_uba,
                fee_bips: info.fee_bips,
                executor_fee_uba: u128::from(fee),
                flare_recipient: String::new(),
                amount_xrp: amount,
                custom_memo: Some(memo),
                skip_mainnet_prompt: args.yes,
            };
            let hash = xrpl::run_direct_mint_payment(
                xrpl_rpc_url,
                xrpl_network,
                params,
                config.flare.fassets.expected_core_vault.as_deref(),
                credential.as_ref(),
            )
            .await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            println!("{hash}");
        }
        _ => color_eyre::eyre::bail!("not a Flare subcommand"),
    }
    Ok(())
}

/// `--watch`: keep the signing key's address for display, drop every key from `config`.
fn enter_watch_mode(
    config: &mut Config,
    credential: Option<&signing::SigningCredential>,
) -> color_eyre::Result<Option<String>> {
    let address = credential.map(|c| c.address()).transpose()?;
    let s = &mut config.xrpl.signing;
    (s.seed, s.mnemonic, s.secret_seed, s.secret_mnemonic) = (None, None, None, None);
    config.flare_evm_key = None;
    Ok(address)
}

/// `None` = launch the TUI; `Some` = script subcommand (requires `-x`).
fn resolve_lazyxrp_command(args: &Cli) -> color_eyre::Result<Option<Cmd>> {
    match (args.command.clone(), args.exec) {
        (None, true) => color_eyre::eyre::bail!(
            "`-x` requires a script subcommand (e.g. `lazyxrp -x info`); omit `-x` to launch the TUI"
        ),
        (None, false) => Ok(None),
        (Some(cmd), false) => {
            let name = cmd.subcommand_name();
            color_eyre::eyre::bail!(
                "bare `{name}` subcommand was removed; use `lazyxrp -x {name}` (alias: rc='lazyxrp -x')"
            )
        }
        (Some(cmd), true) if args.watch && cmd.signs() => color_eyre::eyre::bail!(
            "`{}` signs a transaction; `--watch` is read-only",
            cmd.subcommand_name()
        ),
        (Some(cmd), true) => Ok(Some(cmd)),
    }
}

#[cfg(test)]
mod network_resolve_tests {
    use clap::Parser;

    use super::*;
    use crate::config::Config;
    use crate::test_support::{TestEnvGuard, env_lock};

    fn minimal_config_toml() -> &'static str {
        r#"[xrpl]
account = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"
issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"
currency = "RLUSD"
currency_code = "524C555344000000000000000000000000000000"
offer_limit = 5
poll_interval_ms = 5000
network = "mainnet"
"#
    }

    /// Temp dir with a minimal `config.toml`; caller sets `LAZYXRP_CONFIG`.
    fn temp_config_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("lazyxrp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.toml"), minimal_config_toml()).unwrap();
        root
    }

    /// TC-043: URL precedence cli > env > config file > network default, for HTTP and WS.
    #[test]
    fn resolve_urls_precedence_cli_env_config_default() {
        let _env_lock = env_lock();
        let _env = TestEnvGuard::new(&[config::XRPL_RPC_SERVER_ENV, config::XRPL_WS_SERVER_ENV]);
        _env.remove(config::XRPL_RPC_SERVER_ENV);
        _env.remove(config::XRPL_WS_SERVER_ENV);
        let cli = |extra: &[&str]| {
            Cli::try_parse_from(
                ["lazyxrp"]
                    .into_iter()
                    .chain(extra.iter().copied())
                    .chain(["info"]),
            )
            .unwrap()
        };
        let mut config = Config::default();
        let net = Network::Testnet;

        // Default rung: the selected network's endpoints (not mainnet's).
        assert_eq!(resolve_rpc_url(None, &config, &net), net.rpc_url());
        assert_eq!(resolve_ws_url(&cli(&[]), &config, &net), net.ws_url());
        assert_ne!(net.rpc_url(), Network::Mainnet.rpc_url());

        config.xrpl.rpc_server = Some("https://cfg".into());
        config.xrpl.ws_server = Some("wss://cfg".into());
        assert_eq!(resolve_rpc_url(None, &config, &net), "https://cfg");
        assert_eq!(resolve_ws_url(&cli(&[]), &config, &net), "wss://cfg");

        _env.set(config::XRPL_RPC_SERVER_ENV, "https://env");
        _env.set(config::XRPL_WS_SERVER_ENV, "wss://env");
        assert_eq!(resolve_rpc_url(None, &config, &net), "https://env");
        assert_eq!(resolve_ws_url(&cli(&[]), &config, &net), "wss://env");

        assert_eq!(
            resolve_rpc_url(Some("https://cli".into()), &config, &net),
            "https://cli"
        );
        assert_eq!(
            resolve_ws_url(&cli(&["--ws-server", "wss://cli"]), &config, &net),
            "wss://cli"
        );
    }
    /// TC-044
    #[test]
    fn resolve_ws_url_cli_overrides_env() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            config::XRPL_RPC_SERVER_ENV,
            config::XRPL_WS_SERVER_ENV,
        ]);
        let root = temp_config_root("tc044");
        _env.remove("XDG_CONFIG_HOME");
        _env.remove(config::XRPL_RPC_SERVER_ENV);
        _env.set(config::XRPL_WS_SERVER_ENV, "wss://custom");
        _env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let config = Config::new()?;
        let cli = Cli::try_parse_from(["lazyxrp", "--ws-server", "wss://cli", "info"])
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        let network = cli.network.unwrap_or(config.xrpl.network);
        let url = resolve_ws_url(&cli, &config, &network);
        assert_eq!(url, "wss://cli");
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }
}

#[cfg(test)]
mod endpoint_security_tests {
    use super::*;

    /// Insecure endpoint schemes are rejected unless the --allow-insecure flag is set
    #[test]
    fn insecure_endpoint_schemes_rejected_without_flag() {
        let cases = [
            (
                "http://example.com",
                EndpointKind::Http,
                "https://example.com",
            ),
            ("ws://example.com", EndpointKind::Ws, "wss://example.com"),
        ];
        for (insecure_url, kind, secure_url) in cases {
            let err = ensure_secure_endpoint(insecure_url, kind, false).unwrap_err();
            assert!(err.to_string().contains("refusing insecure"));
            assert!(ensure_secure_endpoint(secure_url, kind, false).is_ok());
            assert!(ensure_secure_endpoint(insecure_url, kind, true).is_ok());
        }
    }

    /// TC-101: signing seed refuses plaintext RPC or WebSocket endpoints
    #[test]
    fn insecure_signing_requires_secure_rpc_and_ws() {
        assert!(refuses_insecure_signing("http://rpc", "wss://ws", true));
        assert!(refuses_insecure_signing("https://rpc", "ws://ws", true));
        assert!(!refuses_insecure_signing("http://rpc", "ws://ws", false));
        assert!(!refuses_insecure_signing("https://rpc", "wss://ws", true));
    }

    /// Flare RPC follows the XRPL endpoint rules: https only, and no signing over http.
    #[test]
    fn flare_endpoint_requires_https_and_refuses_insecure_signing() {
        assert!(check_flare_endpoint("http://flare.example", false, false).is_err());
        assert!(check_flare_endpoint("https://flare.example", false, true).is_ok());
        assert!(check_flare_endpoint("http://flare.example", true, false).is_ok());
        assert!(check_flare_endpoint("http://flare.example", true, true).is_err());
    }
}

#[cfg(test)]
mod command_resolve_tests {
    use clap::Parser;

    use super::*;

    /// TC-189: bare `lazyxrp` → TUI (None) keeping `--account`; bare subcommand → error
    /// pointing at `-x`; `-x` alone → error; `-x info` → Some(Info).
    #[test]
    fn command_resolution() {
        let args =
            Cli::try_parse_from(["lazyxrp", "--account", "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"])
                .unwrap();
        assert!(resolve_lazyxrp_command(&args).unwrap().is_none());

        let args = Cli::try_parse_from(["lazyxrp", "info"]).unwrap();
        let err = resolve_lazyxrp_command(&args).expect_err("bare subcommand");
        assert!(err.to_string().contains("lazyxrp -x info"), "{err}");

        let args = Cli::try_parse_from(["lazyxrp", "-x"]).unwrap();
        assert!(resolve_lazyxrp_command(&args).is_err());

        let args = Cli::try_parse_from(["lazyxrp", "-x", "info"]).unwrap();
        assert!(matches!(
            resolve_lazyxrp_command(&args).unwrap(),
            Some(Cmd::Info)
        ));
    }

    /// TC-076: `--self-uninstall` combined with a subcommand is refused before any side effect.
    #[test]
    fn self_uninstall_with_subcommand_is_refused() {
        let args = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "info"]).unwrap();
        let err = ensure_uninstall_standalone(&args).expect_err("must refuse");
        assert!(err.to_string().contains("--self-uninstall"), "{err}");
        let args = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "--yes"]).unwrap();
        assert!(ensure_uninstall_standalone(&args).is_ok());
    }

    /// TC-234: `--watch` refuses every signing `-x` subcommand before any RPC; reads and the
    /// (non-signing) faucet still run; the TUI path is unaffected.
    #[test]
    fn watch_refuses_write_subcommands() {
        const DEST: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        for argv in [
            vec!["send", DEST],
            vec!["offer-cancel", "5"],
            vec!["offer-create", "XRP:1", "XRP:2"],
            vec!["amm-deposit", "XRP:1", "XRP:2"],
            vec!["amm-withdraw", "XRP", "XRP"],
            vec!["account-delete", DEST],
            vec!["tx", "NFTokenBurn", "NFTokenID=00"],
            vec!["nft-mint", "--taxon", "1"],
            vec!["nft-burn", "00"],
        ] {
            let args =
                Cli::try_parse_from(["lazyxrp", "--watch", "-x"].into_iter().chain(argv.clone()))
                    .unwrap();
            let err = resolve_lazyxrp_command(&args).expect_err("write under --watch");
            assert!(err.to_string().contains("--watch"), "{argv:?}: {err}");
        }
        for argv in [
            vec!["info"],
            vec!["offers", DEST],
            vec!["faucet"],
            vec!["coverage"],
        ] {
            let args =
                Cli::try_parse_from(["lazyxrp", "--watch", "-x"].into_iter().chain(argv.clone()))
                    .unwrap();
            assert!(
                resolve_lazyxrp_command(&args).unwrap().is_some(),
                "{argv:?}"
            );
        }
        let args = Cli::try_parse_from(["lazyxrp", "--watch"]).unwrap();
        assert!(resolve_lazyxrp_command(&args).unwrap().is_none());
    }

    /// TC-235: watch mode keeps only the address: every key (seed, mnemonic, plaintext fields,
    /// Flare EVM key) is dropped from the config the TUI / CLI receive.
    #[test]
    fn watch_mode_drops_all_keys_and_keeps_address() {
        use secrecy::SecretString;
        let mut config = Config::default();
        config.xrpl.signing.seed = Some("plain".into());
        config.xrpl.signing.mnemonic = Some("plain".into());
        config.xrpl.signing.secret_mnemonic = Some(SecretString::from("abandon about"));
        config.xrpl.signing.secret_seed =
            Some(SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB"));
        config.flare_evm_key = Some(SecretString::from("0x01"));
        let credential =
            signing::credential_from_secrets(config.xrpl.signing.secret_seed.as_ref(), None)
                .unwrap();
        let expected = credential.as_ref().unwrap().address().unwrap();

        let address = enter_watch_mode(&mut config, credential.as_ref()).unwrap();
        assert_eq!(address, Some(expected));
        assert!(config.xrpl.signing.seed.is_none() && config.xrpl.signing.mnemonic.is_none());
        assert!(config.xrpl.signing.secret_seed.is_none());
        assert!(config.xrpl.signing.secret_mnemonic.is_none());
        assert!(config.flare_evm_key.is_none());

        let mut empty = Config::default();
        assert_eq!(enter_watch_mode(&mut empty, None).unwrap(), None);
    }

    /// TC-343: XRPL ↔ Flare network pairing and tx-hash shape.
    #[test]
    fn flare_network_pair_and_tx_hash_checks() {
        use config::FlareNetwork::{Coston2, Flare, Songbird};
        assert!(check_network_pair(&Network::Mainnet, Flare).is_ok());
        assert!(check_network_pair(&Network::Testnet, Coston2).is_ok());
        for (x, f) in [
            (Network::Mainnet, Coston2),
            (Network::Testnet, Flare),
            (Network::Mainnet, Songbird),
            (Network::Devnet, Flare),
            (Network::Xahau, Flare),
        ] {
            assert!(check_network_pair(&x, f).is_err(), "{x:?}/{f:?}");
        }
        let h = "8B1625C463F2D71EA21806EB7B6C5D56642915EC8EDC0473A9F70CF9F92252E9";
        assert!(check_xrpl_tx_hash(h).is_ok());
        assert!(check_xrpl_tx_hash(&format!("0x{h}")).is_ok());
        for bad in ["zz", &h[1..], &format!("{h}0"), &h.replace('8', "g")] {
            assert!(check_xrpl_tx_hash(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sanitize_rate_rejects_non_finite_and_clamps() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
            assert_eq!(sanitize_rate(bad, 4.0), 4.0, "{bad}");
        }
        assert_eq!(sanitize_rate(1e9, 4.0), 120.0);
        assert_eq!(sanitize_rate(0.001, 4.0), 0.1);
        assert_eq!(sanitize_rate(10.0, 4.0), 10.0);
    }

    #[test]
    fn rp_alias_matches_only_rp_binary() {
        for yes in ["rp", "RP", "rp.exe", "RP.exe"] {
            assert!(is_rp_alias_bin(yes), "{yes}");
        }
        for no in ["lazyxrp", "rpc", "", "xrp"] {
            assert!(!is_rp_alias_bin(no), "{no}");
        }
    }
}
