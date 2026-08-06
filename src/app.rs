use secrecy::ExposeSecret;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Tabs},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

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
    config::Config,
    flare::{DEFAULT_FLARE_FEEDS, DEFAULT_FLARE_RPC},
    network::Network,
    tui::{Event, Tui},
    xrpl::{
        BookPair, PollCommand, PollContext, fetch_xrpl_toml_with_meta, start_poll_task,
        start_ws_task,
    },
};

/// Tab labels (index mirrors `panels` Vec order)
const TAB_TITLES: &[&str] = &["󰖟 Overview", "󰀉 Account", "󰠿 Market", "󰒍 Assets"];

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
            hint("t", ":tx");
            hint("g", ":keygen");
            hint("r", ":refresh");
            hint("Enter", ":dUNL");
        }
        1 => {
            hint("t", ":tx");
            hint("f", ":filter");
            hint("r", ":refresh");
        }
        2 => hint("b", ":book"),
        3 => hint("o", ":objects"),
        _ => {}
    }
    hint("q", ":quit");
    Line::from(spans)
}

pub struct App {
    config: Arc<Config>,
    /// Wallet AccountSet form typing mode: skip Splash keymap in `on_key_event`.
    keymap_suppressed: bool,
    tick_rate: f64,
    frame_rate: f64,
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
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    poll_tx: mpsc::UnboundedSender<PollCommand>,
    /// Populated in `App::new` for unit tests; cleared when `run` starts a fresh channel.
    test_poll_rx: Option<mpsc::UnboundedReceiver<PollCommand>>,
    rpc_server: String,
    ws_server: String,
    watch_account: String,
    /// Watch sender — future: dynamic network switching from the UI
    net_tx: watch::Sender<Network>,
    tab_tx: watch::Sender<usize>,
    needs_draw: bool,
}

fn resolve_flare_rpc_url() -> Option<String> {
    std::env::var("FLARE_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some(DEFAULT_FLARE_RPC.to_string()))
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

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Splash,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tick_rate: f64,
        frame_rate: f64,
        rpc_server: String,
        ws_server: String,
        account: Option<String>,
        network: Network,
        mut config: Config,
        seed: Option<String>,
        skip_mainnet_prompt: bool,
    ) -> color_eyre::Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (poll_tx, poll_rx) = mpsc::unbounded_channel();
        let test_poll_rx = if cfg!(test) {
            Some(poll_rx)
        } else {
            drop(poll_rx);
            None
        };
        let (net_tx, _net_rx) = watch::channel(network);
        let (tab_tx, _tab_rx) = watch::channel(0usize);
        if let Some(cli_seed) = seed {
            let trimmed_seed = crate::signing::trim_family_seed(&cli_seed);
            config.xrpl.signing.seed = None;
            config.xrpl.signing.secret_seed = if trimmed_seed.is_empty() {
                None
            } else {
                Some(secrecy::SecretString::from(trimmed_seed.to_string()))
            };
        }
        // Keep wallet UI / config network aligned with the resolved CLI/env network.
        config.xrpl.network = network;
        let watch_account = account.unwrap_or_else(|| config.xrpl.account.clone());
        let panels: Vec<Box<dyn Component>> = vec![
            Box::new(OverviewTab::new(rpc_server.clone())),
            Box::new(AccountWalletTab::new(skip_mainnet_prompt)),
            Box::new(MarketOracleTab::new()),
            Box::new(AssetsTab::new()),
        ];
        // UA-1: guard against tab/panel index mismatch (docs/agent/INVARIANTS.md)
        debug_assert_eq!(
            TAB_TITLES.len(),
            panels.len(),
            "TAB_TITLES and panels must have same length"
        );
        Ok(Self {
            keymap_suppressed: false,
            tick_rate,
            frame_rate,
            panels,
            status_bar: StatusBar::new(watch_account.clone(), network),
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
            mode: Mode::Splash,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
            poll_tx,
            test_poll_rx,
            rpc_server,
            ws_server,
            watch_account,
            net_tx,
            tab_tx,
            needs_draw: true,
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let mut tui = Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        tui.enter()?;

        let action_tx = self.action_tx.clone();
        for panel in self.panels.iter_mut() {
            panel.register_action_handler(action_tx.clone())?;
            panel.register_config_handler(Arc::clone(&self.config))?;
            panel.init(tui.size()?)?;
        }
        self.status_bar.register_action_handler(action_tx.clone())?;
        self.status_bar
            .register_config_handler(Arc::clone(&self.config))?;
        self.status_bar.init(tui.size()?)?;
        self.splash.register_action_handler(action_tx.clone())?;
        self.splash
            .register_config_handler(Arc::clone(&self.config))?;
        self.splash.init(tui.size()?)?;

        let (poll_tx, poll_rx) = mpsc::unbounded_channel();
        let (poll_trigger_tx, poll_trigger_rx) = mpsc::unbounded_channel();
        self.poll_tx = poll_tx;
        drop(self.test_poll_rx.take());
        let cancel = CancellationToken::new();

        let book_pair = BookPair {
            base: "XRP".to_string(),
            quote: self.config.xrpl.currency.clone(),
            quote_code: self.config.xrpl.currency_code.clone(),
            issuer: if self.config.xrpl.issuer.trim().is_empty() {
                crate::config::FALLBACK_ISSUER.to_string()
            } else {
                self.config.xrpl.issuer.clone()
            },
            limit: self.config.xrpl.offer_limit,
        };
        start_ws_task(
            self.ws_server.clone(),
            Some(self.watch_account.clone()),
            action_tx.clone(),
            poll_trigger_tx,
            cancel.clone(),
        );
        let seed_address = self
            .config
            .xrpl
            .signing
            .secret_seed
            .as_ref()
            .map(|s| crate::signing::seed_to_address(s.expose_secret()))
            .and_then(Result::ok);
        let flare_rpc_url = resolve_flare_rpc_url();
        let flare_feeds = resolve_flare_feeds();
        start_poll_task(
            PollContext {
                rpc_url: self.rpc_server.clone(),
                watch_address: self.watch_account.clone(),
                book_pair,
                poll_interval: Duration::from_millis(self.config.xrpl.poll_interval_ms),
                seed_address,
                signing_seed: self.config.xrpl.signing.secret_seed.clone(),
                network_watch: self.net_tx.subscribe(),
                tab_watch: self.tab_tx.subscribe(),
                oracles: self.config.xrpl.oracles.clone(),
                oracle_pairs: self.config.xrpl.oracle_pairs.clone(),
                flare_rpc_url: flare_rpc_url.clone(),
                flare_feeds: flare_feeds.clone(),
                flare_fassets_execute: self.config.flare.fassets.execute,
                flare_evm_key_env: self.config.flare.fassets.evm_key_env.clone(),
            },
            poll_rx,
            poll_trigger_rx,
            action_tx.clone(),
            cancel.clone(),
        );
        if self.config.xrpl.oracles.is_empty()
            && flare_rpc_url.is_none()
            && action_tx.send(Action::XrplOracleNotConfigured).is_err()
        {
            warn!("action channel closed (oracle not configured)");
        }

        loop {
            self.forward_tui_events(&mut tui).await?;
            self.drain_and_dispatch_actions(Some(&mut tui))?;
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
        let action_tx = self.action_tx.clone();
        match &event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(*x, *y))?,
            Event::Key(key) => self.on_key_event(*key)?,
            _ => {}
        }
        for panel in self.panels.iter_mut() {
            if let Some(action) = panel.on_event(Some(&event))? {
                action_tx.send(action)?;
            }
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

        // Number keys jump to tabs 1..=TAB_TITLES.len()
        if let KeyCode::Char(c) = key.code
            && c.is_ascii_digit()
            && let Some(d) = c.to_digit(10)
            && d >= 1
            && (d as usize) <= TAB_TITLES.len()
        {
            action_tx.send(Action::TabJump(d as usize - 1))?;
            return Ok(());
        }

        let Some(keymap) = self.config.keybindings.0.get(&self.mode) else {
            return Ok(());
        };
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

    /// Send a poll command only if at least `min` duration has passed since the last one.
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
            if !matches!(
                &action,
                Action::Tick | Action::Render | Action::NftImageLoaded { .. }
            ) {
                debug!("{action:?}");
            }
            // Dirty policy (ratatui plan Phase 3):
            // keys/resize/data/UI => always; Tick => splash only here;
            // FPS label refresh marks dirty after fps.note_action; Render never marks.
            match &action {
                Action::Render => {}
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
            match &action {
                Action::Tick => {}
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => {
                    if let Some(tui) = tui.as_deref_mut() {
                        tui.terminal.clear()?;
                    }
                }
                Action::Resize(w, h) => {
                    if let Some(tui) = tui.as_deref_mut() {
                        self.on_resize(tui, *w, *h)?;
                    }
                }
                Action::Render if self.needs_draw => {
                    if let Some(tui) = tui.as_deref_mut() {
                        self.render(tui)?;
                        self.needs_draw = false;
                        // Count actual draws for the FPS label (not raw Render events).
                        let _ = self.fps.note_action(&Action::Render);
                    }
                }
                Action::Render => {}
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
                    if let Err(err) = self.net_tx.send(*net) {
                        warn!(?err, "network watch channel closed");
                    }
                }
                Action::NftImageRequest { nft_id, uri } => {
                    let action_tx = self.action_tx.clone();
                    let nft_id = nft_id.clone();
                    let uri = uri.clone();
                    tokio::spawn(async move {
                        match crate::xrpl::fetch_nft_image(&uri).await {
                            Ok(image) => {
                                if let Err(err) = action_tx.send(Action::NftImageLoaded {
                                    nft_id,
                                    bytes: image.bytes,
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
                Action::RequestXrplToml {
                    domain,
                    expected_pubkey,
                } => {
                    let domain = domain.clone();
                    let expected_pubkey = expected_pubkey.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        let fetched = fetch_xrpl_toml_with_meta(
                            &domain,
                            &expected_pubkey,
                            Duration::from_secs(10),
                        )
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
                Action::AccountSetSubmit(params) => {
                    self.send_poll(PollCommand::AccountSetSubmit(params.clone()))
                }
                Action::PaymentSubmit(params) => {
                    self.send_poll(PollCommand::PaymentSubmit(params.clone()))
                }
                Action::SetRegularKeySubmit(params) => {
                    self.send_poll(PollCommand::SetRegularKeySubmit(params.clone()))
                }
                Action::EscrowCreateSubmit(params) => {
                    self.send_poll(PollCommand::EscrowCreateSubmit(params.clone()))
                }
                Action::OfferCreateSubmit(params) => {
                    self.send_poll(PollCommand::OfferCreateSubmit(params.clone()))
                }
                Action::TrustSetSubmit(params) => {
                    self.send_poll(PollCommand::TrustSetSubmit(params.clone()))
                }
                Action::FxrpDirectMintPaymentSubmit(params) => {
                    self.send_poll(PollCommand::FxrpDirectMintPayment(params.clone()))
                }
                Action::FxrpExecuteDirectMintSubmit(params) => {
                    self.send_poll(PollCommand::FxrpExecuteDirectMint(params.clone()))
                }
                Action::WalletPropose => {
                    self.send_poll(PollCommand::WalletPropose("ed25519".into()))
                }
                Action::XrplServerInfo(_) => self.startup_done = true,
                Action::Help => self.show_help = !self.show_help,
                _ => {}
            }
            for panel in self.panels.iter_mut() {
                if let Some(a) = panel.update(&action)? {
                    self.action_tx.send(a)?;
                }
            }
            if let Some(a) = self.status_bar.update(&action)? {
                self.action_tx.send(a)?;
            }
            // FPS tick-rate label: dirty at most once/sec when the text changes.
            if matches!(action, Action::Tick) && self.fps.note_action(&action) {
                self.needs_draw = true;
            }
            if !self.startup_done {
                self.splash.update(&action)?;
            }
        }
        Ok(())
    }

    fn on_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> color_eyre::Result<()> {
        tui.resize(Rect::new(0, 0, w, h))?;
        self.render(tui)?;
        let _ = self.fps.note_action(&Action::Render);
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        if !self.startup_done {
            let splash = &mut self.splash;
            let action_tx = &self.action_tx;
            tui.draw(|frame| {
                if let Err(err) = splash.draw(frame, frame.area())
                    && let Err(e) =
                        action_tx.send(Action::Error(format!("Failed to draw: {err:?}")))
                {
                    warn!(?e, "action channel closed while reporting draw error");
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
        let action_tx = &self.action_tx;

        tui.draw(|frame| {
            let [tabs_area, main_area, hints_area, status_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(frame.area());

            // Tabs — numbered, accent highlight, dim divider
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

            // Footer key hints (above status bar)
            let hints = footer_line(active_tab);
            frame.render_widget(Paragraph::new(hints).style(theme::dim_style()), hints_area);

            // Active panel
            if let Some(panel) = panels.get_mut(active_tab)
                && let Err(err) = panel.draw(frame, main_area)
                && let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw panel: {err:?}")))
            {
                warn!(?e, "action channel closed while reporting panel draw error");
            }

            // FPS counter (top-right corner of main area)
            let fps_area = Rect::new(main_area.right().saturating_sub(10), main_area.top(), 10, 1);
            if let Err(err) = fps.draw(frame, fps_area)
                && let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw fps: {err:?}")))
            {
                warn!(?e, "action channel closed while reporting fps draw error");
            }

            // StatusBar
            if let Err(err) = status_bar.draw(frame, status_area)
                && let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw status: {err:?}")))
            {
                warn!(
                    ?e,
                    "action channel closed while reporting status draw error"
                );
            }

            // Help overlay
            if show_help
                && let Err(err) = help.draw(frame, frame.area())
                && let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw help: {err:?}")))
            {
                warn!(?e, "action channel closed while reporting help draw error");
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

    fn test_app() -> color_eyre::Result<App> {
        let config = Config::new()?;
        App::new(
            4.0,
            60.0,
            "https://xrplcluster.com".into(),
            "wss://xrplcluster.com".into(),
            Some("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into()),
            Network::Mainnet,
            config,
            None,
            false,
        )
    }

    /// TC-060: App constructs with one panel per tab (I-9)
    #[test]
    fn watch_app_new_builds_four_tabs() -> color_eyre::Result<()> {
        let app = test_app()?;
        assert_eq!(TAB_TITLES.len(), 4, "product currently ships 4 tabs");
        assert_eq!(app.panels.len(), 4);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.watch_account, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh");
        assert!(!app.should_quit);
        assert!(!app.show_help);
        Ok(())
    }

    /// TC-061
    #[tokio::test]
    async fn quit_action_sets_should_quit() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.action_tx.send(Action::Quit)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.should_quit);
        Ok(())
    }

    /// TC-062
    #[tokio::test]
    async fn refresh_account_sends_poll_command() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.action_tx.send(Action::RefreshAccount)?;
        app.drain_and_dispatch_actions(None)?;
        let cmd = app
            .test_poll_rx
            .as_mut()
            .expect("test receiver")
            .try_recv()
            .expect("RefreshAccount should enqueue a poll command");
        assert_eq!(cmd, PollCommand::Account);
        Ok(())
    }

    /// TC-063
    #[tokio::test]
    async fn refresh_book_sends_poll_command() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.action_tx.send(Action::RefreshBook)?;
        app.drain_and_dispatch_actions(None)?;
        let cmd = app
            .test_poll_rx
            .as_mut()
            .expect("test receiver")
            .try_recv()
            .expect("RefreshBook should enqueue a poll command");
        assert_eq!(cmd, PollCommand::Book);
        Ok(())
    }

    /// TC-064: TabNext / TabPrev wrap around all panels
    #[tokio::test]
    async fn tab_next_and_prev_cycle_all_panels() -> color_eyre::Result<()> {
        let mut app = test_app()?;
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

    /// TC-065 (HelpOverlay visibility is driven by `show_help` + `Action::Help`)
    #[tokio::test]
    async fn help_action_toggles_overlay_flag() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        assert!(!app.show_help);
        app.action_tx.send(Action::Help)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.show_help);
        app.action_tx.send(Action::Help)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.show_help);
        Ok(())
    }

    /// Esc closes overlay when help is shown (mirrors `on_key_event` + config keymap)
    #[tokio::test]
    async fn esc_while_help_sends_help_action() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.show_help = true;
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        app.on_key_event(esc)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.show_help);
        Ok(())
    }

    /// `?` opens help via keybindings
    #[tokio::test]
    async fn question_opens_help_overlay() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let q = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty());
        app.on_key_event(q)?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.show_help);
        Ok(())
    }

    /// Form/modal typing must not treat bare `q` as quit (modal key priority).
    #[tokio::test]
    async fn keymap_suppressed_ignores_bare_q_but_allows_ctrl_c() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.keymap_suppressed = true;

        app.on_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(!app.should_quit);

        app.on_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))?;
        app.drain_and_dispatch_actions(None)?;
        assert!(app.should_quit);
        Ok(())
    }
}
