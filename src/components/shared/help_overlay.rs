use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Row, Table},
};

use crate::action::Action;
use crate::components::{Component, shared::theme};

const BINDINGS: &[(&str, &str)] = &[
    ("q / Ctrl-c / Ctrl-d", "Quit"),
    ("Ctrl-z", "Suspend"),
    ("Tab / BackTab / 1-5", "Next / prev tab / jump by number"),
    ("↑ / ↓ / j / k", "Select next / prev row"),
    ("← / → / h / l", "Focus prev / next pane"),
    ("r", "Refresh account (Account tab)"),
    ("b", "Refresh book (Market tab)"),
    (
        "o",
        "Refresh ledger objects (Objects tab: Checks / MPT / DID / …)",
    ),
    (
        "t / e / s (wallet modal)",
        "Overview: `t` → AccountSet or Payment; Tab/[] rows, Enter, `e` type, `s` send; ok closes modal",
    ),
    ("Enter", "Transaction detail (all table panels)"),
    (
        "f",
        "Filter rows by tx type / hash (Tx History + Wallet Recent txs)",
    ),
    ("m", "Load more transactions (Tx History pagination)"),
    ("?", "Toggle this help"),
];

pub struct HelpOverlay;

impl Component for HelpOverlay {
    fn update(&mut self, _action: &Action) -> color_eyre::Result<Option<Action>> {
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let popup_w = 58u16.min(area.width.saturating_sub(4));
        let popup_h = (BINDINGS.len() as u16 + 4).min(area.height.saturating_sub(2));
        let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_w, popup_h);

        frame.render_widget(Clear, popup_area);

        let block = theme::panel_block("Keybindings", true);
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let [header_area, table_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(inner);

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Key", theme::header_row_style()),
            Span::raw("                          "),
            Span::styled("Action", theme::header_row_style()),
        ]));
        frame.render_widget(header, header_area);

        let rows = BINDINGS.iter().map(|(k, v)| {
            Row::new(vec![
                Span::styled(*k, theme::accent_style().bold()),
                Span::raw(*v),
            ])
        });
        let table = Table::new(rows, [Constraint::Length(28), Constraint::Min(0)]);
        frame.render_widget(table, table_area);

        let footer = Paragraph::new("press ? or Esc to close")
            .alignment(Alignment::Center)
            .style(theme::dim_style().dim());
        frame.render_widget(footer, footer_area);

        Ok(())
    }
}
