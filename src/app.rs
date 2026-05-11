use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
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
        panels::nft::NftPanel,
        shared::{
            fps::FpsCounter, help_overlay::HelpOverlay, splash::SplashScreen,
            status_bar::StatusBar, theme,
        },
        tabs::{
            account_objects::AccountObjectsTab, account_tx::AccountTxTab, market::MarketTab,
            server_overview::ServerOverviewTab,
        },
    },
    config::Config,
    network::Network,
    tui::{Event, Tui},
    xrpl::{BookPair, PollCommand, PollContext, start_poll_task, start_ws_task},
};

/// Tab labels (index mirrors `panels` Vec order)
const TAB_TITLES: &[&str] = &["󰖟 Overview", "󰀉 Account", "󰠿 Market", "󰒍 NFTs", "󰧮 Objects"];

fn footer_hints(active_tab: usize) -> Line<'static> {
    let key = |k: &'static str| Span::styled(k, Style::new().add_modifier(Modifier::BOLD));
    let sep = || Span::raw("  ");
    let label = |s: &'static str| Span::raw(s);
    let mut spans = vec![
        key("?"),
        label(":help"),
        sep(),
        key("Tab"),
        label(":next"),
        sep(),
        key("1-5"),
        label(":jump"),
        sep(),
        key("↑↓/jk"),
        label(":row"),
        sep(),
        key("hl/←→"),
        label(":focus"),
        sep(),
        key("^Z"),
        label(":suspend"),
        sep(),
    ];
    // Tab indices: 0 Overview, 1 Account+Tx, 2 Market, 3 NFTs, 4 Objects (misc + pay + escrow)
    match active_tab {
        0 => {
            spans.extend([
                key("t"),
                label(":tx composer"),
                sep(),
                key("e/s"),
                label(":in modal"),
                sep(),
            ]);
        }
        1 => {
            spans.extend([key("r"), label(":refresh"), sep()]);
        }
        2 => {
            spans.extend([key("b"), label(":refresh book"), sep()]);
        }
        4 => {
            spans.extend([key("o"), label(":obj refresh"), sep()]);
        }
        _ => {}
    }
    spans.extend([key("q"), label(":quit")]);
    Line::from(spans)
}

pub struct App {
    config: Arc<Config>,
    /// Wallet AccountSet form typing mode: skip Splash keymap in `on_key_event`.
    keymap_suppress: bool,
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
    needs_draw: bool,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Splash,
}

impl App {
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
        let (net_tx, _net_rx) = watch::channel(network.clone());
        if let Some(cli_seed) = seed {
            let t = crate::signing::trim_family_seed(&cli_seed);
            config.xrpl.signing.seed = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            };
        }
        let watch_account = account.unwrap_or_else(|| config.xrpl.account.clone());
        Ok(Self {
            keymap_suppress: false,
            tick_rate,
            frame_rate,
            panels: vec![
                Box::new(ServerOverviewTab::new(
                    rpc_server.clone(),
                    skip_mainnet_prompt,
                )),
                Box::new(AccountTxTab::new()),
                Box::new(MarketTab::new()),
                Box::new(NftPanel::new()),
                Box::new(AccountObjectsTab::new()),
            ],
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
            .seed
            .as_ref()
            .and_then(|s| crate::components::panels::wallet::seed_to_address(s).ok());
        start_poll_task(
            PollContext {
                rpc_url: self.rpc_server.clone(),
                watch_address: self.watch_account.clone(),
                book_pair,
                poll_interval: Duration::from_millis(self.config.xrpl.poll_interval_ms),
                seed_address,
                network_watch: self.net_tx.subscribe(),
            },
            poll_rx,
            poll_trigger_rx,
            action_tx.clone(),
            cancel.clone(),
        );

        loop {
            self.handle_events(&mut tui).await?;
            self.process_actions(&mut tui)?;
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

    async fn handle_events(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
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
            if let Some(action) = panel.handle_events(Some(&event))? {
                action_tx.send(action)?;
            }
        }
        Ok(())
    }

    fn on_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        let action_tx = self.action_tx.clone();
        if self.keymap_suppress {
            match key.code {
                KeyCode::Char('q') => action_tx.send(Action::Quit)?,
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
        {
            if let Some(d) = c.to_digit(10)
                && d >= 1
                && (d as usize) <= TAB_TITLES.len()
            {
                action_tx.send(Action::TabJump(d as usize - 1))?;
                return Ok(());
            }
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

    /// Send a poll command only if at least `min` duration has passed since the last one.
    fn try_debounced(
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

    fn process_actions(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }
            if action != Action::Render {
                self.needs_draw = true;
            }
            match &action {
                Action::Tick => {
                    self.last_tick_key_events.drain(..);
                }
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.clear()?,
                Action::Resize(w, h) => self.on_resize(tui, *w, *h)?,
                Action::Render => {
                    if self.needs_draw {
                        self.render(tui)?;
                        self.needs_draw = false;
                    }
                }
                Action::TabNext => {
                    self.active_tab = (self.active_tab + 1) % TAB_TITLES.len();
                }
                Action::TabPrev => {
                    self.active_tab = if self.active_tab == 0 {
                        TAB_TITLES.len() - 1
                    } else {
                        self.active_tab - 1
                    };
                }
                Action::TabJump(i) => {
                    if *i < TAB_TITLES.len() {
                        self.active_tab = *i;
                    }
                }
                Action::NetworkChange(net) => {
                    if let Err(err) = self.net_tx.send(net.clone()) {
                        warn!(?err, "network watch channel closed");
                    }
                }
                Action::RefreshAccount => {
                    Self::try_debounced(
                        &mut self.last_refresh_account,
                        &self.poll_tx,
                        PollCommand::Account,
                    );
                }
                Action::RefreshBook => {
                    Self::try_debounced(
                        &mut self.last_refresh_book,
                        &self.poll_tx,
                        PollCommand::Book,
                    );
                }
                Action::RefreshNfts => {
                    if let Err(err) = self.poll_tx.send(PollCommand::Nfts) {
                        warn!(?err, "poll command channel closed");
                    }
                }
                Action::RefreshLines => {
                    if let Err(err) = self.poll_tx.send(PollCommand::Lines) {
                        warn!(?err, "poll command channel closed");
                    }
                }
                Action::RefreshTxHistory => {
                    if let Err(err) = self.poll_tx.send(PollCommand::TxHistory) {
                        warn!(?err, "poll command channel closed");
                    }
                }
                Action::RefreshLedgerObjects => {
                    Self::try_debounced(
                        &mut self.last_refresh_ledger_objects,
                        &self.poll_tx,
                        PollCommand::LedgerObjects,
                    );
                }
                Action::SetKeymapSuppression(on) => {
                    self.keymap_suppress = *on;
                }
                Action::AccountSetSubmit(params) => {
                    if let Err(err) = self
                        .poll_tx
                        .send(PollCommand::AccountSetSubmit(params.clone()))
                    {
                        warn!(?err, "poll command channel closed");
                    }
                }
                Action::PaymentSubmit(params) => {
                    if let Err(err) = self
                        .poll_tx
                        .send(PollCommand::PaymentSubmit(params.clone()))
                    {
                        warn!(?err, "poll command channel closed");
                    }
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
            if let Some(a) = self.fps.update(&action)? {
                self.action_tx.send(a)?;
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
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        if !self.startup_done {
            let splash = &mut self.splash;
            let action_tx = &self.action_tx;
            tui.draw(|frame| {
                if let Err(err) = splash.draw(frame, frame.area()) {
                    if let Err(e) =
                        action_tx.send(Action::Error(format!("Failed to draw: {err:?}")))
                    {
                        warn!(?e, "action channel closed while reporting draw error");
                    }
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
                .highlight_style(
                    Style::new()
                        .fg(theme::HIGHLIGHT_BG)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )
                .block(Block::default());
            frame.render_widget(tabs, tabs_area);

            // Footer key hints (above status bar)
            let hints = footer_hints(active_tab);
            frame.render_widget(Paragraph::new(hints).style(theme::dim_style()), hints_area);

            // Active panel
            if let Some(panel) = panels.get_mut(active_tab)
                && let Err(err) = panel.draw(frame, main_area)
            {
                if let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw panel: {err:?}")))
                {
                    warn!(?e, "action channel closed while reporting panel draw error");
                }
            }

            // FPS counter (top-right corner of main area)
            let fps_area = Rect::new(main_area.right().saturating_sub(10), main_area.top(), 10, 1);
            if let Err(err) = fps.draw(frame, fps_area) {
                if let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw fps: {err:?}")))
                {
                    warn!(?e, "action channel closed while reporting fps draw error");
                }
            }

            // StatusBar
            if let Err(err) = status_bar.draw(frame, status_area) {
                if let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw status: {err:?}")))
                {
                    warn!(
                        ?e,
                        "action channel closed while reporting status draw error"
                    );
                }
            }

            // Help overlay
            if show_help && let Err(err) = help.draw(frame, frame.area()) {
                if let Err(e) =
                    action_tx.send(Action::Error(format!("Failed to draw help: {err:?}")))
                {
                    warn!(?e, "action channel closed while reporting help draw error");
                }
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::{config::Config, network::Network, tui::Tui};

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

    /// TC-060
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn watch_app_new_does_not_panic() -> color_eyre::Result<()> {
        let app = test_app()?;
        assert_eq!(app.panels.len(), TAB_TITLES.len());
        Ok(())
    }

    /// TC-061
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn quit_action_sets_should_quit() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let mut tui = Tui::new()?;
        app.action_tx.send(Action::Quit)?;
        app.process_actions(&mut tui)?;
        assert!(app.should_quit);
        Ok(())
    }

    /// TC-062
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn refresh_account_sends_poll_command() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let mut tui = Tui::new()?;
        app.action_tx.send(Action::RefreshAccount)?;
        app.process_actions(&mut tui)?;
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
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn refresh_book_sends_poll_command() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let mut tui = Tui::new()?;
        app.action_tx.send(Action::RefreshBook)?;
        app.process_actions(&mut tui)?;
        let cmd = app
            .test_poll_rx
            .as_mut()
            .expect("test receiver")
            .try_recv()
            .expect("RefreshBook should enqueue a poll command");
        assert_eq!(cmd, PollCommand::Book);
        Ok(())
    }

    /// TC-064
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn tab_next_cycles_all_panels() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let mut tui = Tui::new()?;
        assert_eq!(app.active_tab, 0);
        for i in 1..=TAB_TITLES.len() {
            app.action_tx.send(Action::TabNext)?;
            app.process_actions(&mut tui)?;
            assert_eq!(app.active_tab, i % TAB_TITLES.len());
        }
        Ok(())
    }

    /// TC-065 (HelpOverlay visibility is driven by `show_help` + `Action::Help`)
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn help_action_toggles_overlay_flag() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let mut tui = Tui::new()?;
        assert!(!app.show_help);
        app.action_tx.send(Action::Help)?;
        app.process_actions(&mut tui)?;
        assert!(app.show_help);
        app.action_tx.send(Action::Help)?;
        app.process_actions(&mut tui)?;
        assert!(!app.show_help);
        Ok(())
    }

    /// Esc closes overlay when help is shown (mirrors `on_key_event` + config keymap)
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn esc_while_help_sends_help_action() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        app.show_help = true;
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        app.on_key_event(esc)?;
        let mut tui = Tui::new()?;
        app.process_actions(&mut tui)?;
        assert!(!app.show_help);
        Ok(())
    }

    /// `?` opens help via keybindings
    #[test]
    #[ignore = "requires interactive TTY and tokio runtime"]
    fn question_opens_help_overlay() -> color_eyre::Result<()> {
        let mut app = test_app()?;
        let q = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty());
        app.on_key_event(q)?;
        let mut tui = Tui::new()?;
        app.process_actions(&mut tui)?;
        assert!(app.show_help);
        Ok(())
    }
}
