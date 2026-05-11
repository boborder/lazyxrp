use std::time::{Instant, SystemTime};

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
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

pub struct StatusBar {
    account: String,
    network: Network,
    last_server_update: Option<Instant>,
    last_account_update: Option<Instant>,
    last_book_update: Option<Instant>,
    last_any_update_wall: Option<SystemTime>,
    last_error: Option<String>,
    refreshing_account: bool,
    refreshing_book: bool,
    tick: usize,
    price: Option<XrplRlusdPrice>,
}

impl StatusBar {
    pub fn new(account: String, network: Network) -> Self {
        Self {
            account,
            network,
            last_server_update: None,
            last_account_update: None,
            last_book_update: None,
            last_any_update_wall: None,
            last_error: None,
            refreshing_account: false,
            refreshing_book: false,
            tick: 0,
            price: None,
        }
    }

    fn freshness(latest: Option<Instant>) -> String {
        match latest {
            Some(t) => {
                let secs = t.elapsed().as_secs();
                if secs < 60 {
                    format!("{secs}s")
                } else if secs < 3600 {
                    format!("{}m", secs / 60)
                } else {
                    format!("{}h", secs / 3600)
                }
            }
            None => "-".to_string(),
        }
    }

    fn connection_state(&self) -> &'static str {
        if self.last_server_update.is_some() {
            "ONLINE"
        } else if self.last_error.is_some() {
            "OFFLINE"
        } else {
            "CONNECTING"
        }
    }
}

fn short_account(s: &str) -> String {
    if s.len() > 12 {
        format!("{}…{}", &s[..6], &s[s.len() - 4..])
    } else {
        s.to_string()
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
            }
            Action::XrplError(msg) => self.last_error = Some(msg.to_string()),
            Action::RefreshAccount => self.refreshing_account = true,
            Action::RefreshBook => self.refreshing_book = true,
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let state = self.connection_state();
        let state_color = match state {
            "ONLINE" => theme::SUCCESS,
            "OFFLINE" => theme::ERROR,
            _ => theme::WARNING,
        };
        let state_style = Style::new()
            .fg(state_color)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
        let label = theme::dim_style();
        let state_icon = match state {
            "ONLINE" => "●",
            "OFFLINE" => "✖",
            _ => "○",
        };
        let mut spans = vec![
            Span::styled(format!(" {state_icon} {state} "), state_style),
            Span::raw(" "),
            Span::styled("acct:", label),
            Span::styled(short_account(&self.account), theme::accent_style()),
            Span::raw("  "),
            Span::styled("srv:", label),
            Span::raw(Self::freshness(self.last_server_update)),
            Span::raw("  "),
            Span::styled("acc:", label),
            Span::raw(Self::freshness(self.last_account_update)),
            Span::raw("  "),
            Span::styled("bk:", label),
            Span::raw(Self::freshness(self.last_book_update)),
        ];
        if let Some(t) = self.last_any_update_wall {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("@", label));
            spans.push(Span::styled(fmt::fmt_local_hms(t), theme::accent_style()));
        }
        if let Some(p) = &self.price {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("XRP/RLUSD", label));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("M:{}", p.mid), theme::accent_style()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("B:{}", p.bid), theme::success_style()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("A:{}", p.ask), theme::error_style()));
        }
        if self.refreshing_account || self.refreshing_book {
            let s = crate::components::shared::widgets::spinner(self.tick);
            spans.push(Span::raw("  "));
            spans.push(Span::styled(s, theme::accent_style()));
            spans.push(Span::styled(" refreshing", label));
        }
        if let Some(err) = &self.last_error {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(format!("err:{err}"), theme::error_style()));
        }
        let net_color = if self.network.is_mainnet() {
            theme::ERROR
        } else {
            theme::WARNING
        };
        let net_style = Style::new()
            .fg(net_color)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
        let badge = format!(" {} ", self.network.display_name());
        let badge_len = badge.len() as u16;
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(badge_len)])
                .flex(Flex::SpaceBetween)
                .areas(area);
        frame.render_widget(Paragraph::new(Line::from(spans)), left_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(badge, net_style)])),
            right_area,
        );
        Ok(())
    }
}
