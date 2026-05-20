use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::{ArcValue, TrustLineRow},
};

#[derive(Default)]
pub struct TrustLinesPanel {
    lines: Vec<TrustLineRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
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
        if self.detail.visible {
            match action {
                Action::TxDetailToggle => {
                    self.detail.close();
                    return Ok(None);
                }
                Action::SelectNext | Action::FocusNext => {
                    self.detail.scroll = self.detail.scroll.saturating_add(1);
                    return Ok(None);
                }
                Action::SelectPrev | Action::FocusPrev => {
                    self.detail.scroll = self.detail.scroll.saturating_sub(1);
                    return Ok(None);
                }
                Action::Quit => return Ok(None),
                _ => return Ok(None),
            }
        }
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
            Action::TxDetailToggle if self.is_focused && !self.lines.is_empty() => {
                if let Some(idx) = self.table_state.selected()
                    && let Some(line) = self.lines.get(idx)
                {
                    self.detail.open(line.raw_json.clone(), ArcValue::default());
                }
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
        .header(header);

        render_selectable_table(frame, inner, table, &mut self.table_state, self.is_focused);

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}
