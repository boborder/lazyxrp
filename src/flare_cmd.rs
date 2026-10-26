//! Flare EVM write commands behind `lazyxrp -x fxrp-*` (redeem, minting tags, FDC-proven execute)
//! and the Smart Account route lookup for `-x sa-vault`.
//!
//! Every write is gated by the caller (`[flare.fassets] execute = true`, the Flare EVM key, an
//! https RPC, `--yes` on Flare mainnet), estimates gas first (`eth_estimateGas` runs the call, so
//! a revert fails here before anything is sent) and prints what it is about to do.

use alloy::{
    network::EthereumWallet,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
};

use crate::flare::{
    self, EvmGate, IAssetManager, IERC20, IMasterAccountController, IMintingTagManager,
};

alloy::sol! {
    struct Call {
        address target;
        uint256 value;
        bytes data;
    }
    function executeUserOp(Call[] calls) external payable;
    /// ERC-4337 `PackedUserOperation`; only `sender`, `nonce` and `callData` are validated.
    struct PackedUserOperation {
        address sender;
        uint256 nonce;
        bytes initCode;
        bytes callData;
        bytes32 accountGasLimits;
        uint256 preVerificationGas;
        bytes32 gasFees;
        bytes paymasterAndData;
        bytes signature;
    }
}

/// Build a `0xFE` custom instruction: the ABI-encoded `PackedUserOperation` (delivered to the
/// executor as `--data`) and the 42-byte memo `[0xFE | 0 | fee | keccak256(userOp)]` as hex.
/// Each call is `TARGET:0xDATA`, sent with value 0 (value-carrying calls are not supported).
pub fn build_user_operation(
    personal: Address,
    nonce: u128,
    calls: &[String],
    fee_uba: u64,
) -> color_eyre::Result<(Vec<u8>, String)> {
    use alloy::sol_types::{SolCall, SolValue};
    use color_eyre::eyre::{bail, eyre};
    if calls.is_empty() {
        bail!("at least one --call TARGET:0xDATA is required");
    }
    let calls = calls
        .iter()
        .map(|c| {
            let (t, d) = c
                .split_once(':')
                .ok_or_else(|| eyre!("call must be TARGET:0xDATA, got {c}"))?;
            let target: Address = t
                .trim()
                .parse()
                .map_err(|e| eyre!("call target {t}: {e}"))?;
            let data = hex::decode(d.trim().trim_start_matches("0x"))
                .map_err(|e| eyre!("call data must be hex: {e}"))?;
            Ok(Call {
                target,
                value: U256::ZERO,
                data: data.into(),
            })
        })
        .collect::<color_eyre::Result<Vec<_>>>()?;
    let op = PackedUserOperation {
        sender: personal,
        nonce: U256::from(nonce),
        initCode: Default::default(),
        callData: executeUserOpCall { calls }.abi_encode().into(),
        accountGasLimits: Default::default(),
        preVerificationGas: U256::ZERO,
        gasFees: Default::default(),
        paymasterAndData: Default::default(),
        signature: Default::default(),
    };
    let bytes = op.abi_encode();
    let memo = crate::signing::encode_memo_instruction(
        0xFE,
        fee_uba,
        alloy::primitives::keccak256(&bytes).as_slice(),
    )?;
    Ok((bytes, memo))
}

/// Validate a redeem request against what the chain reports. `tag` must fit XRPL's `uint32`.
pub fn check_redeem(
    amount_uba: u128,
    min_uba: u128,
    balance_uba: u128,
    tag: Option<u64>,
) -> Result<Option<u32>, String> {
    if amount_uba == 0 {
        return Err("amount must be greater than zero".into());
    }
    if amount_uba < min_uba {
        return Err(format!(
            "amount {} XRP is below minimumRedeemAmountUBA ({} XRP)",
            flare::uba_to_xrp_display(amount_uba),
            flare::uba_to_xrp_display(min_uba)
        ));
    }
    if amount_uba > balance_uba {
        return Err(format!(
            "amount {} XRP exceeds the FXRP balance ({} XRP)",
            flare::uba_to_xrp_display(amount_uba),
            flare::uba_to_xrp_display(balance_uba)
        ));
    }
    tag.map(|t| u32::try_from(t).map_err(|_| "destination tag must fit in 32 bits".to_string()))
        .transpose()
}

/// A tag operation target must be a real, distinct address (the zero address burns / disables).
pub fn check_tag_address(what: &str, addr: Address, signer: Address) -> Result<(), String> {
    if addr == Address::ZERO {
        return Err(format!("{what} must not be the zero address"));
    }
    if addr == signer {
        return Err(format!("{what} is already your own address"));
    }
    Ok(())
}

/// `--target` → 32-byte XRPL transaction id.
pub fn parse_tx_id(t: &str) -> color_eyre::Result<[u8; 32]> {
    let bytes = hex::decode(t.trim().trim_start_matches("0x"))
        .map_err(|e| color_eyre::eyre::eyre!("--target must be hex: {e}"))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        color_eyre::eyre::eyre!("--target must be 64 hex characters (the stuck XRPL tx hash)")
    })
}

/// Poll interval / limit while waiting for a public executor to finalize a memo payment. A
/// mainnet sample of 8 Smart Account mints took 95–177 s after the XRPL ledger validated them.
pub const FINALIZE_POLL: std::time::Duration = std::time::Duration::from_secs(10);
pub const FINALIZE_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// Poll `check` every `interval` until it reports true or `limit` has passed. `Ok(false)` means
/// "not finalized yet"; a failed read ends the wait with that error, never as `false`.
pub async fn wait_finalized<F, Fut>(
    mut check: F,
    interval: std::time::Duration,
    limit: std::time::Duration,
) -> color_eyre::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = color_eyre::Result<bool>>,
{
    let deadline = std::time::Instant::now() + limit;
    loop {
        if check().await? {
            return Ok(true);
        }
        if std::time::Instant::now() + interval > deadline {
            return Ok(false);
        }
        tokio::time::sleep(interval).await;
    }
}

/// After a `sa-memo` payment validated: wait for a public executor to finalize it (all memo ops
/// take `--data 0x`, so nobody needs private bytes), else say how to finish or re-check. Public
/// executors set their own fee floor (not measured here), so a low `--fee` may never be picked up.
/// The payment already succeeded, so nothing here may fail the command (a re-run would send a
/// second Core Vault payment): a failed read is reported with the re-check hint instead.
pub async fn await_public_executor(rpc_url: &str, tx_hash: &str, wait: bool, say: impl Fn(&str)) {
    let recheck = format!("re-check: lazyxrp -x sa-status {tx_hash}");
    let Ok(tx) = parse_tx_id(tx_hash) else {
        say(&format!(
            "cannot check finalization (bad tx hash); {recheck}"
        ));
        return;
    };
    if !wait {
        say(&format!("not waiting; {recheck}"));
        return;
    }
    say(&format!(
        "waiting up to {} s for a public executor to finalize {tx_hash}…",
        FINALIZE_LIMIT.as_secs()
    ));
    let outcome = wait_finalized(
        || flare::smart_account_tx_used(rpc_url, tx),
        FINALIZE_POLL,
        FINALIZE_LIMIT,
    )
    .await;
    say(&match outcome {
        Ok(true) => "finalized on Flare".to_string(),
        Ok(false) => format!(
            "not finalized after {} s ({recheck}); to finalize it yourself: lazyxrp -x \
             fxrp-execute {tx_hash} --data 0x (needs [flare.fassets] execute, FLARE_EVM_KEY and \
             FLR for the attestation fee + gas)",
            FINALIZE_LIMIT.as_secs()
        ),
        Err(e) => {
            format!("the payment is validated, but the finalization check failed: {e}; {recheck}")
        }
    });
}

/// Per-op state of the target payment (SKILL.md recovery flow): `0xE0` skip-memo and `0xE2`
/// replace-fee only help while the target is NOT yet minted; `0xE1` fast-forward only after it IS.
pub fn check_target_state(op: crate::cli::MemoOp, used: Option<bool>) -> color_eyre::Result<()> {
    use crate::cli::MemoOp;
    let need = match op {
        MemoOp::IgnoreMemo | MemoOp::ReplaceFee => false,
        MemoOp::SetNonce => true,
        MemoOp::SetExecutor | MemoOp::RemoveExecutor => return Ok(()),
    };
    match used {
        Some(u) if u == need => Ok(()),
        Some(true) => color_eyre::eyre::bail!(
            "the target payment is already minted; this instruction only applies before it executes"
        ),
        Some(false) => color_eyre::eyre::bail!(
            "the target payment is not minted yet; run the skip-memo recovery first, then fast-forward the nonce"
        ),
        None => color_eyre::eyre::bail!("--target is required for this instruction"),
    }
}

/// Build the `MemoData` hex for a direct-mint memo instruction. Preconditions mirror
/// `MemoInstructions.sol`: `0xE1` needs `newNonce > current` and a jump of at most `u32::MAX`
/// (`InvalidNonceIncrease` otherwise); `0xD0` refuses the zero address.
pub fn recovery_memo(
    op: crate::cli::MemoOp,
    fee_uba: u64,
    target: Option<&str>,
    nonce: Option<u64>,
    new_fee: Option<u64>,
    executor: Option<&str>,
    current_nonce: Option<u128>,
) -> color_eyre::Result<String> {
    use crate::cli::MemoOp;
    use color_eyre::eyre::{bail, eyre};
    let tx_id = || -> color_eyre::Result<[u8; 32]> {
        parse_tx_id(target.ok_or_else(|| eyre!("--target <XRPL tx hash> is required"))?)
    };
    match op {
        MemoOp::IgnoreMemo => crate::signing::encode_memo_instruction(0xE0, fee_uba, &tx_id()?),
        MemoOp::SetNonce => {
            let new = u128::from(nonce.ok_or_else(|| eyre!("--nonce is required"))?);
            let current =
                current_nonce.ok_or_else(|| eyre!("current personal-account nonce not read"))?;
            if new <= current || new - current > u128::from(u32::MAX) {
                bail!(
                    "newNonce {new} must be greater than the current nonce {current} by at most {}",
                    u32::MAX
                );
            }
            let mut body = [0_u8; 32];
            body[16..].copy_from_slice(&new.to_be_bytes());
            crate::signing::encode_memo_instruction(0xE1, fee_uba, &body)
        }
        MemoOp::ReplaceFee => {
            let new_fee = new_fee.ok_or_else(|| eyre!("--new-fee is required"))?;
            let mut body = tx_id()?.to_vec();
            body.extend_from_slice(&new_fee.to_be_bytes());
            crate::signing::encode_memo_instruction(0xE2, fee_uba, &body)
        }
        MemoOp::SetExecutor => {
            let e = executor.ok_or_else(|| eyre!("--executor is required"))?;
            let addr = Address::from(crate::signing::parse_smart_account_recipient(e)?);
            if addr == Address::ZERO {
                bail!("executor must not be the zero address");
            }
            crate::signing::encode_memo_instruction(0xD0, fee_uba, addr.as_slice())
        }
        MemoOp::RemoveExecutor => crate::signing::encode_memo_instruction(0xD1, fee_uba, &[]),
    }
}

fn to_u128(v: U256, what: &str) -> color_eyre::Result<u128> {
    u128::try_from(v).map_err(|_| color_eyre::eyre::eyre!("Flare RPC returned out-of-range {what}"))
}

/// Receipt → hash, or an error naming the reverted transaction.
async fn confirm(
    pending: alloy::providers::PendingTransactionBuilder<alloy::network::Ethereum>,
    what: &str,
) -> color_eyre::Result<alloy::rpc::types::TransactionReceipt> {
    let hash = *pending.tx_hash();
    let receipt = pending
        .get_receipt()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("{what} receipt failed ({hash:#x}): {e}"))?;
    if !receipt.status() {
        color_eyre::eyre::bail!("{what} reverted: {hash:#x}");
    }
    Ok(receipt)
}

/// Wallet provider + signer address, after the chain-id / `--yes` gate: nothing is signed before it.
async fn wallet_provider(
    rpc_url: &str,
    key: &str,
    gate: EvmGate,
) -> color_eyre::Result<(impl Provider + Clone, Address)> {
    let signer = flare::parse_evm_key(key)?;
    let address = signer.address();
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);
    gate.check(&provider).await?;
    Ok((provider, address))
}

/// `redeemAmount` (or `redeemWithTag` with a destination tag): burn `amount_uba` of the signer's
/// FXRP and have an agent pay XRP to `xrpl_address`. Returns the Flare transaction hash.
pub async fn redeem_fxrp(
    rpc_url: &str,
    key: &str,
    gate: EvmGate,
    amount_uba: u128,
    xrpl_address: &str,
    tag: Option<u64>,
    say: impl Fn(&str),
) -> color_eyre::Result<String> {
    let xrpl_address = crate::signing::require_classic_address_shape("XRPL address", xrpl_address)?;
    let (provider, me) = wallet_provider(rpc_url, key, gate).await?;
    let am_addr = flare::resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
    let am = IAssetManager::new(am_addr, provider.clone());
    let fxrp = IERC20::new(am.fAsset().call().await?, provider.clone());
    let (min, balance) = tokio::try_join!(
        async { Ok::<_, color_eyre::Report>(am.minimumRedeemAmountUBA().call().await?) },
        async { Ok(fxrp.balanceOf(me).call().await?) },
    )?;
    let tag32 = check_redeem(
        amount_uba,
        to_u128(min, "minimum redeem amount")?,
        to_u128(balance, "FXRP balance")?,
        tag,
    )
    .map_err(|e| color_eyre::eyre::eyre!(e))?;
    say(&format!(
        "redeem {} FXRP from {me:#x} → {xrpl_address}{}",
        flare::uba_to_xrp_display(amount_uba),
        tag32.map_or(String::new(), |t| format!(" (destination tag {t})")),
    ));

    let amount = U256::from(amount_uba);
    let pending = match tag32 {
        None => {
            let call = am.redeemAmount(amount, xrpl_address.to_string(), Address::ZERO);
            flare::send_capped(call, &say).await?
        }
        Some(t) => {
            let call = am.redeemWithTag(
                amount,
                xrpl_address.to_string(),
                Address::ZERO,
                U256::from(t),
            );
            flare::send_capped(call, &say).await?
        }
    };
    let receipt = confirm(pending, "redeem").await?;
    {
        use alloy::sol_types::SolEvent;
        for l in receipt
            .inner
            .logs()
            .iter()
            .filter(|l| l.address() == am_addr)
        {
            if let Ok(ev) = IAssetManager::RedemptionAmountIncomplete::decode_log(&l.inner) {
                say(&format!(
                    "WARNING: only part was allocated; {} XRP stays with you as FXRP",
                    flare::uba_to_xrp_display(to_u128(
                        ev.data.remainingAmountUBA,
                        "remaining amount"
                    )?)
                ));
            } else if let Ok(ev) = IAssetManager::RedemptionRequestIncomplete::decode_log(&l.inner)
            {
                say(&format!(
                    "WARNING: only part was allocated; {} lots stay with you as FXRP",
                    ev.data.remainingLots
                ));
            }
        }
    }
    Ok(format!("{:#x}", receipt.transaction_hash))
}

async fn tag_manager<P: Provider + Clone>(
    provider: &P,
    rpc_url: &str,
) -> color_eyre::Result<IMintingTagManager::IMintingTagManagerInstance<P>> {
    let am_addr = flare::resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
    let addr = IAssetManager::new(am_addr, provider.clone())
        .getMintingTagManager()
        .call()
        .await?;
    Ok(IMintingTagManager::new(addr, provider.clone()))
}

/// The tag must be reserved and owned by `me` before anything is sent. Only a revert reads as
/// "not reserved"; transport errors propagate.
async fn require_tag_owner<P: Provider>(
    manager: &IMintingTagManager::IMintingTagManagerInstance<P>,
    tag: u64,
    me: Address,
) -> color_eyre::Result<()> {
    let owner = flare::tag_owner(manager, U256::from(tag))
        .await?
        .ok_or_else(|| color_eyre::eyre::eyre!("tag {tag} is not reserved"))?;
    if owner != me {
        color_eyre::eyre::bail!("tag {tag} is owned by {owner:#x}, not by {me:#x}");
    }
    Ok(())
}

/// `reserve()`: pay `reservationFee` (FLR) for a new minting tag. Returns `(tx hash, tag id)`.
pub async fn reserve_tag(
    rpc_url: &str,
    key: &str,
    gate: EvmGate,
    say: impl Fn(&str),
) -> color_eyre::Result<(String, u128)> {
    let (provider, me) = wallet_provider(rpc_url, key, gate).await?;
    let manager = tag_manager(&provider, rpc_url).await?;
    let fee = manager.reservationFee().call().await?;
    let balance = provider.get_balance(me).await?;
    if balance < fee {
        color_eyre::eyre::bail!("FLR balance {balance} wei is below the reservation fee {fee} wei");
    }
    say(&format!(
        "reserve a minting tag for {fee} wei FLR from {me:#x}"
    ));
    let call = manager.reserve().value(fee);
    let receipt = confirm(flare::send_capped(call, say).await?, "reserve").await?;
    let tag = receipt
        .inner
        .logs()
        .iter()
        .filter(|l| l.address() == *manager.address())
        .find_map(|l| {
            l.log_decode::<IMintingTagManager::MintingTagReserved>()
                .ok()
                .filter(|e| e.inner.data.owner == me)
        })
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "tag reserved ({:#x}) but no MintingTagReserved event for you was found",
                receipt.transaction_hash
            )
        })?;

    Ok((
        format!("{:#x}", receipt.transaction_hash),
        to_u128(tag.inner.data.tag, "tag id")?,
    ))
}

/// `setMintingRecipient` and / or `setAllowedExecutor` on a tag you own. An executor change only
/// takes effect after `executorChangeAfterSeconds` (printed). Returns the tx hashes.
pub async fn set_tag(
    rpc_url: &str,
    key: &str,
    gate: EvmGate,
    tag: u64,
    recipient: Option<Address>,
    executor: Option<Address>,
    say: impl Fn(&str),
) -> color_eyre::Result<Vec<String>> {
    if recipient.is_none() && executor.is_none() {
        color_eyre::eyre::bail!("give --recipient and / or --executor");
    }
    for (what, a) in [("recipient", recipient), ("executor", executor)] {
        if a == Some(Address::ZERO) {
            color_eyre::eyre::bail!("{what} must not be the zero address");
        }
    }
    let (provider, me) = wallet_provider(rpc_url, key, gate).await?;
    let manager = tag_manager(&provider, rpc_url).await?;
    let id = U256::from(tag);
    require_tag_owner(&manager, tag, me).await?;
    let mut hashes = Vec::new();
    if let Some(r) = recipient {
        say(&format!("tag {tag}: minting recipient → {r:#x}"));
        let call = manager.setMintingRecipient(id, r);
        let receipt = confirm(flare::send_capped(call, &say).await?, "setMintingRecipient").await?;
        hashes.push(format!("{:#x}", receipt.transaction_hash));
    }
    if let Some(e) = executor {
        let cooldown = manager.executorChangeAfterSeconds().call().await?;
        say(&format!(
            "tag {tag}: allowed executor → {e:#x} (active after {cooldown} s)"
        ));
        let call = manager.setAllowedExecutor(id, e);
        let receipt = confirm(flare::send_capped(call, &say).await?, "setAllowedExecutor").await?;
        hashes.push(format!("{:#x}", receipt.transaction_hash));
    }
    Ok(hashes)
}

/// The vault id must be registered with the matching type (`1` Firelight, `2` Upshift) before an
/// instruction naming it is paid for; a wrong id would burn the fee or touch another vault.
pub async fn ensure_vault_registered(
    rpc_url: &str,
    vault_id: u16,
    kind: u8,
) -> color_eyre::Result<Address> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let controller_addr = flare::resolve_registry_contract(
        provider.clone(),
        rpc_url,
        flare::MASTER_ACCOUNT_CONTROLLER_NAME,
    )
    .await?;
    let list = IMasterAccountController::new(controller_addr, provider)
        .getVaults()
        .call()
        .await?;
    let found = list
        .vaultIds
        .iter()
        .zip(&list.vaultAddresses)
        .zip(&list.vaultTypes)
        .find(|((id, _), _)| **id == U256::from(vault_id));
    match found {
        None => color_eyre::eyre::bail!("vault {vault_id} is not registered"),
        Some(((_, _), t)) if *t != kind => color_eyre::eyre::bail!(
            "vault {vault_id} is type {t} (1 = Firelight, 2 = Upshift), not {kind}"
        ),
        Some(((_, addr), _)) => Ok(*addr),
    }
}

/// `transfer(to, tag)`: hand a tag you own to another address. The recipient resets to the new
/// owner and the allowed executor is cleared (contract behavior). Irreversible.
pub async fn transfer_tag(
    rpc_url: &str,
    key: &str,
    gate: EvmGate,
    tag: u64,
    to: Address,
    say: impl Fn(&str),
) -> color_eyre::Result<String> {
    let (provider, me) = wallet_provider(rpc_url, key, gate).await?;
    check_tag_address("destination", to, me).map_err(|e| color_eyre::eyre::eyre!(e))?;
    let manager = tag_manager(&provider, rpc_url).await?;
    let id = U256::from(tag);
    require_tag_owner(&manager, tag, me).await?;
    say(&format!("transfer tag {tag}: {me:#x} → {to:#x}"));
    let call = manager.transfer(to, id);
    let receipt = confirm(flare::send_capped(call, say).await?, "transfer").await?;
    Ok(format!("{:#x}", receipt.transaction_hash))
}

/// Operator XRPL address (`getXrplProviderWallets()[0]`) and `getInstructionFee(id)` in drops for
/// one Smart Account instruction id (read-only).
pub async fn instruction_route(rpc_url: &str, id: u8) -> color_eyre::Result<(String, u64)> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let controller_addr = flare::resolve_registry_contract(
        provider.clone(),
        rpc_url,
        flare::MASTER_ACCOUNT_CONTROLLER_NAME,
    )
    .await?;
    let controller = IMasterAccountController::new(controller_addr, provider);
    let (wallets, fee) = tokio::try_join!(
        async { Ok::<_, color_eyre::Report>(controller.getXrplProviderWallets().call().await?) },
        async { Ok(controller.getInstructionFee(U256::from(id)).call().await?) },
    )?;
    let operator = wallets
        .into_iter()
        .next()
        .ok_or_else(|| color_eyre::eyre::eyre!("MasterAccountController lists no operator"))?;
    let fee = u64::try_from(fee)
        .map_err(|_| color_eyre::eyre::eyre!("Flare RPC returned out-of-range instruction fee"))?;
    Ok((operator, fee))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolCall as _;

    /// TC-341: redeem validation (zero, minimum, balance, tag width) and tag-address guards.
    #[test]
    fn redeem_and_tag_guards() {
        let min = 5_000_000;
        assert!(check_redeem(0, min, 10_000_000, None).is_err());
        assert!(
            check_redeem(4_999_999, min, 10_000_000, None)
                .unwrap_err()
                .contains("minimumRedeemAmountUBA")
        );
        assert_eq!(check_redeem(5_000_000, min, 10_000_000, None), Ok(None));
        assert!(
            check_redeem(10_000_001, min, 10_000_000, None)
                .unwrap_err()
                .contains("exceeds")
        );
        assert_eq!(
            check_redeem(5_000_000, min, 10_000_000, Some(4_294_967_295)),
            Ok(Some(u32::MAX))
        );
        assert!(check_redeem(5_000_000, min, 10_000_000, Some(4_294_967_296)).is_err());

        let me = Address::repeat_byte(1);
        assert!(check_tag_address("to", Address::ZERO, me).is_err());
        assert!(check_tag_address("to", me, me).is_err());
        assert!(check_tag_address("to", Address::repeat_byte(2), me).is_ok());
    }

    /// TC-342: vault instruction layout `[id | wallet | value(10) | 2 ignored | vaultId | 16 ignored]`
    /// (matches the memo printed by a live `-x sa-vault` dry run), and only vault ids encode.
    #[test]
    fn vault_instruction_layout() {
        use crate::signing::{encode_vault_instruction, smart_account_memo_data};
        let r = encode_vault_instruction(0x11, 0, 1_000_000, 1).unwrap();
        assert_eq!(
            smart_account_memo_data(&r),
            "1100000000000000000f42400000000100000000000000000000000000000000"
        );
        let r = encode_vault_instruction(0x23, 0, 20_251_218, 0x0203).unwrap();
        assert_eq!((r[0], &r[12..16]), (0x23, &[0, 0, 2, 3][..]));
        // Not vault ids: FXRP instructions, legacy CRT ids, junk.
        for id in [0x01, 0x02, 0x10, 0x20, 0x00, 0xFF] {
            assert!(encode_vault_instruction(id, 0, 1, 1).is_err(), "{id:#x}");
        }
        assert!(encode_vault_instruction(0x11, 0, 0, 1).is_err());
    }

    /// TC-344: memo recovery layouts from `MemoInstructions.sol` (lengths 42 / 42 / 50 / 30 / 10)
    /// and the client-side preconditions (nonce window, required fields, zero executor).
    #[test]
    fn recovery_memo_layouts_and_preconditions() {
        use crate::cli::MemoOp::*;
        let tx = "AB".repeat(32);
        let m = recovery_memo(IgnoreMemo, 7, Some(&tx), None, None, None, None).unwrap();
        assert_eq!(m.len(), 84);
        assert_eq!(&m[..20], "e0000000000000000007");
        assert_eq!(&m[20..], tx.to_lowercase());
        assert!(recovery_memo(IgnoreMemo, 0, Some("00"), None, None, None, None).is_err());
        assert!(recovery_memo(IgnoreMemo, 0, None, None, None, None, None).is_err());

        // newNonce > current, jump <= u32::MAX; big-endian in the last 32 bytes.
        let m = recovery_memo(SetNonce, 0, None, Some(9), None, None, Some(5)).unwrap();
        assert_eq!(m.len(), 84);
        assert!(m.starts_with("e1") && m.ends_with("0000000000000009"));
        assert!(recovery_memo(SetNonce, 0, None, Some(5), None, None, Some(5)).is_err());
        assert!(recovery_memo(SetNonce, 0, None, Some(3), None, None, Some(5)).is_err());
        let max = u64::from(u32::MAX);
        assert!(recovery_memo(SetNonce, 0, None, Some(5 + max), None, None, Some(5)).is_ok());
        assert!(recovery_memo(SetNonce, 0, None, Some(6 + max), None, None, Some(5)).is_err());
        assert!(recovery_memo(SetNonce, 0, None, Some(9), None, None, None).is_err());

        let m = recovery_memo(ReplaceFee, 1, Some(&tx), None, Some(2), None, None).unwrap();
        assert_eq!(m.len(), 100);
        assert!(m.ends_with("0000000000000002"));
        assert!(recovery_memo(ReplaceFee, 1, Some(&tx), None, None, None, None).is_err());

        let a = "0x1111111111111111111111111111111111111111";
        let m = recovery_memo(SetExecutor, 0, None, None, None, Some(a), None).unwrap();
        assert_eq!(m.len(), 60);
        assert!(m.ends_with(&"11".repeat(20)));
        let zero = "0x0000000000000000000000000000000000000000";
        assert!(recovery_memo(SetExecutor, 0, None, None, None, Some(zero), None).is_err());
        assert_eq!(
            recovery_memo(RemoveExecutor, 3, None, None, None, None, None).unwrap(),
            "d1000000000000000003"
        );

        // Target state: E0 / E2 only while unminted, E1 only after minted.
        assert!(check_target_state(IgnoreMemo, Some(false)).is_ok());
        assert!(check_target_state(IgnoreMemo, Some(true)).is_err());
        assert!(check_target_state(ReplaceFee, Some(true)).is_err());
        assert!(check_target_state(SetNonce, Some(true)).is_ok());
        assert!(check_target_state(SetNonce, Some(false)).is_err());
        assert!(check_target_state(SetNonce, None).is_err());
        assert!(check_target_state(RemoveExecutor, None).is_ok());
    }

    /// TC-345: `0xFE` user operation. The memo hash commits to `abi.encode(PackedUserOperation)`,
    /// which decodes back to the same operation (the controller does `abi.decode(_data, (PackedUserOperation))`).
    #[test]
    fn user_operation_encoding_and_memo() {
        use alloy::sol_types::SolValue;
        let personal = Address::repeat_byte(0xAA);
        let target = format!("{:#x}", Address::repeat_byte(0xBB));
        let calls = vec![format!("{target}:0xdeadbeef"), format!("{target}:0x")];
        let (bytes, memo) = build_user_operation(personal, 7, &calls, 9).unwrap();
        let op = <PackedUserOperation as SolValue>::abi_decode(&bytes).unwrap();
        assert_eq!((op.sender, op.nonce), (personal, U256::from(7)));
        assert!(op.signature.is_empty() && op.initCode.is_empty());
        let inner = executeUserOpCall::abi_decode(&op.callData).unwrap();
        assert_eq!(inner.calls.len(), 2);
        assert_eq!(inner.calls[0].data.as_ref(), &[0xde, 0xad, 0xbe, 0xef]);
        // memo = [0xFE | wallet 0 | fee 9 (u64 BE) | keccak256(bytes)]
        let m = hex::decode(&memo).unwrap();
        assert_eq!(m.len(), 42);
        assert_eq!((m[0], m[1], &m[2..10]), (0xFE, 0, &9_u64.to_be_bytes()[..]));
        assert_eq!(&m[10..], alloy::primitives::keccak256(&bytes).as_slice());
        // Refused: no calls, bad target, value-carrying calls are not expressible.
        assert!(build_user_operation(personal, 0, &[], 0).is_err());
        assert!(build_user_operation(personal, 0, &["0x12:0x".into()], 0).is_err());
        assert!(build_user_operation(personal, 0, &[format!("{target}:zz")], 0).is_err());
        assert!(build_user_operation(personal, 0, &[target], 0).is_err());
    }
}

#[cfg(test)]
mod finalize_wait_tests {
    use std::{cell::Cell, time::Duration};

    use super::wait_finalized;

    /// TC-366: the wait returns as soon as the check turns true, reports "not yet" (not an
    /// error) once the limit passes, and surfaces a failed read instead of hiding it as "not yet".
    #[tokio::test]
    async fn wait_finalized_polls_until_true_then_gives_up() {
        let calls = Cell::new(0);
        let on_third = || {
            calls.set(calls.get() + 1);
            let done = calls.get() == 3;
            async move { Ok(done) }
        };
        let tick = Duration::from_millis(1);
        assert!(
            wait_finalized(on_third, tick, Duration::from_secs(5))
                .await
                .unwrap()
        );
        assert_eq!(calls.get(), 3);

        let never = || async { Ok(false) };
        assert!(!wait_finalized(never, tick, Duration::ZERO).await.unwrap());

        let broken = || async { Err(color_eyre::eyre::eyre!("rpc down")) };
        let e = wait_finalized(broken, tick, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("rpc down"));
    }
}

#[cfg(test)]
mod await_executor_tests {
    use std::cell::RefCell;

    /// TC-366: a Flare read failure after the XRPL payment validated is only reported (with the
    /// `sa-status` re-check hint) — the function returns `()`, so it cannot fail the command and
    /// invite a second Core Vault payment. A bad hash and `--no-wait` are reported the same way.
    #[tokio::test]
    async fn await_public_executor_reports_instead_of_failing() {
        let hash = "ab".repeat(32);
        let lines = RefCell::new(Vec::<String>::new());
        let say = |m: &str| lines.borrow_mut().push(m.to_string());
        super::await_public_executor("http://127.0.0.1:1", &hash, true, say).await;
        super::await_public_executor("http://127.0.0.1:1", &hash, false, say).await;
        super::await_public_executor("http://127.0.0.1:1", "zz", true, say).await;
        let lines = lines.into_inner();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("check failed") && l.contains("sa-status")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("not waiting")),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("bad tx hash")), "{lines:?}");
    }
}
