use clap::{Parser, Subcommand};

use crate::{
    config::{config_dir, data_dir},
    network::Network,
};

const CLI_AFTER_HELP: &str = "\
Install:   curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | bash\n\
Uninstall: lazyxrp --self-uninstall [--yes]  (manual: ./install.sh --uninstall-help)\n\
Docs:      https://github.com/boborder/lazyxrp";

#[derive(Parser, Debug)]
#[command(
    name = "lazyxrp",
    author,
    version = version(),
    about = "Terminal UI for the XRP Ledger — monitor accounts, order books, NFTs, trust lines, and AMM pools",
    after_help = CLI_AFTER_HELP
)]
pub struct Cli {
    /// Tick rate, i.e. number of ticks per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 4.0)]
    pub tick_rate: f64,

    /// Custom JSON-RPC URL (overrides config and env)
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,

    /// Custom WebSocket URL (overrides config and env)
    #[arg(long, value_name = "URL")]
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
    ///
    /// Deprecated: appears in process argv / shell history. Prefer XRPL_SEED or config.
    #[arg(long, conflicts_with = "mnemonic")]
    pub seed: Option<String>,

    /// BIP39 mnemonic for XRPL secp256k1 account index 0.
    ///
    /// Deprecated: appears in process argv / shell history. Prefer XRPL_MNEMONIC or config.
    #[arg(long, conflicts_with = "seed")]
    pub mnemonic: Option<String>,

    /// Allow `http://` / `ws://` custom RPC/WS endpoints (default: https/wss only).
    #[arg(long, default_value_t = false)]
    pub allow_insecure_rpc: bool,

    /// Watch account for the TUI (overrides config). Prefer over legacy `watch --account`.
    #[arg(long, global = true)]
    pub account: Option<String>,

    /// Run a non-interactive script subcommand (e.g. `lazyxrp -x info`). Shell alias: `rc='lazyxrp -x'`.
    #[arg(short = 'x', long = "exec", conflicts_with = "self_uninstall")]
    pub exec: bool,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

const RP_AFTER_HELP: &str = "\
Examples:\n  \
rp -t <64-char-tx-hash>\n  \
rp <classic-or-x-address>\n\n\
Full TUI: lazyxrp --help";

/// Short-command CLI when argv0 is `rp` (symlink to lazyxrp).
#[derive(Parser, Debug)]
#[command(
    name = "rp",
    about = "Quick XRPL lookup — transaction hash or account address",
    after_help = RP_AFTER_HELP
)]
pub struct RpCli {
    /// Network to connect to (overrides config and env)
    #[arg(long, value_enum)]
    pub network: Option<Network>,

    #[arg(long)]
    pub server: Option<String>,

    /// Allow `http://` custom RPC endpoint (default: https only).
    #[arg(long, default_value_t = false)]
    pub allow_insecure_rpc: bool,

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
    /// Launch the TUI (default when no subcommand). Deprecated: use bare `lazyxrp` instead.
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

impl Cmd {
    pub fn subcommand_name(&self) -> &'static str {
        match self {
            Cmd::Watch { .. } => "watch",
            Cmd::Info => "info",
            Cmd::Account { .. } => "account",
            Cmd::Book { .. } => "book",
            Cmd::Summary { .. } => "summary",
            Cmd::Nfts { .. } => "nfts",
            Cmd::Lines { .. } => "lines",
            Cmd::Amm { .. } => "amm",
            Cmd::TxHistory { .. } => "tx-history",
            Cmd::AccountStatus { .. } => "account-status",
            Cmd::Send { .. } => "send",
        }
    }
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
    use crate::network::Network;

    /// TC-058
    #[test]
    fn book_requires_base_and_quote_arguments() {
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "book", "--quote", "USD"]).is_err());
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "book", "--base", "XRP"]).is_err());
        // Deprecated bare form still parses until removal.
        assert!(Cli::try_parse_from(["lazyxrp", "book", "--quote", "USD"]).is_err());
    }

    #[test]
    fn exec_flag_parses_with_script_subcommand() {
        let c = Cli::try_parse_from(["lazyxrp", "-x", "info"]).expect("parses");
        assert!(c.exec);
        assert!(matches!(c.command, Some(super::Cmd::Info)));
    }

    #[test]
    fn exec_flag_accepts_global_network_before_subcommand() {
        let c =
            Cli::try_parse_from(["lazyxrp", "--network", "testnet", "-x", "info"]).expect("parses");
        assert!(c.exec);
        assert_eq!(c.network, Some(Network::Testnet));
    }

    #[test]
    fn top_level_account_flag_parses_for_tui() {
        let c = Cli::try_parse_from(["lazyxrp", "--account", "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"])
            .expect("parses");
        assert_eq!(
            c.account.as_deref(),
            Some("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh")
        );
        assert!(c.command.is_none());
        assert!(!c.exec);
    }

    #[test]
    fn exec_conflicts_with_self_uninstall() {
        assert!(Cli::try_parse_from(["lazyxrp", "--self-uninstall", "-x", "info"]).is_err());
    }

    #[test]
    fn self_uninstall_accepts_optional_yes_flag() {
        let c = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "--yes"]).expect("parses");
        assert!(c.self_uninstall);
        assert!(c.yes);
    }

    #[test]
    fn self_uninstall_plus_subcommand_parses() {
        let c = Cli::try_parse_from(["lazyxrp", "--self-uninstall", "info"]).unwrap();
        assert!(c.self_uninstall);
        assert!(matches!(c.command, Some(super::Cmd::Info)));
    }

    /// TC-125: `--network` public values include `xahau` and `xahau-test`
    #[test]
    fn network_flag_accepts_xahau_and_xahau_test() {
        let xahau = Cli::try_parse_from(["lazyxrp", "--network", "xahau"]).expect("parses");
        assert_eq!(xahau.network, Some(Network::Xahau));
        let test = Cli::try_parse_from(["lazyxrp", "--network", "xahau-test"]).expect("parses");
        assert_eq!(test.network, Some(Network::XahauTest));
        assert!(Cli::try_parse_from(["lazyxrp", "--network", "xahau-testnet"]).is_err());
    }

    /// TC-126: `--allow-insecure-rpc` defaults off and the flag enables it
    #[test]
    fn allow_insecure_rpc_defaults_false_and_flag_enables_it() {
        let off = Cli::try_parse_from(["lazyxrp"]).expect("parses");
        assert!(!off.allow_insecure_rpc);
        let on = Cli::try_parse_from(["lazyxrp", "--allow-insecure-rpc"]).expect("parses");
        assert!(on.allow_insecure_rpc);
    }

    /// TC-126: `rp --allow-insecure-rpc` parses
    #[test]
    fn rp_allow_insecure_rpc_flag_parses() {
        let off = RpCli::try_parse_from(["rp", "-t", "abcd"]).expect("parses");
        assert!(!off.allow_insecure_rpc);
        let on =
            RpCli::try_parse_from(["rp", "--allow-insecure-rpc", "-t", "abcd"]).expect("parses");
        assert!(on.allow_insecure_rpc);
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
