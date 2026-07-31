use std::time::{Instant, SystemTime};

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
    xrpl::XrplRlusdPrice,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Online,
    Offline,
    Connecting,
}

impl ConnectionState {
    fn label(self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::Offline => "OFFLINE",
            Self::Connecting => "CONNECTING",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Online => "●",
            Self::Offline => "✖",
            Self::Connecting => "○",
        }
    }

    fn color(self) -> ratatui::style::Color {
        match self {
            Self::Online => theme::SUCCESS,
            Self::Offline => theme::ERROR,
            Self::Connecting => theme::WARNING,
        }
    }
}

pub struct StatusBar {
    account_short: String,
    network: Network,
    network_badge: String,
    last_server_update: Option<Instant>,
    last_account_update: Option<Instant>,
    last_book_update: Option<Instant>,
    last_any_update_wall: Option<SystemTime>,
    cached_wall_time: Option<String>,
    last_error: Option<String>,
    cached_error_display: Option<String>,
    refreshing_account: bool,
    refreshing_book: bool,
    tick: usize,
    price: Option<XrplRlusdPrice>,
    cached_price_spans: Vec<Span<'static>>,
    // freshness caches: (last_elapsed_secs, formatted_string)
    freshness_srv: Option<(u64, String)>,
    freshness_acc: Option<(u64, String)>,
    freshness_book: Option<(u64, String)>,
    // state cache
    cached_state_display: Option<String>,
}

impl StatusBar {
    pub fn new(account: String, network: Network) -> Self {
        let account_short = short_account(&account);
        let network_badge = format!(" {} ", network.display_name());
        Self {
            account_short,
            network,
            network_badge,
            last_server_update: None,
            last_account_update: None,
            last_book_update: None,
            last_any_update_wall: None,
            cached_wall_time: None,
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

    fn cached_freshness_label(
        cache: &mut Option<(u64, String)>,
        latest: Option<Instant>,
    ) -> String {
        match latest {
            Some(t) => {
                let secs = t.elapsed().as_secs();
                if let Some((cached_secs, cached_str)) = cache
                    && *cached_secs == secs
                {
                    return cached_str.clone();
                }
                let freshness_label = if secs < 60 {
                    format!("{secs}s")
                } else if secs < 3600 {
                    format!("{}m", secs / 60)
                } else {
                    format!("{}h", secs / 3600)
                };
                *cache = Some((secs, freshness_label.clone()));
                freshness_label
            }
            None => {
                *cache = None;
                "-".to_string()
            }
        }
    }

    fn connection_state(&self) -> ConnectionState {
        if self.last_server_update.is_some() {
            ConnectionState::Online
        } else if self.last_error.is_some() {
            ConnectionState::Offline
        } else {
            ConnectionState::Connecting
        }
    }
}

fn short_account(address: &str) -> String {
    if address.len() > 12 {
        format!("{}…{}", &address[..6], &address[address.len() - 4..])
    } else {
        address.to_string()
    }
}

impl Component for StatusBar {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
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
            Action::XrplRlusdPrice(p) => {
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
        // Cache state display string
        let state_display = self
            .cached_state_display
            .get_or_insert_with(|| format!(" {} {} ", state.icon(), state.label()));
        let mut spans = vec![
            Span::styled(state_display.clone(), state_style),
            Span::raw(" "),
            Span::styled("acct:", label_style),
            Span::styled(self.account_short.clone(), theme::accent_style()),
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
        if let Some(t) = self.last_any_update_wall {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("@", label_style));
            // Cache wall-time string (changes once per second)
            let wall_str = match &self.cached_wall_time {
                Some(address) if address.len() >= 8 => address.clone(), // rough heuristic; reformat below
                _ => {
                    let address = fmt::fmt_local_hms(t);
                    self.cached_wall_time = Some(address.clone());
                    address
                }
            };
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
            spans.push(Span::styled(err_display.clone(), theme::error_style()));
        }
        let net_color = if self.network.is_mainnet() {
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
                self.network_badge.clone(),
                net_style,
            )])),
            right_area,
        );
        Ok(())
    }
}
