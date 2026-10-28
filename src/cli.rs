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

    /// `-x` writes: return right after submit instead of waiting for the validated result
    #[arg(long, default_value_t = false)]
    pub no_wait: bool,

    /// Remove this executable (and `{name}.bak` next to it) plus resolved config/data dirs. Does not run `cargo uninstall`; see README / `./install.sh --uninstall-help`. Conflicts with subcommands.
    #[arg(long)]
    pub self_uninstall: bool,

    /// Signing seed (family seed format). Overrides any XRPL_SEED / XRPL_MNEMONIC env and config credential.
    ///
    /// Deprecated: appears in process argv / shell history. Prefer XRPL_SEED or config.
    #[arg(long, conflicts_with = "mnemonic")]
    pub seed: Option<String>,

    /// BIP39 mnemonic for XRPL secp256k1 account index 0. Overrides any XRPL_SEED / XRPL_MNEMONIC env and config credential.
    ///
    /// Deprecated: appears in process argv / shell history. Prefer XRPL_MNEMONIC or config.
    #[arg(long, conflicts_with = "seed")]
    pub mnemonic: Option<String>,

    /// Allow `http://` / `ws://` custom RPC/WS endpoints (default: https/wss only).
    #[arg(long, default_value_t = false)]
    pub allow_insecure_rpc: bool,

    /// Read-only: never load the signing key (env, config, `--seed`/`--mnemonic`) or the Flare
    /// EVM key. The TUI shows the account panel (default: the key's address unless `--account`);
    /// `-x` writes are refused.
    #[arg(long, default_value_t = false)]
    pub watch: bool,

    /// Watch account for the TUI (overrides config).
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

/// Short-command CLI: the `rp` binary, or argv0 `rp` (symlink to lazyxrp).
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
    /// Check whether an account exists on the ledger (has been funded)
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
    /// List an account's open DEX offers (`account_offers`, all pages)
    Offers {
        address: String,
    },
    /// Cancel one of the signing account's offers by sequence (simulate first; mainnet needs --yes)
    OfferCancel {
        /// Offer sequence (`seq` from `offers`)
        sequence: u32,
    },
    /// Place a DEX offer (simulate first; mainnet needs --yes). Specs: `XRP:drops` or `CUR:issuer:value`
    OfferCreate {
        /// What you give (TakerGets), e.g. `XRP:1000000`
        taker_gets: String,
        /// What you want (TakerPays), e.g. `USD:rIssuer:1`
        taker_pays: String,
    },
    /// Two-asset AMM deposit (simulate first; mainnet needs --yes). Both specs are maxima:
    /// `XRP:drops` or `CUR:issuer:value`; their assets name the pool
    AmmDeposit {
        /// e.g. `XRP:1000000`
        amount: String,
        /// e.g. `USD:rIssuer:1`
        amount2: String,
    },
    /// AMM withdraw (simulate first; mainnet needs --yes). Assets: `XRP` or `CUR:issuer`.
    /// Without `--lp`, returns all LP tokens
    AmmWithdraw {
        asset: String,
        asset2: String,
        /// LP tokens to return (proportional withdrawal)
        #[arg(long)]
        lp: Option<String>,
    },
    /// Delete the signing account; the remaining XRP goes to DESTINATION (simulate first; mainnet
    /// needs --yes). Irreversible. Use an X-address to set a destination tag
    AccountDelete {
        /// Classic address, or X-address carrying a destination tag
        destination: String,
    },
    /// Print the XRPL transaction coverage table (Markdown; same as ROADMAP.md "XRPL coverage")
    Coverage,
    /// Any spec'd transaction type: `tx <TYPE> Field=value …` (simulate first; mainnet needs
    /// --yes). Types and fields: `docs/architecture.md` §2 / ROADMAP "XRPL coverage".
    /// Example: `tx NFTokenBurn NFTokenID=000B…`
    Tx {
        /// Transaction type, e.g. `NFTokenMint` (case-insensitive; `nft-mint` works)
        tx_type: String,
        /// `Field=value` pairs (names case-insensitive)
        #[arg(value_name = "FIELD=VALUE")]
        fields: Vec<String>,
    },
    /// Mint an NFT (NFTokenMint; simulate first, mainnet needs --yes). `--uri` is plain text
    NftMint {
        /// Metadata URI as plain text (stored hex-encoded, ≤ 256 bytes)
        #[arg(long)]
        uri: Option<String>,
        /// NFTokenTaxon (collection id)
        #[arg(long, default_value_t = 0)]
        taxon: u32,
        /// Comma-separated: burnable, onlyXRP, transferable
        #[arg(long)]
        flags: Option<String>,
        /// TransferFee 0..=50000 (needs `transferable`)
        #[arg(long)]
        transfer_fee: Option<u32>,
        /// Mint on behalf of this issuer
        #[arg(long)]
        issuer: Option<String>,
    },
    /// Burn an NFT (NFTokenBurn; irreversible, simulate first, mainnet needs --yes)
    NftBurn {
        /// 64-hex NFTokenID
        nftoken_id: String,
        /// Current holder when burning as issuer
        #[arg(long)]
        owner: Option<String>,
    },
    /// Fund an account from the public faucet (testnet/devnet only). No ADDRESS: generate a fresh
    /// wallet locally (seed printed once, never saved or sent).
    Faucet {
        /// Existing classic address to fund
        address: Option<String>,
    },
    /// Read-only Flare FXRP report (JSON): Direct Mint fees / pause / limits, MintingTagManager
    /// (optionally one tag) and the global redemption queue. Uses `FLARE_RPC_URL` or `[flare] network`.
    Fxrp {
        /// Minting tag ID to look up (owner, recipient, allowed executor)
        #[arg(long)]
        tag: Option<u64>,
    },
    /// Prove an XRPL Payment with the Flare Data Connector (verifier → FdcHub → DA layer) and run
    /// `executeDirectMinting` with it: the one-command FXRP Direct Mint finalize (C3).
    /// Pays the attestation fee in FLR from the Flare EVM key; needs `[flare.fassets] execute = true`.
    FxrpExecute {
        /// XRPL transaction hash of the Payment to the Core Vault (64 hex)
        tx_hash: String,
        /// Use `executeDirectMintingWithData` with these bytes (Smart Account custom instruction)
        #[arg(long, value_name = "HEX")]
        data: Option<String>,
        /// Resume a run that already paid the attestation: reuse this voting round instead of
        /// paying again (the round is printed right after the attestation is requested)
        #[arg(long)]
        round: Option<u64>,
    },
    /// Burn FXRP and have an agent pay XRP to an XRPL address (`redeemAmount`; `redeemWithTag`
    /// with `--tag`). Needs `[flare.fassets] execute = true`; `--yes` on Flare mainnet.
    FxrpRedeem {
        /// FXRP amount in XRP units, e.g. 12.5 (at least `minimumRedeemAmountUBA`)
        amount: String,
        /// Classic XRPL r-address that receives the XRP
        to: String,
        /// XRPL destination tag for the agent's payment (exchange deposits)
        #[arg(long)]
        tag: Option<u64>,
    },
    /// Reserve a minting tag (`MintingTagManager.reserve()`, pays the reservation fee in FLR).
    FxrpTagReserve,
    /// Set a tag's minting recipient and / or allowed executor (tag owner only; an executor
    /// change activates after the cooldown).
    FxrpTagSet {
        tag: u64,
        /// Flare address that receives FXRP minted with this tag
        #[arg(long)]
        recipient: Option<String>,
        /// Only this Flare address may execute mints for the tag
        #[arg(long)]
        executor: Option<String>,
    },
    /// Transfer a minting tag you own to another Flare address (irreversible; resets recipient
    /// and executor).
    FxrpTagTransfer {
        tag: u64,
        to: String,
    },
    /// Smart Account vault instruction as an XRPL Payment to the operator: Firelight / Upshift
    /// deposit, request-redeem or claim. `--value` is the instruction's value field verbatim
    /// (upstream `Instructions.sol`: deposit = FXRP base units, redeem = shares, claim = period / date).
    SaVault {
        #[arg(value_enum)]
        protocol: VaultProtocol,
        #[arg(value_enum)]
        action: VaultAction,
        /// Vault id from `MasterAccountController.getVaults()`
        vault_id: u16,
        /// Value field (deposit: FXRP base units, 6 decimals; redeem: vault shares; Firelight claim: period; Upshift claim: YYYYMMDD)
        value: u128,
    },
    /// Smart Account memo instruction (recovery / executor pin): a Direct Mint Payment to the
    /// Core Vault carrying the 10–50 byte memo. The payment must mint a positive net amount
    /// (fee-only direct mints revert). After it validates this waits (up to 5 min, `--no-wait`
    /// skips) for a public executor to finalize it; otherwise finalize it yourself with
    /// `fxrp-execute <tx> --data 0x`. No destination tag.
    SaMemo {
        #[arg(value_enum)]
        op: MemoOp,
        /// XRP to send to the Core Vault (must exceed the Direct Mint fees + `--fee`)
        amount: String,
        /// Executor fee in UBA (drops) written into the memo header
        #[arg(long, default_value_t = 0)]
        fee: u64,
        /// ignore-memo / replace-fee: the stuck XRPL transaction hash
        #[arg(long)]
        target: Option<String>,
        /// set-nonce: the new nonce (> current, jump <= u32::MAX)
        #[arg(long)]
        nonce: Option<u64>,
        /// replace-fee: the replacement executor fee (UBA)
        #[arg(long)]
        new_fee: Option<u64>,
        /// set-executor: Flare address of the executor to pin
        #[arg(long)]
        executor: Option<String>,
    },
    /// Smart Account custom instruction `0xFE`: contract calls run from your personal account.
    /// Sends the 42-byte memo (keccak256 of the user operation) as a Direct Mint Payment and prints
    /// the user-operation bytes the executor must pass: `fxrp-execute <tx> --data <HEX>`.
    /// Calls carry no FLR value.
    SaCall {
        /// XRP to send to the Core Vault (must exceed the Direct Mint fees + `--fee`)
        amount: String,
        /// `TARGET:0xDATA` (repeatable); executed in order from the personal account
        #[arg(long = "call", required = true)]
        calls: Vec<String>,
        /// Executor fee in UBA (drops) written into the memo header
        #[arg(long, default_value_t = 0)]
        fee: u64,
    },
    /// Read-only: has a Smart Account payment (`sa-memo` / `sa-call`, or an operator instruction
    /// such as `sa-vault`) been finalized on Flare? (`MasterAccountController.isTransactionIdUsed`;
    /// plain recipient-memo mints are not tracked — checked on Coston2.)
    SaStatus {
        /// XRPL transaction hash of the Direct Mint Payment (64 hex)
        tx_hash: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum VaultProtocol {
    Firelight,
    Upshift,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum VaultAction {
    Deposit,
    RequestRedeem,
    Claim,
}

/// Direct-mint memo instructions of a Smart Account (`MemoInstructions.sol`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MemoOp {
    /// `0xE0`: skip the memo of a stuck XRPL payment on its next direct mint
    IgnoreMemo,
    /// `0xE1`: fast-forward the memo-instruction nonce
    SetNonce,
    /// `0xE2`: replace the executor fee of a stuck XRPL payment
    ReplaceFee,
    /// `0xD0`: pin one executor for the personal account
    SetExecutor,
    /// `0xD1`: remove the executor pin
    RemoveExecutor,
}

impl VaultProtocol {
    /// Smart Account instruction id: type nibble (`1` Firelight, `2` Upshift) + command nibble
    /// (`1` deposit, `2` redeem, `3` claim).
    #[must_use]
    pub fn instruction_id(self, action: VaultAction) -> u8 {
        let kind = match self {
            Self::Firelight => 0x10,
            Self::Upshift => 0x20,
        };
        kind | match action {
            VaultAction::Deposit => 1,
            VaultAction::RequestRedeem => 2,
            VaultAction::Claim => 3,
        }
    }
}

impl Cmd {
    pub fn subcommand_name(&self) -> &'static str {
        match self {
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
            Cmd::Offers { .. } => "offers",
            Cmd::OfferCancel { .. } => "offer-cancel",
            Cmd::OfferCreate { .. } => "offer-create",
            Cmd::AmmDeposit { .. } => "amm-deposit",
            Cmd::AmmWithdraw { .. } => "amm-withdraw",
            Cmd::AccountDelete { .. } => "account-delete",
            Cmd::Coverage => "coverage",
            Cmd::Tx { .. } => "tx",
            Cmd::NftMint { .. } => "nft-mint",
            Cmd::NftBurn { .. } => "nft-burn",
            Cmd::Faucet { .. } => "faucet",
            Cmd::Fxrp { .. } => "fxrp",
            Cmd::FxrpExecute { .. } => "fxrp-execute",
            Cmd::FxrpRedeem { .. } => "fxrp-redeem",
            Cmd::FxrpTagReserve => "fxrp-tag-reserve",
            Cmd::FxrpTagSet { .. } => "fxrp-tag-set",
            Cmd::FxrpTagTransfer { .. } => "fxrp-tag-transfer",
            Cmd::SaVault { .. } => "sa-vault",
            Cmd::SaMemo { .. } => "sa-memo",
            Cmd::SaCall { .. } => "sa-call",
            Cmd::SaStatus { .. } => "sa-status",
        }
    }

    /// Subcommands that sign a transaction (refused under `--watch`).
    pub fn signs(&self) -> bool {
        matches!(
            self,
            Cmd::Send { .. }
                | Cmd::OfferCancel { .. }
                | Cmd::OfferCreate { .. }
                | Cmd::AmmDeposit { .. }
                | Cmd::AmmWithdraw { .. }
                | Cmd::AccountDelete { .. }
                | Cmd::Tx { .. }
                | Cmd::NftMint { .. }
                | Cmd::NftBurn { .. }
                | Cmd::FxrpExecute { .. }
                | Cmd::FxrpRedeem { .. }
                | Cmd::FxrpTagReserve
                | Cmd::FxrpTagSet { .. }
                | Cmd::FxrpTagTransfer { .. }
                | Cmd::SaVault { .. }
                | Cmd::SaMemo { .. }
                | Cmd::SaCall { .. }
        )
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
        // Bare (no `-x`) form is also rejected by clap: `book` requires `--base`.
        assert!(Cli::try_parse_from(["lazyxrp", "book", "--quote", "USD"]).is_err());
    }

    /// TC-190: DEX CLI — `offers <ADDR>`, `offer-cancel <SEQ>`, `offer-create <GETS> <PAYS>` parse;
    /// non-numeric seq rejected.
    #[test]
    fn dex_subcommands_parse() {
        let c = Cli::try_parse_from([
            "lazyxrp",
            "-x",
            "offers",
            "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
        ])
        .expect("offers parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::Offers { ref address, .. })
                if address == "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"
        ));
        let c = Cli::try_parse_from(["lazyxrp", "-x", "offer-cancel", "42"]).expect("parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::OfferCancel { sequence: 42 })
        ));
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "offer-cancel", "abc"]).is_err());
        let c = Cli::try_parse_from(["lazyxrp", "-x", "offer-create", "XRP:1000000", "USD:rX:1"])
            .expect("offer-create parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::OfferCreate { ref taker_gets, ref taker_pays })
                if taker_gets == "XRP:1000000" && taker_pays == "USD:rX:1"
        ));
        let c = Cli::try_parse_from([
            "lazyxrp",
            "-x",
            "amm-withdraw",
            "XRP",
            "USD:rX",
            "--lp",
            "5",
        ])
        .expect("amm-withdraw parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::AmmWithdraw { ref lp, .. }) if lp.as_deref() == Some("5")
        ));
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "amm-deposit", "XRP:1"]).is_err());
        let c = Cli::try_parse_from(["lazyxrp", "-x", "faucet"]).expect("bare faucet parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::Faucet { address: None })
        ));
        let c = Cli::try_parse_from(["lazyxrp", "-x", "faucet", "rX"]).expect("faucet ADDR parses");
        assert!(matches!(
            c.command,
            Some(super::Cmd::Faucet { address: Some(ref a) }) if a == "rX"
        ));
    }

    /// TC-190 (extended): spec'd transaction commands — `tx <TYPE> Field=value…`, `nft-mint`
    /// flags, `nft-burn <ID> --owner`, `coverage`; `nft-mint` without options takes taxon 0;
    /// all but `coverage` sign (refused under `--watch`).
    #[test]
    fn spec_subcommands_parse() {
        use super::Cmd;
        let c = Cli::try_parse_from([
            "lazyxrp",
            "-x",
            "tx",
            "NFTokenBurn",
            "NFTokenID=00",
            "Owner=rX",
        ])
        .expect("tx parses");
        assert!(matches!(
            &c.command,
            Some(Cmd::Tx { tx_type, fields })
                if tx_type == "NFTokenBurn" && fields == &["NFTokenID=00", "Owner=rX"]
        ));
        let c = Cli::try_parse_from([
            "lazyxrp",
            "-x",
            "nft-mint",
            "--uri",
            "ipfs://x",
            "--taxon",
            "3",
            "--flags",
            "burnable,transferable",
            "--transfer-fee",
            "500",
            "--issuer",
            "rX",
        ])
        .expect("nft-mint parses");
        assert!(matches!(
            &c.command,
            Some(Cmd::NftMint { uri, taxon: 3, flags, transfer_fee: Some(500), issuer })
                if uri.as_deref() == Some("ipfs://x")
                    && flags.as_deref() == Some("burnable,transferable")
                    && issuer.as_deref() == Some("rX")
        ));
        let c = Cli::try_parse_from(["lazyxrp", "-x", "nft-mint"]).expect("bare nft-mint");
        assert!(matches!(
            c.command,
            Some(Cmd::NftMint {
                uri: None,
                taxon: 0,
                flags: None,
                transfer_fee: None,
                issuer: None
            })
        ));
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "nft-mint", "--taxon", "x"]).is_err());
        let c = Cli::try_parse_from(["lazyxrp", "-x", "nft-burn", "AB", "--owner", "rX"])
            .expect("nft-burn parses");
        assert!(matches!(
            &c.command,
            Some(Cmd::NftBurn { nftoken_id, owner })
                if nftoken_id == "AB" && owner.as_deref() == Some("rX")
        ));
        assert!(Cli::try_parse_from(["lazyxrp", "-x", "nft-burn"]).is_err());
        for (argv, signs) in [
            (vec!["tx", "NFTokenBurn"], true),
            (vec!["nft-mint"], true),
            (vec!["nft-burn", "AB"], true),
            (vec!["coverage"], false),
            (vec!["sa-status", "AB"], false),
            (vec!["sa-memo", "remove-executor", "1"], true),
        ] {
            let c = Cli::try_parse_from(["lazyxrp", "-x"].into_iter().chain(argv.clone()))
                .expect("parses");
            assert_eq!(c.command.expect("cmd").signs(), signs, "{argv:?}");
        }
    }

    #[test]
    fn exec_conflicts_with_self_uninstall() {
        assert!(Cli::try_parse_from(["lazyxrp", "--self-uninstall", "-x", "info"]).is_err());
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

    /// TC-126: --allow-insecure-rpc defaults off; flag enables TUI and rp
    #[test]
    fn allow_insecure_rpc_defaults_off_and_flag_enables_it() {
        let cases: &[(&[&str], bool)] = &[
            (&["lazyxrp"], false),
            (&["lazyxrp", "--allow-insecure-rpc"], true),
            (&["rp", "-t", "abcd"], false),
            (&["rp", "--allow-insecure-rpc", "-t", "abcd"], true),
        ];
        for &(args, expected) in cases {
            if args[0] == "rp" {
                let c = RpCli::try_parse_from(args).expect("parses");
                assert_eq!(c.allow_insecure_rpc, expected, "{args:?}");
            } else {
                let c = Cli::try_parse_from(args).expect("parses");
                assert_eq!(c.allow_insecure_rpc, expected, "{args:?}");
            }
        }
    }

    /// `-t` wins over the positional; a whitespace-only target is an error.
    #[test]
    fn rp_cli_accepts_flag_and_positional() {
        let flag = RpCli::try_parse_from(["rp", "-t", "abcd"]).expect("flag");
        assert_eq!(flag.resolved_target().unwrap(), "abcd");
        let pos = RpCli::try_parse_from(["rp", "ef01"]).expect("pos");
        assert_eq!(pos.resolved_target().unwrap(), "ef01");
        let both = RpCli::try_parse_from(["rp", "-t", "abcd", "ef01"]).expect("both");
        assert_eq!(both.resolved_target().unwrap(), "abcd");
        let blank = RpCli::try_parse_from(["rp", "-t", "  "]).expect("blank");
        assert!(blank.resolved_target().is_err());
        assert!(
            RpCli::try_parse_from(["rp"])
                .unwrap()
                .resolved_target()
                .is_err()
        );
    }
}
