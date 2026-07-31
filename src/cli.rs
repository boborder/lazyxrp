use clap::{Parser, Subcommand};

use crate::{
    config::{config_dir, data_dir},
    network::Network,
};

#[derive(Parser, Debug)]
#[command(name = "lazyxrp", author, version = version(), about)]
pub struct Cli {
    /// Tick rate, i.e. number of ticks per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 4.0)]
    pub tick_rate: f64,

    /// Frame rate, i.e. number of frames per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 60.0)]
    pub frame_rate: f64,

    #[arg(long)]
    pub server: Option<String>,

    #[arg(long)]
    pub ws_server: Option<String>,

    /// Network to connect to (overrides config and env)
    #[arg(long, value_enum)]
    pub network: Option<Network>,

    /// Skip confirmation prompts (mainnet writes and `lazyxrp --self-uninstall`)
    #[arg(long, default_value_t = false)]
    pub yes: bool,

    /// Remove this executable (and `{name}.bak` next to it) plus resolved config/data dirs. Does not run `cargo uninstall`; see README / `./install.sh --uninstall-help`. Conflicts with subcommands.
    #[arg(long)]
    pub self_uninstall: bool,

    /// Signing seed (family seed format). Overrides XRPL_SEED env var and config.
    #[arg(long)]
    pub seed: Option<String>,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

/// Short-command CLI when argv0 is `rp` (symlink to lazyxrp).
#[derive(Parser, Debug)]
#[command(
    name = "rp",
    about = "Quick XRPL lookup — transaction hash or account address"
)]
pub struct RpCli {
    /// Network to connect to (overrides config and env)
    #[arg(long, value_enum)]
    pub network: Option<Network>,

    #[arg(long)]
    pub server: Option<String>,

    /// Target: 64-char tx hash or classic/X address
    #[arg(short = 't', long = "target", value_name = "TXID_OR_ADDRESS")]
    pub target: Option<String>,

    /// Positional alternative to `-t` / `--target`
    #[arg(value_name = "TXID_OR_ADDRESS")]
    pub query: Option<String>,
}

impl RpCli {
    pub fn resolved_target(&self) -> color_eyre::Result<&str> {
        self.target
            .as_deref()
            .or(self.query.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "rp: pass `-t <txid|address>` or a positional `<txid|address>`"
                )
            })
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum Cmd {
    Watch {
        #[arg(long)]
        account: Option<String>,
    },
    Info,
    Account {
        address: String,
    },
    Book {
        #[arg(long)]
        base: String,
        #[arg(long)]
        quote: String,
        #[arg(long)]
        issuer: Option<String>,
        #[arg(long, default_value_t = 5)]
        limit: u16,
    },
    Summary {
        #[arg(long)]
        account: Option<String>,
    },
    /// List NFTs owned by an account
    Nfts {
        address: String,
    },
    /// List trust lines for an account
    Lines {
        address: String,
    },
    /// Show AMM pool info for a currency pair
    Amm {
        #[arg(long)]
        asset1: String,
        #[arg(long)]
        asset2: String,
        #[arg(long)]
        issuer1: Option<String>,
        #[arg(long)]
        issuer2: Option<String>,
    },
    /// Show recent transactions for an account
    TxHistory {
        address: String,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
    /// Check if an account is activated (has XRP balance >= 10 XRP)
    AccountStatus {
        address: String,
    },
    /// Send XRP to a destination address
    Send {
        /// Destination account address
        destination: String,
        /// Amount in XRP (default: 0.000123)
        #[arg(long, default_value = "0.000123")]
        amount: String,
    },
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_DESCRIBE"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

pub fn version() -> String {
    let author = clap::crate_authors!();

    // let current_exe_path = PathBuf::from(clap::crate_name!()).display().to_string();
    let config_dir_path = config_dir().display().to_string();
    let data_dir_path = data_dir().display().to_string();

    format!(
        "\
{VERSION_MESSAGE}

Authors: {author}

Config directory: {config_dir_path}
Data directory: {data_dir_path}"
    )
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, RpCli};

    /// TC-058
    #[test]
    fn book_requires_base_and_quote_arguments() {
        assert!(Cli::try_parse_from(["lazyxrp", "book", "--quote", "USD"]).is_err());
        assert!(Cli::try_parse_from(["lazyxrp", "book", "--base", "XRP"]).is_err());
    }

    #[test]
    fn self_uninstall_accepts_optional_yes_flag() {
        let c = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "--yes"]).expect("parses");
        assert!(c.self_uninstall);
        assert!(c.yes);
    }

    #[test]
    fn self_uninstall_plus_subcommand_is_parseable_but_invalid_at_runtime() {
        let c = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "info"]).unwrap();
        assert!(c.self_uninstall);
        assert!(matches!(c.command, Some(super::Cmd::Info)));
    }

    #[test]
    fn rp_cli_accepts_flag_and_positional() {
        let flag = RpCli::try_parse_from(["rp", "-t", "abcd"]).expect("flag");
        assert_eq!(flag.resolved_target().unwrap(), "abcd");
        let pos = RpCli::try_parse_from(["rp", "ef01"]).expect("pos");
        assert_eq!(pos.resolved_target().unwrap(), "ef01");
        assert!(
            RpCli::try_parse_from(["rp"])
                .unwrap()
                .resolved_target()
                .is_err()
        );
    }
}
