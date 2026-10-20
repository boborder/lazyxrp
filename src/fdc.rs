//! Flare Data Connector proof fetch for an XRPL `Payment` (`XRPPayment` attestation), used by
//! `lazyxrp -x fxrp-execute`: verifier `prepareRequest` → `FdcHub.requestAttestation` (pays the
//! fee from the Flare EVM key) → wait for the voting round → DA layer proof.
//!
//! Trust: the verifier and DA layer are untrusted HTTP services. The verifier's encoded request is
//! checked byte for byte against what was asked, and `executeDirectMinting` verifies the Merkle
//! proof on-chain, so a lying service can only waste the attestation fee, never mint.

use std::time::Duration;

use alloy::{
    network::EthereumWallet,
    primitives::{Address, Bytes, U256},
    providers::{Provider, ProviderBuilder},
    sol,
};
use serde_json::{Value, json};

use crate::{config::FlareNetwork, flare};

/// FDC protocol id inside the Flare system protocols (`Relay.isFinalized`).
const FDC_PROTOCOL_ID: u64 = 200;
/// A hostile verifier / DA answer is cut off here (a real answer is well under 10 KiB).
const MAX_RESPONSE_BYTES: usize = 1 << 20;
/// Attestation fee above this (100 FLR) is treated as a hostile / broken RPC answer. The real
/// mainnet `XRPPayment` fee measured 20 FLR (2026-10-20), so the cap leaves 5x headroom.
const MAX_REQUEST_FEE_WEI: u128 = 100_000_000_000_000_000_000;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_EVERY: Duration = Duration::from_secs(10);
/// Rounds finalize in 90 s (+ up to one round of delay); stop waiting after this.
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(600);
/// Public test key accepted by the rate-limited public verifiers.
const PUBLIC_VERIFIER_KEY: &str = "00000000-0000-0000-0000-000000000000";

sol! {
    #[sol(rpc)]
    interface IFdcHub {
        function requestAttestation(bytes calldata _data) external payable;
    }

    #[sol(rpc)]
    interface IFdcRequestFeeConfigurations {
        function getRequestFee(bytes calldata _data) external view returns (uint256);
    }

    #[sol(rpc)]
    interface IRelay {
        function isFinalized(uint256 _protocolId, uint256 _votingRoundId) external view returns (bool);
    }

    #[sol(rpc)]
    interface IFlareSystemsManager {
        function firstVotingRoundStartTs() external view returns (uint64);
        function votingEpochDurationSeconds() external view returns (uint64);
    }
}

/// Verifier / DA endpoints and the XRPL source id for one Flare network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdcConfig {
    pub verifier_base: String,
    pub da_base: String,
    pub api_key: String,
    /// `XRP` (mainnet) / `testXRP` (testnet) as the verifier's `sourceId`.
    pub source_id: &'static str,
}

/// Defaults per network; `FDC_VERIFIER_URL` / `FDC_DA_URL` / `FDC_VERIFIER_API_KEY` override.
/// Both URLs must be `https://` (the DA answer decides what gets submitted on-chain).
pub fn fdc_config(
    network: FlareNetwork,
    env: impl Fn(&str) -> Option<String>,
) -> color_eyre::Result<FdcConfig> {
    let (verifier, da, source_id) = match network {
        FlareNetwork::Flare => (
            "https://fdc-verifiers-mainnet.flare.network",
            "https://flr-data-availability.flare.network",
            "XRP",
        ),
        FlareNetwork::Coston2 => (
            "https://fdc-verifiers-testnet.flare.network",
            "https://ctn2-data-availability.flare.network",
            "testXRP",
        ),
        FlareNetwork::Songbird => {
            color_eyre::eyre::bail!("FXRP / FDC XRPPayment proofs are not available on Songbird")
        }
    };
    let pick = |var: &str, default: &str| {
        env(var)
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default.to_string())
    };
    let cfg = FdcConfig {
        verifier_base: pick("FDC_VERIFIER_URL", verifier),
        da_base: pick("FDC_DA_URL", da),
        api_key: env("FDC_VERIFIER_API_KEY")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| PUBLIC_VERIFIER_KEY.to_string()),
        source_id,
    };
    for (what, url) in [("verifier", &cfg.verifier_base), ("DA layer", &cfg.da_base)] {
        if !url.starts_with("https://") {
            color_eyre::eyre::bail!("FDC {what} URL must be https://, got {url}");
        }
    }
    Ok(cfg)
}

/// ASCII name right-padded with zeros to 32 bytes, `0x`-prefixed (FDC `bytes32` type / source ids).
fn bytes32_name(name: &str) -> String {
    format!("0x{:0<64}", hex::encode(name))
}

/// The 160-byte `abiEncodedRequest` the verifier must return for our request:
/// `[attestationType | sourceId | MIC | transactionId | proofOwner (left-padded)]`.
/// Everything except the MIC (a hash we cannot recompute) is checked.
pub fn check_encoded_request(
    encoded: &[u8],
    source_id: &str,
    tx_id: &[u8; 32],
    owner: Address,
) -> color_eyre::Result<()> {
    let name = |s: &str| {
        let mut out = [0_u8; 32];
        out[..s.len()].copy_from_slice(s.as_bytes());
        out
    };
    let mut padded_owner = [0_u8; 32];
    padded_owner[12..].copy_from_slice(owner.as_slice());
    if encoded.len() != 160
        || encoded[..32] != name("XRPPayment")
        || encoded[32..64] != name(source_id)
        || encoded[96..128] != *tx_id
        || encoded[128..160] != padded_owner
    {
        color_eyre::eyre::bail!(
            "verifier returned a request that does not match the XRPL transaction / proof owner"
        );
    }
    Ok(())
}

async fn read_limited(resp: reqwest::Response, what: &str) -> color_eyre::Result<Vec<u8>> {
    use futures::StreamExt;
    let status = resp.status();
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk?);
        if body.len() > MAX_RESPONSE_BYTES {
            color_eyre::eyre::bail!("{what} answer exceeds {MAX_RESPONSE_BYTES} bytes");
        }
    }
    if !status.is_success() {
        color_eyre::eyre::bail!(
            "{what} HTTP {status}: {}",
            String::from_utf8_lossy(&body[..body.len().min(200)])
        );
    }
    Ok(body)
}

fn http_client() -> color_eyre::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// Verifier `prepareRequest` for the XRPL payment; returns the validated `abiEncodedRequest`.
async fn prepare_request(
    cfg: &FdcConfig,
    tx_hash: &str,
    owner: Address,
) -> color_eyre::Result<Vec<u8>> {
    let tx_id: [u8; 32] = hex::decode(tx_hash.trim_start_matches("0x"))?
        .try_into()
        .map_err(|_| color_eyre::eyre::eyre!("XRPL transaction hash must be 32 bytes"))?;
    let body = json!({
        "attestationType": bytes32_name("XRPPayment"),
        "sourceId": bytes32_name(cfg.source_id),
        "requestBody": {
            "transactionId": format!("0x{}", hex::encode(tx_id)),
            // The verifier lower-cases it; the contract compares the address, not the text.
            "proofOwner": format!("{owner:#x}"),
        },
    });
    let resp = http_client()?
        .post(format!(
            "{}/verifier/xrp/XRPPayment/prepareRequest",
            cfg.verifier_base
        ))
        .header("X-API-KEY", &cfg.api_key)
        .json(&body)
        .send()
        .await?;
    let answer: Value = serde_json::from_slice(&read_limited(resp, "FDC verifier").await?)?;
    if answer["status"] != "VALID" {
        color_eyre::eyre::bail!(
            "verifier rejected the request: {} (the payment needs 3 validated ledgers, ≈ 12 s)",
            answer["status"]
        );
    }
    let encoded = hex::decode(
        answer["abiEncodedRequest"]
            .as_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("verifier answer has no abiEncodedRequest"))?
            .trim_start_matches("0x"),
    )?;
    check_encoded_request(&encoded, cfg.source_id, &tx_id, owner)?;
    Ok(encoded)
}

/// DA layer proof for `(round, request)`: `{response, proof}` JSON text (decoded leaves).
async fn fetch_da_proof(cfg: &FdcConfig, round: u64, request: &[u8]) -> color_eyre::Result<String> {
    let resp = http_client()?
        .post(format!("{}/api/v1/fdc/proof-by-request-round", cfg.da_base))
        .json(&json!({
            "votingRoundId": round,
            "requestBytes": format!("0x{}", hex::encode(request)),
        }))
        .send()
        .await?;
    let text = String::from_utf8(read_limited(resp, "FDC DA layer").await?)?;
    // Shape check now (same parser as the executor), so a bad answer fails before any gas.
    flare::parse_fdc_payment_proof_json(&text)?;
    Ok(text)
}

/// Voting round of a block timestamp: `(ts - firstVotingRoundStartTs) / votingEpochDurationSeconds`.
pub fn voting_round(block_ts: u64, first_start: u64, epoch_secs: u64) -> color_eyre::Result<u64> {
    if epoch_secs == 0 || block_ts < first_start {
        color_eyre::eyre::bail!("implausible FDC round parameters ({first_start}, {epoch_secs})");
    }
    Ok((block_ts - first_start) / epoch_secs)
}

/// Prove the XRPL payment `xrpl_tx_hash` for `executeDirectMinting` / `...WithData`.
/// Spends the attestation fee (a Flare transaction signed by `private_key_hex`) and returns the
/// DA layer proof JSON. The proof is bound to the signer's address (`proofOwner`), so only that
/// key can use it.
pub async fn prove_xrp_payment(
    rpc_url: &str,
    private_key_hex: &str,
    cfg: &FdcConfig,
    xrpl_tx_hash: &str,
    resume_round: Option<u64>,
    progress: impl Fn(&str),
) -> color_eyre::Result<String> {
    let signer = flare::parse_evm_key(private_key_hex)?;
    let owner = signer.address();
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);

    progress("asking the verifier to prepare the request…");
    let request = prepare_request(cfg, xrpl_tx_hash, owner).await?;

    let (hub, fees, relay, systems) = tokio::try_join!(
        flare::resolve_registry_contract(provider.clone(), rpc_url, "FdcHub"),
        flare::resolve_registry_contract(provider.clone(), rpc_url, "FdcRequestFeeConfigurations"),
        flare::resolve_registry_contract(provider.clone(), rpc_url, "Relay"),
        flare::resolve_registry_contract(provider.clone(), rpc_url, "FlareSystemsManager"),
    )?;
    let round = if let Some(r) = resume_round {
        progress(&format!(
            "resuming voting round {r}: not paying for a new attestation"
        ));
        r
    } else {
        let request_bytes = Bytes::from(request.clone());
        let fee = IFdcRequestFeeConfigurations::new(fees, provider.clone())
            .getRequestFee(request_bytes.clone())
            .call()
            .await?;
        if fee > U256::from(MAX_REQUEST_FEE_WEI) {
            color_eyre::eyre::bail!(
                "attestation fee {fee} wei is implausibly high; refusing to pay"
            );
        }
        progress(&format!("requesting the attestation (fee {fee} wei)…"));
        let pending = IFdcHub::new(hub, provider.clone())
            .requestAttestation(request_bytes)
            .value(fee)
            .send()
            .await
            .map_err(|e| color_eyre::eyre::eyre!("requestAttestation failed: {e}"))?;
        let receipt = pending.get_receipt().await?;
        if !receipt.status() {
            color_eyre::eyre::bail!(
                "requestAttestation reverted: {:#x}",
                receipt.transaction_hash
            );
        }
        let block = provider
            .get_block_by_number(
                receipt
                    .block_number
                    .ok_or_else(|| color_eyre::eyre::eyre!("receipt has no block number"))?
                    .into(),
            )
            .await?
            .ok_or_else(|| color_eyre::eyre::eyre!("attestation block not found"))?;
        let sm = IFlareSystemsManager::new(systems, provider.clone());
        let (first, epoch) = tokio::try_join!(
            async { Ok::<_, color_eyre::Report>(sm.firstVotingRoundStartTs().call().await?) },
            async { Ok(sm.votingEpochDurationSeconds().call().await?) },
        )?;
        let round = voting_round(block.header.timestamp, first, epoch)?;
        // Printed before the wait so a dropped run can resume with `--round`.
        progress(&format!(
            "attestation paid; if this run dies, resume with --round {round}"
        ));
        round
    };

    progress(&format!("waiting for voting round {round} to finalize…"));
    let relay = IRelay::new(relay, provider.clone());
    let deadline = tokio::time::Instant::now() + FINALIZE_TIMEOUT;
    // A dropped connection mid-wait must not strand the paid attestation: tolerate a few
    // consecutive RPC errors before giving up.
    let mut rpc_errors = 0_u32;
    loop {
        match relay
            .isFinalized(U256::from(FDC_PROTOCOL_ID), U256::from(round))
            .call()
            .await
        {
            Ok(true) => break,
            Ok(false) => rpc_errors = 0,
            Err(e) => {
                rpc_errors += 1;
                if rpc_errors > 5 {
                    return Err(e.into());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            color_eyre::eyre::bail!(
                "round {round} did not finalize in {} s (the attestation fee is spent)",
                FINALIZE_TIMEOUT.as_secs()
            );
        }
        tokio::time::sleep(POLL_EVERY).await;
    }

    progress("fetching the proof from the DA layer…");
    // The DA layer can lag a few seconds behind the on-chain Merkle root.
    let mut last = None;
    for _ in 0..12 {
        match fetch_da_proof(cfg, round, &request).await {
            Ok(proof) => return Ok(proof),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
    Err(last.unwrap_or_else(|| color_eyre::eyre::eyre!("DA layer returned no proof")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_none(_: &str) -> Option<String> {
        None
    }

    /// TC-340: the verifier's encoded request must match the asked transaction and proof owner;
    /// any mismatch (other tx, other owner, wrong type / source, wrong length) is refused.
    #[test]
    fn verifier_request_is_checked_against_what_was_asked() {
        let tx = [0xab_u8; 32];
        let owner = Address::repeat_byte(0x11);
        let build = |ty: &str, src: &str, tx: &[u8; 32], owner: Address| {
            let mut v = Vec::new();
            for s in [ty, src] {
                let mut w = [0_u8; 32];
                w[..s.len()].copy_from_slice(s.as_bytes());
                v.extend_from_slice(&w);
            }
            v.extend_from_slice(&[0x77; 32]);
            v.extend_from_slice(tx);
            v.extend_from_slice(&[0_u8; 12]);
            v.extend_from_slice(owner.as_slice());
            v
        };
        let good = build("XRPPayment", "XRP", &tx, owner);
        assert!(check_encoded_request(&good, "XRP", &tx, owner).is_ok());
        assert!(check_encoded_request(&good, "testXRP", &tx, owner).is_err());
        assert!(check_encoded_request(&good, "XRP", &[0xcd; 32], owner).is_err());
        assert!(check_encoded_request(&good, "XRP", &tx, Address::repeat_byte(0x22)).is_err());
        assert!(
            check_encoded_request(&build("Payment", "XRP", &tx, owner), "XRP", &tx, owner).is_err()
        );
        assert!(check_encoded_request(&good[..159], "XRP", &tx, owner).is_err());
        // The padded request name is the documented hex.
        assert_eq!(
            bytes32_name("XRP"),
            "0x5852500000000000000000000000000000000000000000000000000000000000"
        );
    }
    /// The FDC read path on mainnet (no key, no fee): registry names, round parameters, a
    /// finalized past round, the verifier answer for a real Core Vault payment
    /// (`FDC_LIVE_TX`, 64 hex) passing `check_encoded_request`, and `getRequestFee` on it.
    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn fdc_read_path_live() -> color_eyre::Result<()> {
        let tx = std::env::var("FDC_LIVE_TX")?;
        let rpc = flare::DEFAULT_FLARE_RPC;
        let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
        let (fees, relay, systems) = tokio::try_join!(
            flare::resolve_registry_contract(provider.clone(), rpc, "FdcRequestFeeConfigurations"),
            flare::resolve_registry_contract(provider.clone(), rpc, "Relay"),
            flare::resolve_registry_contract(provider.clone(), rpc, "FlareSystemsManager"),
        )?;
        flare::resolve_registry_contract(provider.clone(), rpc, "FdcHub").await?;
        let sm = IFlareSystemsManager::new(systems, provider.clone());
        let (first, epoch) = (
            sm.firstVotingRoundStartTs().call().await?,
            sm.votingEpochDurationSeconds().call().await?,
        );
        assert_eq!(epoch, 90);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let round = voting_round(now, first, epoch)?;
        let finalized = IRelay::new(relay, provider.clone())
            .isFinalized(U256::from(FDC_PROTOCOL_ID), U256::from(round - 20))
            .call()
            .await?;
        assert!(finalized, "round {} should be finalized", round - 20);

        let cfg = fdc_config(FlareNetwork::Flare, |v| std::env::var(v).ok())?;
        let request = prepare_request(&cfg, &tx, Address::repeat_byte(0x11)).await?;
        let fee = IFdcRequestFeeConfigurations::new(fees, provider)
            .getRequestFee(Bytes::from(request))
            .call()
            .await?;
        eprintln!("round {round}, first {first}, request fee {fee} wei");
        assert!(fee > U256::ZERO && fee < U256::from(MAX_REQUEST_FEE_WEI));
        Ok(())
    }

    /// TC-340: round arithmetic, network defaults, https-only overrides, no Songbird.
    #[test]
    fn round_math_and_endpoint_config() {
        assert_eq!(voting_round(1_658_430_090, 1_658_430_000, 90).unwrap(), 1);
        assert_eq!(voting_round(1_658_430_089, 1_658_430_000, 90).unwrap(), 0);
        assert!(voting_round(1, 2, 90).is_err());
        assert!(voting_round(10, 0, 0).is_err());

        let main = fdc_config(FlareNetwork::Flare, env_none).unwrap();
        assert_eq!(main.source_id, "XRP");
        assert!(main.verifier_base.starts_with("https://"));
        assert_eq!(
            fdc_config(FlareNetwork::Coston2, env_none)
                .unwrap()
                .source_id,
            "testXRP"
        );
        assert!(fdc_config(FlareNetwork::Songbird, env_none).is_err());
        let http = |v: &str| (v == "FDC_DA_URL").then(|| "http://evil.example/".to_string());
        assert!(fdc_config(FlareNetwork::Flare, http).is_err());
        let custom = |v: &str| (v == "FDC_DA_URL").then(|| " https://da.example/ ".to_string());
        assert_eq!(
            fdc_config(FlareNetwork::Flare, custom).unwrap().da_base,
            "https://da.example"
        );
    }
}
