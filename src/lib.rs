//! lazyxrp library — shared by the `lazyxrp` and `rp` binaries.
use clap::Parser;
use cli::{Cli, Cmd, RpCli};
use config::Config;
use network::Network;
use secrecy::ExposeSecret;
use std::env;
use std::path::Path;

mod action;
mod app;
mod cli;
mod components;
mod config;
pub mod errors;
mod flare;
mod logging;
mod network;
mod signing;
mod tui;
mod uninstall;
mod xrpl;

fn resolve_network(args: &Cli, config: &Config) -> Network {
    args.network
        .or_else(|| {
            env::var(config::XRPL_NETWORK_ENV)
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(config.xrpl.network)
}

fn resolve_rpc_url(args: &Cli, config: &Config, network: &Network) -> String {
    args.server
        .clone()
        .or_else(|| env::var(config::XRPL_RPC_SERVER_ENV).ok())
        .or_else(|| config.xrpl.rpc_server.clone())
        .unwrap_or_else(|| network.rpc_url().to_string())
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

fn resolve_network_opt(cli_network: Option<Network>, config: &Config) -> Network {
    cli_network
        .or_else(|| {
            env::var(config::XRPL_NETWORK_ENV)
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(config.xrpl.network)
}

fn resolve_rpc_url_opt(cli_server: Option<String>, config: &Config, network: &Network) -> String {
    cli_server
        .or_else(|| env::var(config::XRPL_RPC_SERVER_ENV).ok())
        .or_else(|| config.xrpl.rpc_server.clone())
        .unwrap_or_else(|| network.rpc_url().to_string())
}

async fn run_rp_inner() -> color_eyre::Result<()> {
    let args = RpCli::parse();
    let config = Config::new()?;
    crate::logging::init(config.resolved_data_dir())?;
    let network = resolve_network_opt(args.network, &config);
    let rpc_url = resolve_rpc_url_opt(args.server.clone(), &config, &network);
    let target = args.resolved_target()?;
    xrpl::execute_rp_lookup(&rpc_url, target).await
}

/// Entry for the `rp` binary (and argv0 `rp` symlink via [`run`]).
pub async fn run_rp() -> color_eyre::Result<()> {
    crate::errors::init()?;
    run_rp_inner().await
}

/// Entry for the `lazyxrp` binary (TUI + full CLI). Also accepts argv0 `rp` symlink.
pub async fn run() -> color_eyre::Result<()> {
    crate::errors::init()?;

    if is_rp_alias_bin(&argv0_basename()) {
        return run_rp_inner().await;
    }

    let args = Cli::parse();
    if args.self_uninstall {
        if args.command.is_some() {
            color_eyre::eyre::bail!("`--self-uninstall` cannot be combined with a subcommand");
        }
        let config = Config::new()?;
        return uninstall::perform_self_uninstall(&config, args.yes);
    }

    let mut config = Config::new()?;
    crate::logging::init(config.resolved_data_dir())?;
    if let Some(cli_seed) = args.seed.clone() {
        let t = signing::trim_family_seed(&cli_seed);
        config.xrpl.signing.seed = None;
        config.xrpl.signing.secret_seed = if t.is_empty() {
            None
        } else {
            Some(secrecy::SecretString::from(t.to_string()))
        };
    }
    let _ = signing::SigningConfig::prime_seed_source(
        config
            .xrpl
            .signing
            .secret_seed
            .as_ref()
            .map(|s| s.expose_secret().to_string()),
    );

    let network = resolve_network(&args, &config);
    let rpc_url = resolve_rpc_url(&args, &config, &network);
    let ws_url = resolve_ws_url(&args, &config, &network);
    let tick_rate = args.tick_rate;
    let frame_rate = args.frame_rate;
    let yes = args.yes;
    let cmd = args.command.unwrap_or(Cmd::Watch { account: None });
    match cmd {
        Cmd::Watch { account } => {
            let mut app = app::App::new(
                tick_rate, frame_rate, rpc_url, ws_url, account, network, config, None, yes,
            )?;
            app.run().await?;
        }
        other => {
            xrpl::execute_cli_command(
                other,
                &rpc_url,
                &network,
                config
                    .xrpl
                    .signing
                    .secret_seed
                    .as_ref()
                    .map(|s| s.expose_secret().to_string()),
                yes,
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod network_resolve_tests {
    use clap::Parser;

    use super::*;
    use crate::config::{Config, TestEnvGuard, env_lock};

    /// TC-042
    #[test]
    fn resolve_network_cli_overrides_env() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&["LAZYXRP_CONFIG", "LAZYXRP_DATA", config::XRPL_NETWORK_ENV]);
        _env.remove("LAZYXRP_CONFIG");
        _env.remove("LAZYXRP_DATA");
        let config = Config::new()?;
        let cli = Cli::try_parse_from(["lazyxrp", "--network", "devnet"])
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        _env.set(config::XRPL_NETWORK_ENV, "testnet");
        let n = resolve_network(&cli, &config);
        assert_eq!(n, Network::Devnet);
        Ok(())
    }

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

    /// TC-043
    #[test]
    fn resolve_rpc_url_uses_network_default() -> color_eyre::Result<()> {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "XDG_CONFIG_HOME",
            config::XRPL_RPC_SERVER_ENV,
            config::XRPL_WS_SERVER_ENV,
        ]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc043-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("config.toml"), minimal_config_toml())?;
        _env.remove("XDG_CONFIG_HOME");
        _env.remove(config::XRPL_RPC_SERVER_ENV);
        _env.remove(config::XRPL_WS_SERVER_ENV);
        _env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let config = Config::new()?;
        let cli =
            Cli::try_parse_from(["lazyxrp", "info"]).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        let network = resolve_network(&cli, &config);
        let url = resolve_rpc_url(&cli, &config, &network);
        assert_eq!(url, "https://xrplcluster.com");
        std::fs::remove_dir_all(&root).ok();
        Ok(())
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
        let root = std::env::temp_dir().join(format!("lazyxrp-tc044-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("config.toml"), minimal_config_toml())?;
        _env.remove("XDG_CONFIG_HOME");
        _env.remove(config::XRPL_RPC_SERVER_ENV);
        _env.set(config::XRPL_WS_SERVER_ENV, "wss://custom");
        _env.set("LAZYXRP_CONFIG", root.to_str().unwrap());
        let config = Config::new()?;
        let cli = Cli::try_parse_from(["lazyxrp", "--ws-server", "wss://cli", "info"])
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        let network = resolve_network(&cli, &config);
        let url = resolve_ws_url(&cli, &config, &network);
        assert_eq!(url, "wss://cli");
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }
}
