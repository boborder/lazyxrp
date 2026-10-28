use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Mainnet,
    Testnet,
    Devnet,
    #[serde(rename = "xahau")]
    #[value(name = "xahau")]
    Xahau,
    #[serde(rename = "xahau-test")]
    #[value(name = "xahau-test")]
    XahauTest,
}

impl Network {
    pub fn rpc_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "https://xrplcluster.com",
            Self::Testnet => "https://s.altnet.rippletest.net:51234",
            Self::Devnet => "https://s.devnet.rippletest.net:51234",
            Self::Xahau => "https://xahau.network",
            Self::XahauTest => "https://xahau-test.net",
        }
    }

    pub fn ws_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "wss://xrplcluster.com",
            Self::Testnet => "wss://s.altnet.rippletest.net:51233",
            Self::Devnet => "wss://s.devnet.rippletest.net:51233",
            Self::Xahau => "wss://xahau.network",
            Self::XahauTest => "wss://xahau-test.net",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Mainnet => "MAINNET",
            Self::Testnet => "TESTNET",
            Self::Devnet => "DEVNET",
            Self::Xahau => "XAHAU",
            Self::XahauTest => "XAHAU-TEST",
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Mainnet | Self::Xahau)
    }

    /// `server_info.network_id` this network's nodes report (mainnet omits it: 0).
    pub fn network_id(&self) -> u32 {
        match self {
            Self::Mainnet => 0,
            Self::Testnet => 1,
            Self::Devnet => 2,
            Self::Xahau => 21337,
            Self::XahauTest => 21338,
        }
    }

    /// Refuse to sign when the endpoint's `network_id` is another network's (e.g. env
    /// `XRPL_RPC_SERVER` pointing at mainnet under `--network testnet` would skip `--yes`).
    pub fn check_endpoint_network_id(&self, reported: Option<u32>) -> color_eyre::Result<()> {
        let got = reported.unwrap_or(0);
        let want = self.network_id();
        if got != want {
            color_eyre::eyre::bail!(
                "endpoint network_id {got} is not {} (network_id {want}); refusing to sign — check --network / XRPL_RPC_SERVER / rpc_server",
                self.display_name()
            );
        }
        Ok(())
    }

    /// Cycle order for the TUI hotkey: mainnet→testnet→devnet→xahau→xahau-test→mainnet.
    pub fn next_network(&self) -> Self {
        match self {
            Self::Mainnet => Self::Testnet,
            Self::Testnet => Self::Devnet,
            Self::Devnet => Self::Xahau,
            Self::Xahau => Self::XahauTest,
            Self::XahauTest => Self::Mainnet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Case-insensitive parse (env `XRPL_NETWORK`); unknown names error.
    #[test]
    fn from_str_contract() {
        for (s, net) in [
            ("mainnet", Network::Mainnet),
            ("testnet", Network::Testnet),
            ("devnet", Network::Devnet),
            ("xahau", Network::Xahau),
            ("xahau-test", Network::XahauTest),
            ("MAINNET", Network::Mainnet),
            ("Testnet", Network::Testnet),
            ("XAHAU", Network::Xahau),
            ("Xahau-Test", Network::XahauTest),
        ] {
            assert_eq!(
                <Network as clap::ValueEnum>::from_str(s, true).unwrap(),
                net,
                "input {s:?}"
            );
        }
        assert!(<Network as clap::ValueEnum>::from_str("foonet", true).is_err());
    }

    #[test]
    fn is_production() {
        assert!(Network::Mainnet.is_production());
        assert!(!Network::Testnet.is_production());
        assert!(!Network::Devnet.is_production());
        // Xahau is a production chain: write guard applies.
        assert!(Network::Xahau.is_production());
        assert!(!Network::XahauTest.is_production());
    }

    /// TC-111: every network resolves distinct secure endpoints (xahau included).
    #[test]
    fn endpoints_are_secure_and_distinct() {
        let all = [
            Network::Mainnet,
            Network::Testnet,
            Network::Devnet,
            Network::Xahau,
            Network::XahauTest,
        ];
        for (i, a) in all.iter().enumerate() {
            assert!(a.rpc_url().starts_with("https://"), "{a:?}");
            assert!(a.ws_url().starts_with("wss://"), "{a:?}");
            for b in &all[i + 1..] {
                assert_ne!(a.rpc_url(), b.rpc_url(), "{a:?} vs {b:?}");
                assert_ne!(a.ws_url(), b.ws_url(), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn next_network_wraps_all_variants() {
        let mut net = Network::Mainnet;
        for expected in [
            Network::Testnet,
            Network::Devnet,
            Network::Xahau,
            Network::XahauTest,
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
            (Network::XahauTest, "xahau-test"),
        ] {
            let json = serde_json::to_string(&net).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            assert_eq!(serde_json::from_str::<Network>(&json).unwrap(), net);
        }
    }

    /// TC-203: endpoint `network_id` must match the selected network before signing
    /// (mainnet nodes omit it = 0); a mainnet node behind `--network testnet` is refused.
    #[test]
    fn endpoint_network_id_must_match() {
        for (net, ok, bad) in [
            (Network::Mainnet, Some(0), Some(1)),
            (Network::Testnet, Some(1), None),
            (Network::Devnet, Some(2), Some(1)),
            (Network::Xahau, Some(21337), Some(21338)),
            (Network::XahauTest, Some(21338), None),
        ] {
            assert!(net.check_endpoint_network_id(ok).is_ok(), "{net:?} {ok:?}");
            let err = net.check_endpoint_network_id(bad).expect_err("mismatch");
            assert!(err.to_string().contains("network_id"), "{err}");
        }
        assert!(Network::Mainnet.check_endpoint_network_id(None).is_ok());
    }
}
