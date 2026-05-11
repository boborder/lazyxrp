use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::SelectableTableState,
            theme,
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::TrustLineRow,
};

#[derive(Default)]
pub struct TrustLinesPanel {
    lines: Vec<TrustLineRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
}

impl TrustLinesPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }
}

impl Component for TrustLinesPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplTrustLines(lines) => {
                self.lines = lines.to_vec();
                self.table_state.reset_len(self.lines.len());
                self.received = true;
            }
            Action::SelectNext if !self.lines.is_empty() && self.is_focused => {
                self.table_state.select_next(self.lines.len());
            }
            Action::SelectPrev if !self.lines.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.lines.len());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.received {
            render_loading(
                frame,
                area,
                "Trust Lines",
                self.tick,
                "loading trust lines...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.lines.is_empty() {
            render_empty(
                frame,
                area,
                "Trust Lines",
                "(no trust lines)",
                self.is_focused,
            );
            return Ok(());
        }
        let block = titled_block_with_count(
            "Trust Lines",
            self.table_state.selected(),
            self.lines.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let header = Row::new(vec!["Currency", "Issuer", "Balance", "Limit"])
            .style(theme::header_row_style());
        let rows = self.lines.iter().map(|l| {
            let balance_style = if l.balance.starts_with('-') {
                theme::error_style()
            } else {
                theme::success_style()
            };
            Row::new(vec![
                l.currency.clone(),
                l.account.chars().take(12).collect::<String>(),
                l.balance.clone(),
                l.limit.clone(),
            ])
            .style(balance_style)
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(18),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        let [tbl_area, sb_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

        frame.render_stateful_widget(table, tbl_area, self.table_state.table_mut());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(theme::dim_style())
                .thumb_style(theme::accent_style()),
            sb_area,
            self.table_state.scroll_mut(),
        );
        Ok(())
    }
}
