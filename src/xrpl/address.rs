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
        // xrpl codec rejects tags > u32::MAX at decode (InvalidCAddressTag), so
        // the out-of-range arm below is defensive only — not reachable via real input.
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
    if is_test && network.is_production() {
        return Err(color_eyre::eyre::eyre!(
            "X-address is for a test network, but connected network is {}",
            network.display_name()
        ));
    }
    if !is_test && !network.is_production() {
        return Err(color_eyre::eyre::eyre!(
            "X-address is for a production network, but connected network is {}",
            network.display_name()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xrpl::core::addresscodec::classic_address_to_xaddress;

    const CLASSIC: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";

    #[test]
    fn resolve_empty_destination_is_rejected() {
        assert!(resolve_payment_destination("").is_err());
        assert!(resolve_payment_destination("   ").is_err());
    }

    #[test]
    fn resolve_invalid_destination_is_rejected() {
        let err = resolve_payment_destination("not-an-address").expect_err("invalid shape");
        assert!(err.to_string().contains("invalid destination"), "{err}");
    }

    #[test]
    fn resolve_xaddress_preserves_destination_tag() {
        let xaddr = classic_address_to_xaddress(CLASSIC, Some(42), false).expect("xaddr");
        let resolved = resolve_payment_destination(&xaddr).expect("resolve");
        assert_eq!(resolved.classic, CLASSIC);
        assert_eq!(resolved.destination_tag, Some(42));
        assert_eq!(resolved.xaddress_is_test, Some(false));
    }

    #[test]
    fn resolve_classic_address_has_no_tag() {
        let resolved = resolve_payment_destination(CLASSIC).expect("resolve");
        assert_eq!(resolved.classic, CLASSIC);
        assert!(resolved.destination_tag.is_none());
        assert!(resolved.xaddress_is_test.is_none());
    }

    #[test]
    fn xaddress_network_mismatch_is_rejected_in_both_directions() {
        let mainnet_xaddr = classic_address_to_xaddress(CLASSIC, None, false).expect("xaddr");
        let testnet_xaddr = classic_address_to_xaddress(CLASSIC, None, true).expect("xaddr");

        let resolved = resolve_payment_destination(&testnet_xaddr).expect("resolve");
        let err = ensure_xaddress_matches_network(&resolved, &Network::Mainnet)
            .expect_err("test X-address on mainnet");
        assert!(err.to_string().contains("test network"), "{err}");

        let resolved = resolve_payment_destination(&mainnet_xaddr).expect("resolve");
        let err = ensure_xaddress_matches_network(&resolved, &Network::Testnet)
            .expect_err("mainnet X-address on testnet");
        assert!(err.to_string().contains("production network"), "{err}");
    }

    #[test]
    fn xaddress_and_classic_pass_network_check_on_matching_networks() {
        let mainnet_xaddr = classic_address_to_xaddress(CLASSIC, None, false).expect("xaddr");
        let resolved = resolve_payment_destination(&mainnet_xaddr).expect("resolve");
        ensure_xaddress_matches_network(&resolved, &Network::Mainnet).expect("matches mainnet");

        let testnet_xaddr = classic_address_to_xaddress(CLASSIC, None, true).expect("xaddr");
        let resolved = resolve_payment_destination(&testnet_xaddr).expect("resolve");
        ensure_xaddress_matches_network(&resolved, &Network::Testnet).expect("matches testnet");

        let resolved = resolve_payment_destination(CLASSIC).expect("resolve");
        ensure_xaddress_matches_network(&resolved, &Network::Mainnet)
            .expect("classic bypasses X-address check");
    }
}
