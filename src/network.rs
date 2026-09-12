use serde::{Deserialize, Serialize};
use strum::EnumString;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumString, clap::ValueEnum,
)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Mainnet,
    Testnet,
    Devnet,
    #[strum(serialize = "xahau", ascii_case_insensitive)]
    #[serde(rename = "xahau")]
    Xahau,
    #[strum(serialize = "xahau-test", ascii_case_insensitive)]
    #[serde(rename = "xahau-test")]
    XahauTestnet,
}

impl Network {
    pub fn rpc_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "https://xrplcluster.com",
            Self::Testnet => "https://s.altnet.rippletest.net:51234",
            Self::Devnet => "https://s.devnet.rippletest.net:51234",
            Self::Xahau => "https://xahau.network",
            Self::XahauTestnet => "https://xahau-test.net",
        }
    }

    pub fn ws_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "wss://xrplcluster.com",
            Self::Testnet => "wss://s.altnet.rippletest.net:51233",
            Self::Devnet => "wss://s.devnet.rippletest.net:51233",
            Self::Xahau => "wss://xahau.network",
            Self::XahauTestnet => "wss://xahau-test.net",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Mainnet => "MAINNET",
            Self::Testnet => "TESTNET",
            Self::Devnet => "DEVNET",
            Self::Xahau => "XAHAU",
            Self::XahauTestnet => "XAHAU-TEST",
        }
    }

    pub fn is_mainnet(&self) -> bool {
        matches!(self, Self::Mainnet | Self::Xahau)
    }

    /// Cycle order for the TUI hotkey: mainnet→testnet→devnet→xahau→xahau-test→mainnet.
    pub fn next_network(&self) -> Self {
        match self {
            Self::Mainnet => Self::Testnet,
            Self::Testnet => Self::Devnet,
            Self::Devnet => Self::Xahau,
            Self::Xahau => Self::XahauTestnet,
            Self::XahauTestnet => Self::Mainnet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_mainnet() {
        assert_eq!(Network::default(), Network::Mainnet);
    }

    #[test]
    fn from_str_roundtrip() {
        assert_eq!("mainnet".parse::<Network>().unwrap(), Network::Mainnet);
        assert_eq!("testnet".parse::<Network>().unwrap(), Network::Testnet);
        assert_eq!("devnet".parse::<Network>().unwrap(), Network::Devnet);
        assert_eq!("xahau".parse::<Network>().unwrap(), Network::Xahau);
        assert_eq!(
            "xahau-test".parse::<Network>().unwrap(),
            Network::XahauTestnet
        );
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!("MAINNET".parse::<Network>().unwrap(), Network::Mainnet);
        assert_eq!("Testnet".parse::<Network>().unwrap(), Network::Testnet);
        assert_eq!("XAHAU".parse::<Network>().unwrap(), Network::Xahau);
        assert_eq!(
            "Xahau-Test".parse::<Network>().unwrap(),
            Network::XahauTestnet
        );
    }

    #[test]
    fn from_str_unknown_is_err() {
        assert!("foonet".parse::<Network>().is_err());
    }

    #[test]
    fn is_mainnet() {
        assert!(Network::Mainnet.is_mainnet());
        assert!(!Network::Testnet.is_mainnet());
        assert!(!Network::Devnet.is_mainnet());
        // Xahau mainnet is a production chain: mainnet write guard applies.
        assert!(Network::Xahau.is_mainnet());
        assert!(!Network::XahauTestnet.is_mainnet());
    }

    /// TC-111: Xahau networks parse, resolve endpoints, and guard writes
    #[test]
    fn xahau_network_endpoints() {
        assert_eq!(Network::Xahau.rpc_url(), "https://xahau.network");
        assert_eq!(Network::Xahau.ws_url(), "wss://xahau.network");
        assert_eq!(Network::Xahau.display_name(), "XAHAU");
        assert_eq!(Network::XahauTestnet.rpc_url(), "https://xahau-test.net");
        assert_eq!(Network::XahauTestnet.display_name(), "XAHAU-TEST");
    }

    /// next_network cycles through all variants and wraps to mainnet.
    #[test]
    fn next_network_wraps_all_variants() {
        let mut net = Network::Mainnet;
        for expected in [
            Network::Testnet,
            Network::Devnet,
            Network::Xahau,
            Network::XahauTestnet,
            Network::Mainnet,
        ] {
            net = net.next_network();
            assert_eq!(net, expected);
        }
    }

    /// TC-112: serde roundtrip for all networks including xahau variants
    #[test]
    fn serde_roundtrip_all() {
        for (net, s) in [
            (Network::Mainnet, "mainnet"),
            (Network::Testnet, "testnet"),
            (Network::Devnet, "devnet"),
            (Network::Xahau, "xahau"),
            (Network::XahauTestnet, "xahau-test"),
        ] {
            let json = serde_json::to_string(&net).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            assert_eq!(serde_json::from_str::<Network>(&json).unwrap(), net);
        }
    }
}
