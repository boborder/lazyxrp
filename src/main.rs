use clap::Parser;
use cli::{Cli, Cmd};
use config::Config;
use network::Network;
use std::env;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
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

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;

    let args = Cli::parse();
    if args.self_uninstall {
        if args.command.is_some() {
            color_eyre::eyre::bail!("`--self-uninstall` cannot be combined with a subcommand");
        }
        let config = Config::new()?;
        return uninstall::run_self_uninstall(&config, args.yes);
    }

    let mut config = Config::new()?;
    crate::logging::init(config.resolved_data_dir())?;
    if let Some(cli_seed) = args.seed.clone() {
        let t = signing::trim_family_seed(&cli_seed);
        config.xrpl.signing.seed = if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        };
    }
    let _ = signing::SigningConfig::prime_seed_source(config.xrpl.signing.seed.clone());

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
            xrpl::execute_cli_command(other, &rpc_url, &network, config.xrpl.signing.seed.clone())
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
        let cfg = Config::new()?;
        let cli = Cli::try_parse_from(["lazyxrp", "--network", "devnet"])
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        _env.set(config::XRPL_NETWORK_ENV, "testnet");
        let n = resolve_network(&cli, &cfg);
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
        let cfg = Config::new()?;
        let cli =
            Cli::try_parse_from(["lazyxrp", "info"]).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        let network = resolve_network(&cli, &cfg);
        let url = resolve_rpc_url(&cli, &cfg, &network);
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
        let cfg = Config::new()?;
        let cli = Cli::try_parse_from(["lazyxrp", "--ws-server", "wss://cli", "info"])
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        let network = resolve_network(&cli, &cfg);
        let url = resolve_ws_url(&cli, &cfg, &network);
        assert_eq!(url, "wss://cli");
        std::fs::remove_dir_all(&root).ok();
        Ok(())
    }
}
