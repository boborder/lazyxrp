use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Tabs},
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fps::FpsCounter, help_overlay::HelpOverlay, splash::SplashScreen,
            status_bar::StatusBar, theme,
        },
        tabs::{
            account_wallet::AccountWalletTab, assets::AssetsTab, market_oracle::MarketOracleTab,
            overview::OverviewTab,
        },
    },
    config::{Config, FlareConfig, Secrets},
    flare::DEFAULT_FLARE_FEEDS,
    network::Network,
    tui::{Event, Tui},
    xrpl::{
        BookPair, PollCommand, PollContext, fetch_xrpl_toml_with_meta, start_poll_task,
        start_ws_task,
    },
};

/// Tab labels (index mirrors `panels` Vec order)
pub(crate) const TAB_TITLES: &[&str] = &["󰖟 Overview", "󰀉 Account", "󰠿 Market", "󰒍 Assets"];
/// Indices into [`TAB_TITLES`] / `panels`.
const ACCOUNT_TAB: usize = 1;
const ASSETS_TAB: usize = 3;

fn footer_line(active_tab: usize) -> Line<'static> {
    let bold = Style::new().bold();
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(27);
    let mut hint = |key: &'static str, label: &'static str| {
        spans.push(Span::styled(key, bold));
        spans.push(Span::raw(label));
        spans.push(Span::raw("  "));
    };
    hint("?", ":help");
    hint("Tab", ":next");
    hint("1-4", ":jump");
    hint("↑↓/jk", ":row");
    hint("hl/←→", ":focus");
    hint("^Z", ":suspend");
    // Tab indices: 0 Overview, 1 Account, 2 Market, 3 Assets
    match active_tab {
        0 => {
            hint("r", ":refresh");
            hint("Enter", ":dUNL");
        }
        1 => {
            hint("t", ":tx");
            hint("g", ":keygen");
            hint("f", ":filter");
            hint("r", ":refresh");
        }
        2 => hint("b", ":book"),
        3 => {
            hint("o", ":objects");
            hint("x", ":burn NFT");
        }
        _ => {}
    }
    hint("q", ":quit");
    Line::from(spans)
}

pub struct App {
    config: Arc<Config>,
    /// Signing credential and Flare EVM key: kept out of the shared `config`, moved into the poll
    /// task by `run()`.
    credential: Option<crate::signing::SigningCredential>,
    flare_evm_key: Option<secrecy::SecretString>,
    /// A seed or mnemonic is configured: the wallet pane (and its composer) exists. Kept after
    /// `run()` moves the credential into the poll task, so `TxComposerOpen` can still route.
    has_wallet: bool,
    /// A form/modal is typing: skip the global keymap in `on_key_event`.
    keymap_suppressed: bool,
    tick_rate: f64,
    /// One panel per tab — index matches TAB_TITLES
    panels: Vec<Box<dyn Component>>,
    status_bar: StatusBar,
    fps: FpsCounter,
    active_tab: usize,
    splash: Box<dyn Component>,
    help: HelpOverlay,
    startup_done: bool,
    show_help: bool,
    last_refresh_account: Option<Instant>,
    last_refresh_book: Option<Instant>,
    last_refresh_ledger_objects: Option<Instant>,
    last_refresh_nfts: Option<Instant>,
    last_refresh_lines: Option<Instant>,
    last_refresh_tx_history: Option<Instant>,
    should_quit: bool,
    should_suspend: bool,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    poll_tx: mpsc::UnboundedSender<PollCommand>,
    /// Lets unit tests read the poll commands `App::new` queues; `run` replaces the channel.
    #[cfg(test)]
    test_poll_rx: Option<mpsc::UnboundedReceiver<PollCommand>>,
    rpc_server: String,
    ws_server: String,
    watch_account: String,
    network: Network,
    custom_rpc: bool,
    custom_ws: bool,
    net_tx: watch::Sender<Network>,
    tab_tx: watch::Sender<usize>,
    needs_draw: bool,
    /// NFT ids / domains whose fetch is still running (a repeat request is dropped).
    nft_images_inflight: HashSet<String>,
    toml_inflight: HashSet<String>,
}

/// `FLARE_RPC_URL` (trimmed, non-empty) wins over the `[flare] network` preset, which itself
/// falls back to Flare mainnet: there is always an endpoint.
pub(crate) fn resolve_flare_rpc_url(flare: &FlareConfig) -> String {
    std::env::var("FLARE_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| flare.network.rpc_url().to_string())
}

fn resolve_flare_feeds() -> Vec<String> {
    if let Ok(raw) = std::env::var("FLARE_FEEDS") {
        let feeds: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        if !feeds.is_empty() {
            return feeds;
        }
    }
    if let Ok(one) = std::env::var("FLARE_FEED") {
        let t = one.trim().to_string();
        if !t.is_empty() {
            return vec![t];
        }
    }
    DEFAULT_FLARE_FEEDS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// The poll-task command an action asks for, if it is one of the actions forwarded to the task.
fn poll_command_for(action: &Action) -> Option<PollCommand> {
    Some(match action {
        Action::AccountSetSubmit(params) => PollCommand::AccountSetSubmit(params.clone()),
        Action::PaymentSubmit(params) => PollCommand::PaymentSubmit(params.clone()),
        Action::PaymentQuote(params) => PollCommand::PaymentQuote(params.clone()),
        Action::SetRegularKeySubmit(params) => PollCommand::SetRegularKeySubmit(params.clone()),
        Action::EscrowCreateSubmit(params) => PollCommand::EscrowCreateSubmit(params.clone()),
        Action::EscrowCloseSubmit(params) => PollCommand::EscrowCloseSubmit(params.clone()),
        Action::AmmDepositSubmit(params) => PollCommand::AmmDepositSubmit(params.clone()),
        Action::AmmWithdrawSubmit(params) => PollCommand::AmmWithdrawSubmit(params.clone()),
        Action::AccountDeleteSubmit(params) => PollCommand::AccountDeleteSubmit(params.clone()),
        Action::GenericTxSubmit(params) => PollCommand::GenericTx(params.clone()),
        Action::OfferCreateSubmit(params) => PollCommand::OfferCreateSubmit(params.clone()),
        Action::OfferCancelSubmit(params) => PollCommand::OfferCancelSubmit(params.clone()),
        Action::TrustSetSubmit(params) => PollCommand::TrustSetSubmit(params.clone()),
        Action::FxrpDirectMintPaymentSubmit(params) => {
            PollCommand::FxrpDirectMintPayment(params.clone())
        }
        Action::FxrpExecuteDirectMintSubmit(params) => {
            PollCommand::FxrpExecuteDirectMint(params.clone())
        }
        Action::SmartAccountInstructionSubmit(params) => {
            PollCommand::SmartAccountInstruction(params.clone())
        }
        Action::RefreshAccountEscrows => PollCommand::AccountEscrows,
        Action::RefreshAccountOffers => PollCommand::AccountOffers,
        Action::GenerateWalletKeys => PollCommand::GenerateWalletKeys("ed25519".into()),
        _ => return None,
    })
}

/// The resolved network and the RPC / WebSocket URLs to use for it (CLI, env and config
/// overrides already applied).
pub struct Endpoints {
    pub network: Network,
    pub rpc_url: String,
    pub ws_url: String,
}

impl App {
    pub fn new(
        tick_rate: f64,
        endpoints: Endpoints,
        account: Option<String>,
        mut config: Config,
        secrets: Secrets,
        skip_mainnet_prompt: bool,
    ) -> color_eyre::Result<Self> {
        let Endpoints {
            network,
            rpc_url: rpc_server,
            ws_url: ws_server,
        } = endpoints;
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (poll_tx, poll_rx) = mpsc::unbounded_channel();
        // `run` replaces this channel; only unit tests keep the receiver to inspect what was queued.
        #[cfg(test)]
        let test_poll_rx = Some(poll_rx);
        #[cfg(not(test))]
        drop(poll_rx);
        let (net_tx, _net_rx) = watch::channel(network);
        let (tab_tx, _tab_rx) = watch::channel(0usize);
        // Keep wallet UI / config network aligned with the resolved CLI/env network.
        config.xrpl.network = network;
        let credential = secrets.credential()?;
        // Derived once; the wallet pane gets only the address, never a key.
        let wallet = credential
            .as_ref()
            .map(|c| c.address().map_err(|e| e.to_string()));
        let has_wallet = wallet.is_some();
        let watch_account = account.unwrap_or_else(|| {
            // The wallet signs for its own address: history/refresh must follow it, not the
            // config default, or "Recent txs" would describe a different account.
            wallet
                .clone()
                .and_then(Result::ok)
                .unwrap_or_else(|| config.xrpl.account.clone())
        });
        let flare_display = config.flare.display;
        let custom_rpc = rpc_server != network.rpc_url();
        let custom_ws = ws_server != network.ws_url();
        let panels: Vec<Box<dyn Component>> = vec![
            Box::new(OverviewTab::new(rpc_server.clone(), flare_display)),
            Box::new(AccountWalletTab::new(skip_mainnet_prompt, wallet)),
            Box::new(MarketOracleTab::new(flare_display)),
            Box::new(AssetsTab::new()),
        ];
        // UA-1: guard against tab/panel index mismatch.
        debug_assert_eq!(
            TAB_TITLES.len(),
            panels.len(),
            "TAB_TITLES and panels must have same length"
        );
        Ok(Self {
            keymap_suppressed: false,
            tick_rate,
            panels,
            status_bar: StatusBar::new(
                watch_account.clone(),
                network,
                flare_display != crate::config::FlareDisplay::Off,
            ),
            fps: FpsCounter::default(),
            active_tab: 0,
            splash: Box::new(SplashScreen::default()),
            help: HelpOverlay,
            startup_done: false,
            show_help: false,
            last_refresh_account: None,
            last_refresh_book: None,
            last_refresh_ledger_objects: None,
            last_refresh_nfts: None,
            last_refresh_lines: None,
            last_refresh_tx_history: None,
            should_quit: false,
            should_suspend: false,
            config: Arc::new(config),
            credential,
            flare_evm_key: secrets.flare_evm_key,
            has_wallet,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
            poll_tx,
            #[cfg(test)]
            test_poll_rx,
            rpc_server,
            ws_server,
            watch_account,
            network,
            custom_rpc,
            custom_ws,
            net_tx,
            tab_tx,
            needs_draw: true,
            nft_images_inflight: HashSet::new(),
            toml_inflight: HashSet::new(),
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let mut tui = Tui::new()?.tick_rate(self.tick_rate);
        tui.enter()?;

        let action_tx = self.action_tx.clone();
        for panel in self.panels.iter_mut() {
            panel.register_action_handler(action_tx.clone())?;
            panel.register_config_handler(Arc::clone(&self.config))?;
            panel.init(tui.terminal.size()?)?;
        }
        self.status_bar.register_action_handler(action_tx.clone())?;
        self.status_bar
            .register_config_handler(Arc::clone(&self.config))?;
        self.status_bar.init(tui.terminal.size()?)?;
        self.splash.register_action_handler(action_tx.clone())?;
        self.splash
            .register_config_handler(Arc::clone(&self.config))?;
        self.splash.init(tui.terminal.size()?)?;
        tui.start();

        let (poll_tx, poll_rx) = mpsc::unbounded_channel();
        let (poll_trigger_tx, poll_trigger_rx) = mpsc::unbounded_channel();
        self.poll_tx = poll_tx;
        #[cfg(test)]
        drop(self.test_poll_rx.take());
        let cancel = CancellationToken::new();

        let book_pair = BookPair::from_config(&self.config.xrpl);
        start_ws_task(
            self.ws_server.clone(),
            self.custom_ws,
            self.net_tx.subscribe(),
            Some(self.watch_account.clone()),
            action_tx.clone(),
            poll_trigger_tx,
            cancel.clone(),
        );
        let signing_credential = self.credential.take();
        let seed_address = signing_credential.as_ref().and_then(|c| c.address().ok());
        let flare_rpc_url = resolve_flare_rpc_url(&self.config.flare);
        let flare_feeds = resolve_flare_feeds();
        start_poll_task(
            PollContext {
                rpc_url: self.rpc_server.clone(),
                custom_rpc: self.custom_rpc,
                watch_address: self.watch_account.clone(),
                book_pair,
                poll_interval: Duration::from_millis(self.config.xrpl.poll_interval_ms),
                seed_address,
                signing_credential,
                network_watch: self.net_tx.subscribe(),
                tab_watch: self.tab_tx.subscribe(),
                oracles: self.config.xrpl.oracles.clone(),
                oracle_pairs: self.config.xrpl.oracle_pairs.clone(),
                flare_rpc_url: Some(flare_rpc_url),
                flare_feeds,
                flare_expected_core_vault: self.config.flare.fassets.expected_core_vault.clone(),
                flare_expected_operator: self.config.flare.smart_accounts.expected_operator.clone(),
                flare_fassets_execute: self.config.flare.fassets.execute,
                flare_network: self.config.flare.network,
                flare_display: self.config.flare.display,
                flare_evm_key_env: self.config.flare.fassets.evm_key_env.clone(),
                flare_evm_key: self.flare_evm_key.take(),
                flare_wallet_address: self.config.flare.wallet.address.clone(),
                submit_lock: Arc::new(tokio::sync::Mutex::new(())),
            },
            poll_rx,
            poll_trigger_rx,
            action_tx.clone(),
            cancel.clone(),
        );
        // The Oracle panel is the XRPL on-chain oracle table; without both lists nothing is ever
        // fetched, so say so instead of spinning forever.
        if (self.config.xrpl.oracles.is_empty() || self.config.xrpl.oracle_pairs.is_empty())
            && action_tx.send(Action::XrplOracleNotConfigured).is_err()
        {
            warn!("action channel closed (oracle not configured)");
        }
        loop {
            self.forward_tui_events(&mut tui).await?;
            self.drain_and_dispatch_actions(Some(&mut tui))?;
            if self.needs_draw && !self.should_suspend && !self.should_quit {
                self.draw_frame(&mut tui)?;
            }
            if self.should_suspend {
                tui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                tui.resume()?;
            } else if self.should_quit {
                cancel.cancel();
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }

    async fn forward_tui_events(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        let Some(event) = tui.next_event().await else {
            return Ok(());
        };
        self.route_event(&event)
    }

    /// Keys and paste belong to the visible tab only: a hidden tab must not open modals,
    /// move selections or fire fetches from input the user typed elsewhere.
    fn route_event(&mut self, event: &Event) -> color_eyre::Result<()> {
        let action_tx = self.action_tx.clone();
        match event {
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(*x, *y))?,
            Event::Key(key) => self.on_key_event(*key)?,
            _ => {}
        }
        // The help overlay owns the keyboard while open.
        if self.show_help {
            return Ok(());
        }
        if let Some(panel) = self.panels.get_mut(self.active_tab)
            && let Some(action) = panel.on_event(Some(event))?
        {
            action_tx.send(action)?;
        }
        Ok(())
    }

    fn on_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        let action_tx = self.action_tx.clone();
        if self.keymap_suppressed {
            // Modal/form has key priority (ratatui modal routing): bare `q` must
            // reach the focused composer (DangerConfirm / payment fields), not quit.
            // Force-quit remains available via Ctrl-C / Ctrl-D.
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    action_tx.send(Action::Quit)?;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    action_tx.send(Action::Quit)?;
                }
                _ => {}
            }
            return Ok(());
        }

        // Tab / BackTab handled directly (not through config keybindings)
        match key.code {
            KeyCode::Tab => {
                action_tx.send(Action::TabNext)?;
                return Ok(());
            }
            KeyCode::BackTab => {
                action_tx.send(Action::TabPrev)?;
                return Ok(());
            }
            _ => {}
        }

        if self.show_help {
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('?') {
                action_tx.send(Action::Help)?;
            }
            return Ok(());
        }

        if let KeyCode::Char(c) = key.code
            && c.is_ascii_digit()
            && let Some(d) = c.to_digit(10)
            && d >= 1
            && (d as usize) <= TAB_TITLES.len()
        {
            action_tx.send(Action::TabJump(d as usize - 1))?;
            return Ok(());
        }
        // Assets tab: `x` starts NFTokenBurn for the selected NFT (NftTab answers with TxComposerOpen).
        if self.active_tab == ASSETS_TAB
            && key.code == KeyCode::Char('x')
            && key.modifiers.is_empty()
        {
            action_tx.send(Action::NftBurnStart)?;
            return Ok(());
        }

        let keymap = &self.config.keybindings.0;
        match keymap.get(&vec![key]) {
            Some(action) => {
                info!("Got action: {action:?}");
                action_tx.send(action.clone())?;
            }
            _ => {
                self.last_tick_key_events.push(key);
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    info!("Got action: {action:?}");
                    action_tx.send(action.clone())?;
                    // A matched sequence is consumed: otherwise the stale prefix would make
                    // the same sequence fail to fire again until the next Tick clears it.
                    self.last_tick_key_events.clear();
                }
            }
        }
        Ok(())
    }

    /// Send a poll command, logging a warning when the channel is closed.
    fn send_poll(&self, command: PollCommand) {
        if let Err(err) = self.poll_tx.send(command) {
            warn!(?err, "poll command channel closed");
        }
    }

    /// Send a poll command only if at least 500 ms have passed since the last one.
    fn send_debounced_poll(
        last: &mut Option<Instant>,
        tx: &mpsc::UnboundedSender<PollCommand>,
        cmd: PollCommand,
    ) {
        const MIN: Duration = Duration::from_millis(500);
        let now = Instant::now();
        if last.is_none_or(|t| now - t >= MIN) {
            *last = Some(now);
            if let Err(err) = tx.send(cmd) {
                warn!(?err, "poll command channel closed");
            }
        }
    }

    fn drain_and_dispatch_actions(&mut self, mut tui: Option<&mut Tui>) -> color_eyre::Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if !matches!(&action, Action::Tick | Action::NftImageLoaded { .. }) {
                debug!("{action:?}");
            }
            self.mark_dirty(&action);
            self.handle_action(&action, tui.as_deref_mut())?;
            self.fan_out(&action)?;
        }
        Ok(())
    }

    /// Dirty policy: keys/resize/data/UI => always; Tick => splash only here; FPS label refresh
    /// marks dirty after fps.note_action. `run()` draws once per loop iteration when dirty
    /// (`draw_frame`), coalescing queued actions. The FPS frame counter is fed directly by
    /// `draw_frame`.
    fn mark_dirty(&mut self, action: &Action) {
        match action {
            Action::Tick => {
                self.last_tick_key_events.drain(..);
                if !self.startup_done {
                    self.needs_draw = true;
                }
            }
            _ => {
                self.needs_draw = true;
            }
        }
    }

    /// App-level effects of one action: quit/suspend, tab and network switches, background fetches,
    /// poll commands. Panels see every action afterwards, in [`App::fan_out`].
    fn handle_action(&mut self, action: &Action, tui: Option<&mut Tui>) -> color_eyre::Result<()> {
        match action {
            Action::Tick => {}
            Action::Quit => self.should_quit = true,
            Action::Suspend => self.should_suspend = true,
            Action::Resume => self.should_suspend = false,
            Action::ClearScreen => {
                if let Some(tui) = tui {
                    tui.terminal.clear()?;
                }
            }
            Action::Resize(w, h) => {
                if let Some(tui) = tui {
                    self.on_resize(tui, *w, *h)?;
                }
            }
            Action::TabNext => {
                self.active_tab = (self.active_tab + 1) % TAB_TITLES.len();
                let _ = self.tab_tx.send(self.active_tab);
            }
            Action::TabPrev => {
                self.active_tab = if self.active_tab == 0 {
                    TAB_TITLES.len() - 1
                } else {
                    self.active_tab - 1
                };
                let _ = self.tab_tx.send(self.active_tab);
            }
            Action::TabJump(i) if *i < TAB_TITLES.len() => {
                self.active_tab = *i;
                let _ = self.tab_tx.send(self.active_tab);
            }
            Action::NetworkChange(net) => {
                self.network = *net;
                if let Err(err) = self.net_tx.send(*net) {
                    warn!(?err, "network watch channel closed");
                }
            }
            Action::NetworkSwitchCycle => {
                let next = self.network.next_network();
                if self.action_tx.send(Action::NetworkChange(next)).is_err() {
                    warn!("action channel closed (network switch)");
                }
            }
            Action::NftImageRequest { nft_id, uri } => self.request_nft_image(nft_id, uri),
            Action::RequestXrplToml {
                domain,
                expected_pubkey,
            } => self.request_xrpl_toml(domain, expected_pubkey),
            Action::XrplTomlFetched { domain, .. } => {
                self.toml_inflight.remove(domain);
            }
            Action::NftImageLoaded { nft_id, .. } | Action::NftImageError { nft_id, .. } => {
                self.nft_images_inflight.remove(nft_id);
            }
            Action::RefreshAccount => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_account,
                    &self.poll_tx,
                    PollCommand::Account,
                );
            }
            Action::RefreshBook => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_book,
                    &self.poll_tx,
                    PollCommand::Book,
                );
            }
            Action::RefreshNfts => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_nfts,
                    &self.poll_tx,
                    PollCommand::Nfts,
                );
            }
            Action::RefreshLines => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_lines,
                    &self.poll_tx,
                    PollCommand::Lines,
                );
            }
            Action::RefreshTxHistory => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_tx_history,
                    &self.poll_tx,
                    PollCommand::TxHistory,
                );
            }
            Action::RefreshTxHistoryMore(marker) => {
                if let Err(err) = self
                    .poll_tx
                    .send(PollCommand::TxHistoryMore(marker.clone()))
                {
                    warn!(?err, "poll command channel closed");
                }
            }
            Action::RefreshLedgerObjects => {
                Self::send_debounced_poll(
                    &mut self.last_refresh_ledger_objects,
                    &self.poll_tx,
                    PollCommand::LedgerObjects,
                );
            }
            Action::SetKeymapSuppression(on) => {
                self.keymap_suppressed = *on;
            }
            Action::TxComposerOpen { .. } => {
                // Only the signing wallet answers it (--watch has none): never jump to
                // an Account tab that would show no composer.
                if self.has_wallet {
                    self.active_tab = ACCOUNT_TAB;
                    let _ = self.tab_tx.send(self.active_tab);
                }
            }
            Action::XrplServerInfo(_) => self.startup_done = true,
            // Unreachable RPC: leave the splash so the error UI is visible. These are typed
            // actions, not string-prefix matching on XrplError.
            Action::XrplServerInfoError(_) | Action::XrplRpcInitError(_) => {
                self.startup_done = true;
            }
            Action::Help => self.show_help = !self.show_help,
            _ => {
                if let Some(command) = poll_command_for(action) {
                    self.send_poll(command);
                }
            }
        }
        Ok(())
    }

    /// Fetch an NFT image in the background; a request already in flight is ignored.
    fn request_nft_image(&mut self, nft_id: &str, uri: &str) {
        if self.nft_images_inflight.insert(nft_id.to_string()) {
            let action_tx = self.action_tx.clone();
            let nft_id = nft_id.to_string();
            let uri = uri.to_string();
            tokio::spawn(async move {
                match crate::xrpl::fetch_nft_image(&uri).await {
                    Ok(image) => {
                        if let Err(err) = action_tx.send(Action::NftImageLoaded {
                            nft_id,
                            bytes: Arc::new(image.bytes),
                        }) {
                            warn!(?err, "action channel closed (nft image)");
                        }
                    }
                    Err(err) => {
                        let _ = action_tx.send(Action::NftImageError {
                            nft_id,
                            message: err.to_string(),
                        });
                    }
                }
            });
        }
    }

    /// Fetch a validator's xrp-ledger.toml in the background; a request already in flight is ignored.
    fn request_xrpl_toml(&mut self, domain: &str, expected_pubkey: &str) {
        if self.toml_inflight.insert(domain.to_string()) {
            let domain = domain.to_string();
            let expected_pubkey = expected_pubkey.to_string();
            let tx = self.action_tx.clone();
            tokio::spawn(async move {
                let fetched =
                    fetch_xrpl_toml_with_meta(&domain, &expected_pubkey, Duration::from_secs(10))
                        .await;
                if tx
                    .send(Action::XrplTomlFetched {
                        domain,
                        status: fetched.status,
                        content_type: fetched.content_type,
                        raw: fetched.raw,
                        result: fetched.result,
                    })
                    .is_err()
                {
                    warn!("action channel closed (xrpl toml fetch)");
                }
            });
        }
    }

    /// Hand the action to the panels, the status bar and the splash.
    fn fan_out(&mut self, action: &Action) -> color_eyre::Result<()> {
        // Nav/selection/overlay actions belong to the visible tab; data and results fan out.
        let targets = if matches!(
            action,
            Action::FocusNext
                | Action::FocusPrev
                | Action::SelectNext
                | Action::SelectPrev
                | Action::TxDetailToggle
                | Action::NftBurnStart
        ) {
            self.active_tab..self.active_tab + 1
        } else {
            0..self.panels.len()
        };
        for panel in &mut self.panels[targets] {
            if let Some(a) = panel.update(action)? {
                self.action_tx.send(a)?;
            }
        }
        if let Some(a) = self.status_bar.update(action)? {
            self.action_tx.send(a)?;
        }
        // FPS tick-rate label: dirty at most once/sec when the text changes.
        if matches!(action, Action::Tick) && self.fps.note_action(action) {
            self.needs_draw = true;
        }
        if !self.startup_done {
            self.splash.update(action)?;
        }
        Ok(())
    }

    fn on_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> color_eyre::Result<()> {
        tui.terminal.resize(Rect::new(0, 0, w, h))?;
        self.draw_frame(tui)?;
        Ok(())
    }

    /// Draw once and clear the dirty flag; the FPS label counts real draws only.
    fn draw_frame(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        self.render(tui)?;
        self.needs_draw = false;
        self.fps.note_frame();
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        if !self.startup_done {
            let splash = &mut self.splash;
            tui.terminal.draw(|frame| {
                if let Err(err) = splash.draw(frame, frame.area()) {
                    error!(?err, "failed to draw splash");
                }
            })?;
            return Ok(());
        }

        let show_help = self.show_help;
        let active_tab = self.active_tab;
        let help = &mut self.help;
        let panels = &mut self.panels;
        let status_bar = &mut self.status_bar;
        let fps = &mut self.fps;

        tui.terminal.draw(|frame| {
            let [tabs_area, main_area, hints_area, status_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(frame.area());

            let titles: Vec<Line<'_>> = TAB_TITLES
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Line::from(vec![
                        Span::styled(format!("{} ", i + 1), theme::dim_style()),
                        Span::raw(*t),
                    ])
                })
                .collect();
            let tabs = Tabs::new(titles)
                .select(active_tab)
                .divider(Span::styled(" │ ", theme::dim_style()))
                .highlight_style(Style::new().fg(theme::HIGHLIGHT_BG).bold().underlined())
                .block(Block::default());
            frame.render_widget(tabs, tabs_area);

            let hints = footer_line(active_tab);
            frame.render_widget(Paragraph::new(hints).style(theme::dim_style()), hints_area);

            if let Some(panel) = panels.get_mut(active_tab)
                && let Err(err) = panel.draw(frame, main_area)
            {
                error!(?err, "failed to draw panel");
            }

            let fps_area = Rect::new(main_area.right().saturating_sub(10), main_area.top(), 10, 1);
            if let Err(err) = fps.draw(frame, fps_area) {
                error!(?err, "failed to draw fps");
            }

            if let Err(err) = status_bar.draw(frame, status_area) {
                error!(?err, "failed to draw status");
            }

            if show_help && let Err(err) = help.draw(frame, frame.area()) {
                error!(?err, "failed to draw help");
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::{config::Config, network::Network};

    /// Builds the app from the embedded default config only: holds the env lock and points
    /// XDG at an empty dir so parallel env-mutating tests and a developer's real
    /// `config.toml` cannot leak in.
    async fn test_app() -> color_eyre::Result<App> {
        test_app_seeded(None).await
    }

    async fn test_app_seeded(seed: Option<&str>) -> color_eyre::Result<App> {
        use crate::test_support::{TestEnvGuard, env_lock_async};
        let config = {
            let _lock = env_lock_async().await;
            let env = TestEnvGuard::new(&[
                "LAZYXRP_CONFIG",
                "LAZYXRP_DATA",
                "XDG_CONFIG_HOME",
                "XRPL_NETWORK",
                "XRPL_RPC_SERVER",
                "XRPL_WS_SERVER",
            ]);
            for key in [
                "LAZYXRP_CONFIG",
                "LAZYXRP_DATA",
                "XRPL_NETWORK",
                "XRPL_RPC_SERVER",
                "XRPL_WS_SERVER",
            ] {
                env.remove(key);
            }
            env.set(
                "XDG_CONFIG_HOME",
                std::env::temp_dir()
                    .join(format!("lazyxrp-app-test-{}", std::process::id()))
                    .to_str()
                    .expect("temp xdg path"),
            );
            let mut config = Config::new()?;
            // Drop whatever the environment supplied; the test's own seed is the only secret.
            drop(config.take_secrets());
            config
        };
        let secrets = Secrets {
            seed: seed.map(|s| secrecy::SecretString::from(s.to_string())),
            ..Secrets::default()
        };
        App::new(
            4.0,
            Endpoints {
                network: Network::Mainnet,
                rpc_url: "https://xrplcluster.com".into(),
                ws_url: "wss://xrplcluster.com".into(),
            },
            Some("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into()),
            config,
            secrets,
            false,
        )
    }

    /// `q` quits; while the keymap is suppressed (text entry) only Ctrl+C does.
    ///
    /// TC-061: the `q` key (via the keymap) quits.
    ///
    /// Form/modal typing must not treat bare `q` as quit (modal key priority).
    #[tokio::test]
    async fn quit_keys() -> color_eyre::Result<()> {
        // q key quits
        let mut app = test_app().await?;
        assert!(!app.should_quit);
        app.on_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.should_quit);

        // keymap suppressed ignores bare q but allows ctrl c
        let mut app = test_app().await?;
        app.keymap_suppressed = true;

        app.on_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.should_quit);

        app.on_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.should_quit);
        Ok(())
    }

    /// TC-062-063: `r`/`b` send one PollCommand; a second press inside the 500 ms debounce
    /// window enqueues nothing.
    #[tokio::test]
    async fn refresh_keys_send_poll_commands_debounced() -> color_eyre::Result<()> {
        for (key, expected) in [('r', PollCommand::Account), ('b', PollCommand::Book)] {
            let mut app = test_app().await?;
            for _ in 0..2 {
                app.on_key_event(KeyEvent::new(KeyCode::Char(key), KeyModifiers::empty()))?;
                app.drain_and_dispatch_actions(None)?;
            }
            let rx = app.test_poll_rx.as_mut().expect("test receiver");
            assert_eq!(rx.try_recv().expect("first press enqueues"), expected);
            assert!(rx.try_recv().is_err(), "second press must be debounced");
        }
        Ok(())
    }

    /// TC-064: TabNext / TabPrev wrap around all panels
    #[tokio::test]
    async fn tab_next_and_prev_cycle_all_panels() -> color_eyre::Result<()> {
        let mut app = test_app().await?;
        assert_eq!(app.active_tab, 0);
        for i in 1..=TAB_TITLES.len() {
            app.action_tx.send(Action::TabNext)?;
            app.drain_and_dispatch_actions(None)?;
            assert_eq!(app.active_tab, i % TAB_TITLES.len());
        }
        assert_eq!(app.active_tab, 0);
        app.action_tx.send(Action::TabPrev)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(app.active_tab, TAB_TITLES.len() - 1);
        app.action_tx.send(Action::TabPrev)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(app.active_tab, TAB_TITLES.len() - 2);
        Ok(())
    }

    /// TC-065 (HelpOverlay visibility is driven by `show_help` + `Action::Help`);
    /// `?` opens and Esc closes via the config keymap.
    #[tokio::test]
    async fn help_overlay_opens_and_closes_via_keys_and_actions() -> color_eyre::Result<()> {
        let mut app = test_app().await?;
        assert!(!app.show_help);
        app.action_tx.send(Action::Help)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.show_help);
        app.action_tx.send(Action::Help)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.show_help);

        // `?` opens via keybinding; Esc closes when help is shown.
        let q = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty());
        app.on_key_event(q)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.show_help);
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        app.on_key_event(esc)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.show_help);
        Ok(())
    }

    /// TC-115: NetworkSwitchCycle advances to the next network via net_tx.
    #[tokio::test]
    async fn network_switch_cycle_sends_next_network() -> color_eyre::Result<()> {
        let mut app = test_app().await?;
        let net_rx = app.net_tx.subscribe();
        app.action_tx.send(Action::NetworkSwitchCycle)?;
        app.drain_and_dispatch_actions(None)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(*net_rx.borrow(), Network::Testnet);
        assert_eq!(app.network, Network::Testnet);
        assert_eq!(app.status_bar.network_badge, format!(" {} ", "TESTNET"));
        app.action_tx.send(Action::NetworkSwitchCycle)?;
        app.drain_and_dispatch_actions(None)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(*net_rx.borrow(), Network::Devnet);
        assert_eq!(app.network, Network::Devnet);
        Ok(())
    }

    /// TC-256: Assets tab `x` starts an NFTokenBurn for the selected NFT — the NFT tab answers
    /// with `TxComposerOpen` and the app jumps to the Account tab (signing credential only);
    /// other tabs ignore `x`, and an empty NFT list opens nothing.
    #[tokio::test]
    async fn x_on_assets_tab_opens_burn_composer_for_selected_nft() -> color_eyre::Result<()> {
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty());
        let nft = crate::xrpl::NftRow {
            nft_id: "000B013A95F14B0044F78A264E41713C64B5F89242540EE208C3098E00000D65".into(),
            taxon: 0,
            serial: 1,
            transfer_fee: 0,
            uri: String::new(),
            is_mutable: false,
            raw_json: Default::default(),
        };

        let mut app = test_app_seeded(Some(crate::test_support::TEST_SEED)).await?;
        app.action_tx
            .send(Action::XrplAccountNfts(vec![nft.clone()]))?;
        app.drain_and_dispatch_actions(None)?;

        app.active_tab = 2;
        app.on_key_event(x)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(app.active_tab, 2, "x is an Assets-tab key");

        app.active_tab = ASSETS_TAB;
        app.on_key_event(x)?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(app.active_tab, ACCOUNT_TAB);

        let mut empty = test_app_seeded(Some(crate::test_support::TEST_SEED)).await?;
        empty.active_tab = ASSETS_TAB;
        empty.on_key_event(x)?;
        empty.drain_and_dispatch_actions(None)?;
        assert_eq!(
            empty.active_tab, ASSETS_TAB,
            "no NFT selected → nothing opens"
        );
        Ok(())
    }

    /// Funded wallet + seed, handlers registered the way `run()` does, then Overview active.
    async fn seeded_app() -> color_eyre::Result<App> {
        let mut app = test_app_seeded(Some(crate::test_support::TEST_SEED)).await?;
        let tx = app.action_tx.clone();
        for panel in app.panels.iter_mut() {
            panel.register_action_handler(tx.clone())?;
            panel.register_config_handler(Arc::clone(&app.config))?;
        }
        app.action_tx.send(Action::XrplWalletOverview(Some(
            crate::xrpl::AccountSummary {
                account: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
                balance_xrp: "100".into(),
                balance_drops: 100_000_000,
                sequence: 1,
                owner_count: 0,
                flags: 0,
                regular_key: None,
                domain_hex: None,
            },
        )))?;
        app.drain_and_dispatch_actions(None)?;
        Ok(app)
    }

    fn press(app: &mut App, c: char) -> color_eyre::Result<()> {
        app.route_event(&Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::empty(),
        )))?;
        app.drain_and_dispatch_actions(None)
    }

    /// Keys belong to the visible tab: `t` on Overview must not open the (invisible) wallet
    /// composer or suppress the global keymap; on the Account tab it does.
    #[tokio::test]
    async fn overview_t_key_does_not_open_hidden_composer() -> color_eyre::Result<()> {
        let mut app = seeded_app().await?;
        assert_eq!(app.active_tab, 0);
        press(&mut app, 't')?;
        assert!(
            !app.keymap_suppressed,
            "hidden composer must not capture keys"
        );
        let account_tab = crate::test_support::render_to_string(120, 40, |f| {
            app.panels[ACCOUNT_TAB].draw(f, f.area()).unwrap();
        });
        assert!(!account_tab.contains("Choose action"), "{account_tab}");
        press(&mut app, 'q')?;
        assert!(
            app.should_quit,
            "keymap must still work after `t` on Overview"
        );

        let mut app = seeded_app().await?;
        app.active_tab = ACCOUNT_TAB;
        press(&mut app, 't')?;
        assert!(app.keymap_suppressed, "composer opens on the Account tab");
        Ok(())
    }

    /// Tx History `f` reaches the focused pane; filter typing suppresses the keymap.
    #[tokio::test]
    async fn account_tab_filter_key_reaches_tx_history() -> color_eyre::Result<()> {
        let mut app = seeded_app().await?;
        app.active_tab = ACCOUNT_TAB;
        app.action_tx.send(Action::FocusNext)?;
        app.drain_and_dispatch_actions(None)?;
        press(&mut app, 'f')?;
        assert!(app.keymap_suppressed, "filter typing must own the keyboard");
        press(&mut app, 'q')?;
        assert!(!app.should_quit, "`q` is filter text, not quit");
        app.route_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::empty(),
        )))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.keymap_suppressed);
        Ok(())
    }

    /// An unreachable RPC (`server_info` error) ends the splash; unrelated errors do not.
    #[tokio::test]
    async fn startup_ends_when_server_info_errors() -> color_eyre::Result<()> {
        let mut app = test_app().await?;
        assert!(!app.startup_done);
        app.action_tx
            .send(Action::XrplError("account_tx: timeout".into()))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.startup_done, "unrelated errors keep the splash");
        app.action_tx.send(Action::XrplServerInfoError(
            "server_info: connection refused".into(),
        ))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.startup_done, "unreachable RPC must reach the error UI");
        Ok(())
    }

    /// `TxComposerOpen` without a signing credential (--watch) does not jump to Account.
    #[tokio::test]
    async fn composer_open_without_credential_stays_on_tab() -> color_eyre::Result<()> {
        let mut app = test_app().await?;
        app.action_tx.send(Action::TxComposerOpen {
            tx_type: "NFTokenBurn".into(),
            fields: vec![],
        })?;
        app.drain_and_dispatch_actions(None)?;
        assert_eq!(app.active_tab, 0);
        Ok(())
    }

    /// TC-373: the actions the poll task serves are forwarded as their own poll command; the
    /// debounced refreshes and everything else are not (payload-carrying ones are matched to their
    /// command by type at compile time).
    #[test]
    fn poll_command_for_forwards_only_poll_task_actions() {
        assert!(matches!(
            poll_command_for(&Action::RefreshAccountEscrows),
            Some(PollCommand::AccountEscrows)
        ));
        assert!(matches!(
            poll_command_for(&Action::RefreshAccountOffers),
            Some(PollCommand::AccountOffers)
        ));
        assert!(matches!(
            poll_command_for(&Action::GenerateWalletKeys),
            Some(PollCommand::GenerateWalletKeys(key_type)) if key_type == "ed25519"
        ));
        for action in [Action::Tick, Action::RefreshAccount, Action::Help] {
            assert!(poll_command_for(&action).is_none(), "{action:?}");
        }
    }
}
