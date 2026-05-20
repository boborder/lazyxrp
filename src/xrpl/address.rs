//! Classic / X-address helpers for Payment destinations.

use crate::network::Network;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaymentDestination {
    pub classic: String,
    pub destination_tag: Option<u32>,
    /// `Some` only when the input was an X-address (network bit is meaningful).
    pub xaddress_is_test: Option<bool>,
}

pub fn resolve_payment_destination(
    trimmed: &str,
) -> color_eyre::Result<ResolvedPaymentDestination> {
    use xrpl::core::addresscodec::{
        is_valid_classic_address, is_valid_xaddress, xaddress_to_classic_address,
    };
    if trimmed.is_empty() {
        return Err(color_eyre::eyre::eyre!("destination is empty"));
    }
    if is_valid_classic_address(trimmed) {
        return Ok(ResolvedPaymentDestination {
            classic: trimmed.to_string(),
            destination_tag: None,
            xaddress_is_test: None,
        });
    }
    if is_valid_xaddress(trimmed) {
        let (classic, tag, is_test_network) = xaddress_to_classic_address(trimmed)
            .map_err(|e| color_eyre::eyre::eyre!("invalid X-address: {e:?}"))?;
        let destination_tag = match tag {
            None => None,
            Some(t) if t <= u64::from(u32::MAX) => Some(t as u32),
            Some(t) => {
                return Err(color_eyre::eyre::eyre!(
                    "X-address destination tag out of range: {t}"
                ));
            }
        };
        return Ok(ResolvedPaymentDestination {
            classic,
            destination_tag,
            xaddress_is_test: Some(is_test_network),
        });
    }
    Err(color_eyre::eyre::eyre!(
        "invalid destination (need classic `r…` or X-address)"
    ))
}

pub fn ensure_xaddress_matches_network(
    resolved: &ResolvedPaymentDestination,
    network: &Network,
) -> color_eyre::Result<()> {
    let Some(is_test) = resolved.xaddress_is_test else {
        return Ok(());
    };
    if is_test && network.is_mainnet() {
        return Err(color_eyre::eyre::eyre!(
            "X-address is for a test network, but connected network is mainnet"
        ));
    }
    if !is_test && !network.is_mainnet() {
        return Err(color_eyre::eyre::eyre!(
            "X-address is for mainnet, but connected network is {}",
            network.display_name()
        ));
    }
    Ok(())
}
