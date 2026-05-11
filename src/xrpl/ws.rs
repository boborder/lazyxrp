use std::{borrow::Cow, time::Duration};

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use xrpl::{
    asynch::clients::{AsyncWebSocketClient, MultiExecutorMutex, XRPLAsyncWebsocketIO},
    models::requests::subscribe::{StreamParameter, Subscribe},
};

use crate::action::Action;

use super::backoff::next_backoff_secs;
use super::json_util::extract_json_u32;
use super::types::TxSummary;

pub fn start_ws_task(
    ws_url: String,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_ws_loop(
        ws_url,
        watch_address,
        action_tx,
        poll_trigger_tx,
        cancel,
    ))
}

async fn run_ws_loop(
    ws_url: String,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) {
    let mut backoff_secs: u64 = 0;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {
                match connect_and_subscribe(&ws_url, &watch_address, &action_tx, &poll_trigger_tx, &cancel).await {
                    Ok(()) => return,
                    Err(e) => {
                        tracing::error!("ws error: {e}");
                        backoff_secs = next_backoff_secs(backoff_secs);
                    }
                }
            }
        }
    }
}

async fn connect_and_subscribe(
    ws_url: &str,
    watch_address: &Option<String>,
    action_tx: &UnboundedSender<Action>,
    poll_trigger_tx: &UnboundedSender<()>,
    cancel: &CancellationToken,
) -> color_eyre::Result<()> {
    let parsed_url = ws_url
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid ws url: {e}"))?;
    let mut ws: AsyncWebSocketClient<MultiExecutorMutex, _> =
        AsyncWebSocketClient::open(parsed_url).await?;

    let accounts = watch_address
        .as_deref()
        .map(|a| vec![Cow::Owned(a.to_string())])
        .unwrap_or_default();
    let sub = Subscribe::new(
        None,
        Some(accounts),
        None,
        None,
        Some(vec![StreamParameter::Ledger]),
        None,
        None,
        None,
    );
    ws.xrpl_send(sub.into()).await?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            msg = ws.xrpl_receive() => {
                match msg {
                    Ok(Some(ws_msg)) => {
                        let value = serde_json::to_value(ws_msg)?;
                        let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
                        if event_type == "ledgerClosed" {
                            let _ = action_tx.send(Action::XrplLedgerClose {
                                ledger_index: extract_json_u32(&value, &["ledger_index"]),
                                base_fee: extract_json_u32(&value, &["fee_base"]),
                                reserve_base: extract_json_u32(&value, &["reserve_base"]),
                                reserve_inc: extract_json_u32(&value, &["reserve_inc"]),
                            });
                            let _ = poll_trigger_tx.send(());
                        } else if value.get("transaction").is_some() {
                            let hash = value
                                .get("transaction")
                                .and_then(|v| v.get("hash"))
                                .and_then(Value::as_str)
                                .unwrap_or("-")
                                .to_string();
                            let _ = action_tx.send(Action::XrplAccountTx(Box::new(TxSummary { hash })));
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("ws stream ended");
                        return Err(color_eyre::eyre::eyre!("websocket closed"));
                    }
                    Err(e) => {
                        tracing::error!("ws receive error: {e}");
                        return Err(e.into());
                    }
                }
            }
        }
    }
}
