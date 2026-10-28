use std::{borrow::Cow, time::Duration};

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use xrpl::{
    asynch::clients::{AsyncWebSocketClient, MultiExecutorMutex, XRPLAsyncWebsocketIO},
    models::requests::subscribe::{StreamParameter, Subscribe},
};

use crate::action::Action;

use super::types::TxSummary;
use super::util::{json_u32, next_backoff_secs};

/// No frame for this long = dead socket (ledgers close every ~4s): drop it and reconnect.
const WS_READ_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(30)
};

/// Emit one poll trigger per ledger index within a WebSocket session.
fn should_emit_ledger_trigger(last: Option<u32>, current: u32) -> bool {
    current > 0 && last != Some(current)
}

pub fn start_ws_task(
    ws_url: String,
    custom_ws: bool,
    network_watch: tokio::sync::watch::Receiver<crate::network::Network>,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(drive_ws_reconnect_loop(
        ws_url,
        custom_ws,
        network_watch,
        watch_address,
        action_tx,
        poll_trigger_tx,
        cancel,
    ))
}

async fn drive_ws_reconnect_loop(
    ws_url: String,
    custom_ws: bool,
    mut network_watch: tokio::sync::watch::Receiver<crate::network::Network>,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) {
    let mut backoff_secs: u64 = 0;
    let mut watching = true;
    let mut connected = *network_watch.borrow_and_update();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            network_res = network_watch.changed(), if watching => {
                match network_res {
                    // Sender gone: nobody can switch networks any more (and `changed` would
                    // return immediately forever).
                    Err(_) => watching = false,
                    Ok(()) if !custom_ws && *network_watch.borrow_and_update() != connected => {
                        backoff_secs = 0; // reconnect to the new network at once
                    }
                    Ok(()) => {}
                }
                continue;
            }
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
        }

        connected = *network_watch.borrow_and_update();
        let url = if custom_ws {
            ws_url.as_str()
        } else {
            connected.ws_url()
        };
        let mut subscribed = false;
        match connect_and_subscribe(
            url,
            &watch_address,
            &action_tx,
            &poll_trigger_tx,
            &cancel,
            &mut subscribed,
            (!custom_ws && watching).then_some((&mut network_watch, connected)),
        )
        .await
        {
            Ok(()) => return,
            Err(e) => {
                if !custom_ws && *network_watch.borrow() != connected {
                    backoff_secs = 0; // the session ended because the network switched
                } else if subscribed {
                    // Live session ended — restart backoff from the floor.
                    backoff_secs = next_backoff_secs(0);
                } else {
                    backoff_secs = next_backoff_secs(backoff_secs);
                }
                tracing::warn!(
                    delay_secs = backoff_secs,
                    error = %e,
                    "ws reconnect scheduled"
                );
            }
        }
    }
}

/// `follow`: the network watch + the network this session connected for (`None` for a custom WS
/// URL, which never follows network switches); a switch ends the session so the loop reconnects.
async fn connect_and_subscribe(
    ws_url: &str,
    watch_address: &Option<String>,
    action_tx: &UnboundedSender<Action>,
    poll_trigger_tx: &UnboundedSender<()>,
    cancel: &CancellationToken,
    subscribed: &mut bool,
    mut follow: Option<(
        &mut tokio::sync::watch::Receiver<crate::network::Network>,
        crate::network::Network,
    )>,
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
    *subscribed = true;

    let mut last_ledger_index = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            changed = async {
                match follow.as_mut() {
                    Some((watch, _)) => watch.changed().await.is_ok(),
                    None => std::future::pending().await,
                }
            } => {
                let Some((watch, connected)) = follow.as_mut() else { continue };
                if !changed {
                    // Sender gone: nobody can switch networks any more; stop watching.
                    follow = None;
                } else if *watch.borrow_and_update() != *connected {
                    return Err(color_eyre::eyre::eyre!("network changed"));
                }
            }
            // Raw frames: `xrpl_receive` re-wraps stream messages as `XRPLResponse`, which
            // drops the top-level `type`, so `ledgerClosed` would never match (TC-227).
            msg = tokio::time::timeout(WS_READ_TIMEOUT, ws.next()) => {
                let Ok(msg) = msg else {
                    return Err(color_eyre::eyre::eyre!(
                        "websocket read timeout ({}s)",
                        WS_READ_TIMEOUT.as_secs_f32()
                    ));
                };
                match msg {
                    Some(Ok(text)) => {
                        let value: Value = serde_json::from_str(&text)?;
                        let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
                        if event_type == "ledgerClosed" {
                            let ledger_index = json_u32(&value, &["ledger_index"]);
                            if !should_emit_ledger_trigger(last_ledger_index, ledger_index) {
                                continue;
                            }
                            last_ledger_index = Some(ledger_index);
                            let _ = action_tx.send(Action::XrplLedgerClose {
                                ledger_index,
                                base_fee: json_u32(&value, &["fee_base"]),
                                reserve_base: json_u32(&value, &["reserve_base"]),
                                reserve_inc: json_u32(&value, &["reserve_inc"]),
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
                    None => {
                        return Err(color_eyre::eyre::eyre!("websocket closed"));
                    }
                    Some(Err(e)) => {
                        return Err(e.into());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_emit_ledger_trigger;

    /// TC-103: duplicate ledger-close events emit one trigger per ledger index
    #[test]
    fn ledger_trigger_emits_once_per_nonzero_index() {
        assert!(!should_emit_ledger_trigger(None, 0));
        assert!(should_emit_ledger_trigger(None, 100));
        assert!(!should_emit_ledger_trigger(Some(100), 100));
        assert!(should_emit_ledger_trigger(Some(100), 101));
    }

    /// TC-227: local WebSocket fixture — subscribe carries the watched account + ledger stream;
    /// a ledger close emits `XrplLedgerClose` + one poll trigger (duplicate index dropped);
    /// an account tx emits `XrplAccountTx`; a server close or a malformed (non-JSON) frame
    /// reconnects and resubscribes.
    #[tokio::test]
    async fn ws_fixture_ledger_close_triggers_poll_and_reconnects() {
        use crate::action::Action;
        use futures::{SinkExt, StreamExt};
        use serde_json::{Value, json};
        use std::time::Duration;
        use tokio::{net::TcpListener, sync::mpsc, time::timeout};
        use tokio_tungstenite::{accept_async, tungstenite::Message};

        const WATCH: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("ws://{}", listener.local_addr().expect("addr"));
        let closed = |i: u32| {
            Message::text(
                // Full rippled shape: a typed parse of this (via `XRPLResponse`) loses `type`.
                json!({"type": "ledgerClosed", "ledger_index": i, "ledger_hash": "AA",
                       "ledger_time": 1, "txn_count": 0, "validated_ledgers": "1-200",
                       "fee_base": 10, "reserve_base": 1_000_000, "reserve_inc": 200_000})
                .to_string(),
            )
        };
        let server = tokio::spawn(async move {
            let mut subscribes = Vec::new();
            for session in 0..3 {
                let (tcp, _) = listener.accept().await.expect("accept");
                let mut ws = accept_async(tcp).await.expect("handshake");
                let Some(Ok(Message::Text(sub))) = ws.next().await else {
                    panic!("expected subscribe");
                };
                subscribes.push(serde_json::from_str::<Value>(&sub).expect("json"));
                if session == 0 {
                    for msg in [
                        closed(100),
                        closed(100),
                        Message::text(
                            json!({"type": "transaction", "transaction": {"hash": "AB12"}})
                                .to_string(),
                        ),
                        closed(101),
                    ] {
                        ws.send(msg).await.expect("send");
                    }
                    ws.close(None).await.expect("close");
                } else if session == 1 {
                    // A non-JSON frame ends the session (reconnect), after the valid close is handled.
                    ws.send(closed(102)).await.expect("send");
                    ws.send(Message::text("this is not json"))
                        .await
                        .expect("send");
                } else {
                    ws.send(closed(103)).await.expect("send");
                    return (subscribes, ws);
                }
            }
            unreachable!()
        });

        let (_net_tx, net_rx) = tokio::sync::watch::channel(crate::network::Network::Testnet);
        let (action_tx, mut actions) = mpsc::unbounded_channel();
        let (trigger_tx, mut triggers) = mpsc::unbounded_channel();
        let cancel = tokio_util::sync::CancellationToken::new();
        let task = super::start_ws_task(
            url,
            true,
            net_rx,
            Some(WATCH.into()),
            action_tx,
            trigger_tx,
            cancel.clone(),
        );

        let mut ledgers = Vec::new();
        let mut tx_hashes = Vec::new();
        while ledgers.len() < 4 {
            match timeout(Duration::from_secs(10), actions.recv()).await {
                Ok(Some(Action::XrplLedgerClose {
                    ledger_index,
                    base_fee,
                    ..
                })) => {
                    assert_eq!(base_fee, 10);
                    ledgers.push(ledger_index);
                }
                Ok(Some(Action::XrplAccountTx(tx))) => tx_hashes.push(tx.hash.clone()),
                other => panic!("unexpected {other:?} after {ledgers:?}"),
            }
        }
        assert_eq!(
            ledgers,
            [100, 101, 102, 103],
            "duplicate 100 dropped; 102 after reconnect; 103 after malformed frame reconnect"
        );
        assert_eq!(tx_hashes, ["AB12"]);
        let mut trigger_count = 0;
        while triggers.try_recv().is_ok() {
            trigger_count += 1;
        }
        assert_eq!(trigger_count, 4);

        let (subscribes, _ws) = server.await.expect("server");
        for sub in &subscribes {
            assert_eq!(sub["command"], "subscribe");
            assert_eq!(sub["accounts"], json!([WATCH]));
            assert_eq!(sub["streams"], json!(["ledger"]));
        }
        cancel.cancel();
        timeout(Duration::from_secs(5), task)
            .await
            .expect("task stops on cancel")
            .expect("join");
    }

    /// TC-354: silent socket times out and reconnects
    /// A socket that goes silent (no frames, no close) is abandoned after the read timeout and
    /// the loop reconnects.
    #[tokio::test]
    async fn ws_silent_socket_times_out_and_reconnects() {
        use crate::action::Action;
        use futures::{SinkExt, StreamExt};
        use serde_json::json;
        use std::time::Duration;
        use tokio::{net::TcpListener, sync::mpsc, time::timeout};
        use tokio_tungstenite::{accept_async, tungstenite::Message};

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("ws://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            // Session 0: subscribe received, then silence (socket kept open).
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut silent = accept_async(tcp).await.expect("handshake");
            let _ = silent.next().await;
            let (tcp, _) = listener.accept().await.expect("reconnect");
            let mut ws = accept_async(tcp).await.expect("handshake");
            let _ = ws.next().await;
            ws.send(Message::text(
                json!({"type": "ledgerClosed", "ledger_index": 7}).to_string(),
            ))
            .await
            .expect("send");
            (silent, ws)
        });
        let (_net_tx, net_rx) = tokio::sync::watch::channel(crate::network::Network::Testnet);
        let (action_tx, mut actions) = mpsc::unbounded_channel();
        let (trigger_tx, _triggers) = mpsc::unbounded_channel();
        let cancel = tokio_util::sync::CancellationToken::new();
        let task = super::start_ws_task(
            url,
            true,
            net_rx,
            None,
            action_tx,
            trigger_tx,
            cancel.clone(),
        );
        match timeout(Duration::from_secs(15), actions.recv()).await {
            Ok(Some(Action::XrplLedgerClose { ledger_index, .. })) => assert_eq!(ledger_index, 7),
            other => panic!("expected reconnect + ledger close, got {other:?}"),
        }
        let _sockets = server.await.expect("server");
        cancel.cancel();
        timeout(Duration::from_secs(5), task)
            .await
            .expect("task stops on cancel")
            .expect("join");
    }
}
