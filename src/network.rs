use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Mainnet,
    Testnet,
    Devnet,
}

impl Network {
    pub fn rpc_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "https://xrplcluster.com",
            Self::Testnet => "https://s.altnet.rippletest.net:51234",
            Self::Devnet => "https://s.devnet.rippletest.net:51234",
        }
    }

    pub fn ws_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "wss://xrplcluster.com",
            Self::Testnet => "wss://s.altnet.rippletest.net:51233",
            Self::Devnet => "wss://s.devnet.rippletest.net:51233",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Mainnet => "MAINNET",
            Self::Testnet => "TESTNET",
            Self::Devnet => "DEVNET",
        }
    }

    pub fn is_mainnet(&self) -> bool {
        matches!(self, Self::Mainnet)
    }
}

impl std::str::FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "devnet" => Ok(Self::Devnet),
            _ => Err(format!(
                "unknown network: {s}. expected mainnet, testnet, or devnet"
            )),
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
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!("MAINNET".parse::<Network>().unwrap(), Network::Mainnet);
        assert_eq!("Testnet".parse::<Network>().unwrap(), Network::Testnet);
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
    }
}
