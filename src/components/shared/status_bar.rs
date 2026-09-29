use std::time::{Duration, Instant, SystemTime};

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{fmt, theme},
    },
    network::Network,
    xrpl::{BookMidPrice, DUNL_CACHE_TTL},
};

/// XRPL dot degrades to STALE after this long without a server_info update —
/// quiet backoff windows (no polls, no errors) must not stay green.
const XRPL_STALE_AFTER: Duration = Duration::from_secs(30);

/// Flare dot degrades to STALE after this long without a flare fetch success;
/// flare only polls on the Overview tab, so other tabs age it by design.
const FLARE_STALE_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Online,
    Offline,
    Connecting,
    Stale,
}

impl ConnectionState {
    fn label(self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::Offline => "OFFLINE",
            Self::Connecting => "CONNECTING",
            Self::Stale => "STALE",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Online | Self::Stale => "●",
            Self::Offline => "✖",
            Self::Connecting => "○",
        }
    }

    fn color(self) -> ratatui::style::Color {
        match self {
            Self::Online => theme::SUCCESS,
            Self::Offline => theme::ERROR,
            Self::Connecting | Self::Stale => theme::WARNING,
        }
    }
}

pub struct StatusBar {
    account_short: String,
    network: Network,
    pub(crate) network_badge: String,
    last_server_update: Option<Instant>,
    last_account_update: Option<Instant>,
    last_book_update: Option<Instant>,
    flare_enabled: bool,
    last_flare_update: Option<Instant>,
    flare_down: bool,
    last_any_update_wall: Option<SystemTime>,
    cached_wall_time: Option<String>,
    cached_wall_for: Option<SystemTime>,
    last_error: Option<String>,
    cached_error_display: Option<String>,
    refreshing_account: bool,
    refreshing_book: bool,
    tick: usize,
    price: Option<BookMidPrice>,
    cached_price_spans: Vec<Span<'static>>,
    // freshness caches: (last_elapsed_secs, formatted_string)
    freshness_srv: Option<(u64, String)>,
    freshness_acc: Option<(u64, String)>,
    freshness_book: Option<(u64, String)>,
    freshness_flr: Option<(u64, String)>,
    /// `SystemTime` of the dUNL manifest data itself (from `DunlSummary`), so
    /// the age label stays truthful while stale-on-error re-serves the cache.
    dunl_data_wall: Option<SystemTime>,
    // state cache
    cached_state_display: Option<String>,
}

impl StatusBar {
    pub fn new(account: String, network: Network, flare_enabled: bool) -> Self {
        let account_short = fmt::short_hex(&account, 6, 4);
        let network_badge = format!(" {} ", network.display_name());
        Self {
            account_short,
            network,
            network_badge,
            last_server_update: None,
            last_account_update: None,
            last_book_update: None,
            last_flare_update: None,
            flare_down: false,
            flare_enabled,
            freshness_flr: None,
            dunl_data_wall: None,
            last_any_update_wall: None,
            cached_wall_time: None,
            cached_wall_for: None,
            last_error: None,
            cached_error_display: None,
            refreshing_account: false,
            refreshing_book: false,
            tick: 0,
            price: None,
            cached_price_spans: Vec::new(),
            freshness_srv: None,
            freshness_acc: None,
            freshness_book: None,
            cached_state_display: None,
        }
    }

    fn cached_freshness_label(cache: &mut Option<(u64, String)>, latest: Option<Instant>) -> &str {
        match latest {
            Some(t) => {
                let secs = t.elapsed().as_secs();
                if cache
                    .as_ref()
                    .is_none_or(|(cached_secs, _)| *cached_secs != secs)
                {
                    *cache = Some((secs, freshness_label(secs)));
                }
                cache
                    .as_ref()
                    .expect("freshness cache populated")
                    .1
                    .as_str()
            }
            None => {
                *cache = None;
                "-"
            }
        }
    }

    fn connection_state(&self) -> ConnectionState {
        // server_info polls every tick (never tab-gated), so an RPC outage
        // reliably surfaces as a server_info error; other XrplError labels
        // (dUNL, path_find, book) are fetch-specific, not connectivity.
        if self
            .last_error
            .as_deref()
            .is_some_and(|e| e.contains("server_info"))
        {
            ConnectionState::Offline
        } else if let Some(t) = self.last_server_update {
            if t.elapsed() > XRPL_STALE_AFTER {
                ConnectionState::Stale
            } else {
                ConnectionState::Online
            }
        } else {
            ConnectionState::Connecting
        }
    }

    /// Flare health dot state; `None` renders nothing (flare disabled).
    fn flare_state(&self) -> Option<ConnectionState> {
        if !self.flare_enabled {
            return None;
        }
        if self.flare_down {
            return Some(ConnectionState::Offline);
        }
        Some(match self.last_flare_update {
            Some(t) if t.elapsed() <= FLARE_STALE_AFTER => ConnectionState::Online,
            Some(_) => ConnectionState::Stale,
            None => ConnectionState::Connecting,
        })
    }
}

impl Component for StatusBar {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => {
                self.tick = self.tick.wrapping_add(1);
                // Staleness transitions (Online→Stale, flare aging) happen
                // without any action; force the state chip to re-render.
                self.cached_state_display = None;
            }
            Action::XrplServerInfo(_) => {
                self.last_server_update = Some(Instant::now());
                self.last_any_update_wall = Some(SystemTime::now());
                self.last_error = None;
                self.cached_error_display = None;
                self.cached_state_display = None;
            }
            Action::XrplAccount(_) => {
                self.last_account_update = Some(Instant::now());
                self.last_any_update_wall = Some(SystemTime::now());
                self.refreshing_account = false;
            }
            Action::XrplBookOffers(_) => {
                self.last_book_update = Some(Instant::now());
                self.last_any_update_wall = Some(SystemTime::now());
                self.refreshing_book = false;
            }
            Action::BookMidPrice(p) => {
                self.price = Some(p.clone());
                self.last_any_update_wall = Some(SystemTime::now());
                self.last_error = None;
                self.cached_error_display = None;
                self.cached_state_display = None;
                // Precompute price spans
                self.cached_price_spans = vec![
                    Span::raw("  "),
                    Span::styled("XRP/RLUSD", theme::dim_style()),
                    Span::raw(" "),
                    Span::styled(format!("M:{}", p.mid), theme::accent_style()),
                    Span::raw(" "),
                    Span::styled(format!("B:{}", p.bid), theme::success_style()),
                    Span::raw(" "),
                    Span::styled(format!("A:{}", p.ask), theme::error_style()),
                ];
            }
            Action::XrplError(msg) => {
                self.last_error = Some(msg.to_string());
                self.cached_error_display = Some(format!("err:{msg}"));
                self.cached_state_display = None;
            }
            Action::NetworkChange(net) => {
                self.network = *net;
                self.network_badge = format!(" {} ", (*net).display_name());
            }
            Action::XrplDunl(summary) => {
                self.dunl_data_wall = Some(summary.fetched_at);
            }
            Action::FlareOraclePrices(_)
            | Action::FxrpDirectMintInfo(_)
            | Action::FlareWalletBalance(_) => {
                self.last_flare_update = Some(Instant::now());
                self.flare_down = false;
            }
            Action::FlareFetchFailed => {
                self.flare_down = true;
                self.cached_state_display = None;
            }
            Action::RefreshAccount => self.refreshing_account = true,
            Action::RefreshBook => self.refreshing_book = true,
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let state = self.connection_state();
        let state_style = Style::new().fg(state.color()).bold().reversed();
        let label_style = theme::dim_style();
        // Hoist: flare_state() needs an immutable self borrow, which conflicts
        // with the mutable cached_state_display borrow held by `spans` below.
        let flr_state = self.flare_state();
        // Cache state display string
        let state_display = self
            .cached_state_display
            .get_or_insert_with(|| format!(" {} {} ", state.icon(), state.label()));
        let mut spans = vec![
            Span::styled(state_display.as_str(), state_style),
            Span::raw(" "),
            Span::styled("acct:", label_style),
            Span::styled(self.account_short.as_str(), theme::accent_style()),
            Span::raw("  "),
            Span::styled("srv:", label_style),
            Span::raw(Self::cached_freshness_label(
                &mut self.freshness_srv,
                self.last_server_update,
            )),
            Span::raw("  "),
            Span::styled("acc:", label_style),
            Span::raw(Self::cached_freshness_label(
                &mut self.freshness_acc,
                self.last_account_update,
            )),
            Span::raw("  "),
            Span::styled("bk:", label_style),
            Span::raw(Self::cached_freshness_label(
                &mut self.freshness_book,
                self.last_book_update,
            )),
        ];
        if let Some(flr_state) = flr_state {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("flr:", label_style));
            spans.push(Span::styled(
                flr_state.icon(),
                Style::new().fg(flr_state.color()),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::raw(Self::cached_freshness_label(
                &mut self.freshness_flr,
                self.last_flare_update,
            )));
        }
        if let Some(data_wall) = self.dunl_data_wall {
            // dUNL data age: the manifest itself, not the last receive (the
            // poll task re-sends the cached summary on every tick).
            let age_secs = SystemTime::now()
                .duration_since(data_wall)
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);
            let stale = age_secs > DUNL_CACHE_TTL.as_secs();
            let style = if stale {
                Style::new().fg(theme::WARNING)
            } else {
                theme::dim_style()
            };
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!(
                    "dunl:{}{}",
                    freshness_label(age_secs),
                    if stale { "!" } else { "" }
                ),
                style,
            ));
        }
        if let Some(t) = self.last_any_update_wall {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("@", label_style));
            // Cache wall-time string until last_any_update_wall changes
            if self.cached_wall_for != Some(t) {
                self.cached_wall_for = Some(t);
                self.cached_wall_time = Some(fmt::fmt_local_hms(t));
            }
            let wall_str = self.cached_wall_time.as_deref().expect("wall time cached");
            spans.push(Span::styled(wall_str, theme::accent_style()));
        }
        if !self.cached_price_spans.is_empty() {
            spans.extend(self.cached_price_spans.clone());
        }
        if self.refreshing_account || self.refreshing_book {
            let address = crate::components::shared::widgets::spinner(self.tick);
            spans.push(Span::raw("  "));
            spans.push(Span::styled(address, theme::accent_style()));
            spans.push(Span::styled(" refreshing", label_style));
        }
        if let Some(err_display) = &self.cached_error_display {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(err_display.as_str(), theme::error_style()));
        }
        let net_color = if self.network.is_production() {
            theme::ERROR
        } else {
            theme::WARNING
        };
        let net_style = Style::new().fg(net_color).bold().reversed();
        let badge_len = self.network_badge.len() as u16;
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(badge_len)])
                .flex(Flex::SpaceBetween)
                .areas(area);
        frame.render_widget(Paragraph::new(Line::from(spans)), left_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                self.network_badge.as_str(),
                net_style,
            )])),
            right_area,
        );
        Ok(())
    }
}

/// "59s" / "1m" / "59m" / "1h" freshness bucket for an elapsed-secs value.
/// The `None` (never updated) case renders `-` in the caller, not here.
fn freshness_label(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-136: freshness_label bucket boundaries — 59s/1m/59m/1h.
    #[test]
    fn freshness_label_boundaries() {
        assert_eq!(freshness_label(0), "0s");
        assert_eq!(freshness_label(59), "59s");
        assert_eq!(freshness_label(60), "1m");
        assert_eq!(freshness_label(3599), "59m");
        assert_eq!(freshness_label(3600), "1h");
    }

    /// TC-136: cached_freshness_label keeps the `-` path when no update was seen.
    #[test]
    fn cached_freshness_label_none_uses_dash() {
        let mut cache = None;
        assert_eq!(StatusBar::cached_freshness_label(&mut cache, None), "-");
        assert!(cache.is_none());
    }

    /// TC-174: staleness-aware connection state — server_info error wins
    /// (Offline), an aged server update degrades (Stale), fresh stays Online.
    #[test]
    fn connection_state_staleness_transitions() {
        let mut bar = StatusBar::new(
            "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
            Network::Testnet,
            false,
        );
        assert_eq!(bar.connection_state(), ConnectionState::Connecting);
        bar.last_error = Some("server_info: connection refused".into());
        assert_eq!(bar.connection_state(), ConnectionState::Offline);
        bar.last_server_update = Some(Instant::now() - Duration::from_secs(31));
        assert_eq!(bar.connection_state(), ConnectionState::Offline);
        bar.last_error = None;
        assert_eq!(bar.connection_state(), ConnectionState::Stale);
        bar.last_server_update = Some(Instant::now());
        assert_eq!(bar.connection_state(), ConnectionState::Online);
    }

    /// TC-174: flare dot — disabled renders nothing, failure arms Offline,
    /// fresh success restores Online, aged success degrades to Stale.
    #[test]
    fn flare_state_tracks_enabled_failure_and_staleness() {
        let mut bar = StatusBar::new("acct".into(), Network::Testnet, true);
        assert_eq!(bar.flare_state(), Some(ConnectionState::Connecting));
        bar.last_flare_update = Some(Instant::now());
        assert_eq!(bar.flare_state(), Some(ConnectionState::Online));
        bar.last_flare_update = Some(Instant::now() - Duration::from_secs(31));
        assert_eq!(bar.flare_state(), Some(ConnectionState::Stale));
        bar.flare_down = true;
        assert_eq!(bar.flare_state(), Some(ConnectionState::Offline));
        bar.flare_enabled = false;
        assert_eq!(bar.flare_state(), None);
    }

    /// TC-174: draw renders the flare dot and dUNL data-age marker when fed
    /// their actions; disabled flare renders no `flr:` track.
    #[test]
    fn draw_shows_flr_and_dunl_tracks() {
        let mut bar = StatusBar::new("acct".into(), Network::Testnet, true);
        bar.update(&Action::XrplServerInfo(Box::new(
            crate::xrpl::ServerInfoSummary {
                ledger_index: 100,
                hostid: String::new(),
                build_version: String::new(),
                validation_quorum: None,
                validator_list: None,
            },
        )))
        .expect("update");
        bar.update(&Action::FlareOraclePrices(Vec::new()))
            .expect("update");
        let dunl = crate::xrpl::DunlSummary {
            fetched_at: SystemTime::now(),
            validator_count: 2,
            sequence: 1,
            expiration_ripple: 0,
            expiration_utc: String::new(),
            validators: Vec::new(),
        };
        bar.update(&Action::XrplDunl(dunl.clone())).expect("update");
        let rendered = crate::test_support::render_to_string(120, 1, |frame| {
            bar.draw(frame, frame.area()).expect("draw");
        });
        assert!(rendered.contains("ONLINE"), "chip: {rendered}");
        assert!(rendered.contains("flr:"), "flare track: {rendered}");
        assert!(rendered.contains("dunl:"), "dUNL track: {rendered}");

        // Characterization (TC-174): a manifest older than DUNL_CACHE_TTL
        // renders the WARNING-colored `!` marker from its preserved data age.
        let aged_dunl = crate::xrpl::DunlSummary {
            fetched_at: SystemTime::now() - Duration::from_secs(601),
            ..dunl
        };
        bar.update(&Action::XrplDunl(aged_dunl)).expect("update");
        let rendered = crate::test_support::render_to_string(120, 1, |frame| {
            bar.draw(frame, frame.area()).expect("draw");
        });
        assert!(rendered.contains('!'), "stale marker: {rendered}");

        let mut muted = StatusBar::new("acct".into(), Network::Testnet, false);
        let rendered = crate::test_support::render_to_string(120, 1, |frame| {
            muted.draw(frame, frame.area()).expect("draw");
        });
        assert!(!rendered.contains("flr:"), "no flare track when disabled");
    }
}
