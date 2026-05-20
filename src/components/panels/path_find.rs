use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::SelectableTableState,
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_error, render_loading, titled_block_with_count},
        },
    },
    xrpl::{ArcValue, PathFindRow},
};

#[derive(Default)]
pub struct PathFindPanel {
    rows: Vec<PathFindRow>,
    dest_summary: String,
    route_count: usize,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    error: Option<String>,
    pub is_focused: bool,
    detail: TxDetailState,
}

impl PathFindPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }

    fn summary_lines(&self) -> Vec<Line<'static>> {
        if self.dest_summary.is_empty() {
            return Vec::new();
        }
        let route_part = if self.route_count == 0 {
            "no routes".to_string()
        } else {
            let word = if self.route_count == 1 {
                "route"
            } else {
                "routes"
            };
            format!("{} {word} (cheapest send first)", self.route_count)
        };
        vec![
            Line::from(vec![
                Span::styled("Receive ", theme::dim_style()),
                Span::styled(self.dest_summary.clone(), theme::accent_style()),
                Span::styled(format!(" · {route_part}"), theme::dim_style()),
            ]),
            Line::from(Span::styled(
                "Self-payment preview for the configured book pair · Enter: raw JSON",
                theme::dim_style(),
            )),
        ]
    }
}

fn path_find_table_row(rank: usize, row: &PathFindRow) -> Row<'_> {
    Row::new(vec![
        Cell::from(format!("{rank}")).style(theme::dim_style()),
        Cell::from(row.send.as_str()).style(theme::accent_style()),
        Cell::from(row.hops.as_str()).style(theme::secondary_style()),
        Cell::from(row.path.as_str()),
    ])
}

impl Component for PathFindPanel {
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
            Action::XrplPathFind(snap) => {
                self.dest_summary = snap.dest_summary.clone();
                self.rows = snap.rows.clone();
                self.route_count = self.rows.len();
                self.table_state.reset_len(self.rows.len());
                self.received = true;
                self.error = None;
            }
            Action::XrplError(e) if e.contains("ripple_path_find") => {
                self.received = true;
                self.rows.clear();
                self.route_count = 0;
                self.table_state.reset_len(0);
                self.error = Some(e.to_string());
            }
            Action::SelectNext if !self.rows.is_empty() && self.is_focused => {
                self.table_state.select_next(self.rows.len());
            }
            Action::SelectPrev if !self.rows.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.rows.len());
            }
            Action::TxDetailToggle if self.is_focused && !self.rows.is_empty() => {
                if let Some(idx) = self.table_state.selected()
                    && let Some(row) = self.rows.get(idx)
                {
                    self.detail.open(row.raw_json.clone(), ArcValue::default());
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.detail.visible {
            render_tx_detail(frame, area, &mut self.detail);
            return Ok(());
        }

        if !self.received {
            render_loading(
                frame,
                area,
                "Path-Find",
                self.tick,
                "finding payment routes...",
                self.is_focused,
            );
            return Ok(());
        }

        if let Some(err) = &self.error {
            render_error(frame, area, "Path-Find", err, self.is_focused);
            return Ok(());
        }

        if self.rows.is_empty() {
            let msg = if self.dest_summary.is_empty() {
                "No payment routes found for this pair.".to_string()
            } else {
                format!(
                    "Receive {} — no routes (try another issuer or currency on Overview)",
                    self.dest_summary
                )
            };
            render_empty(frame, area, "Path-Find", &msg, self.is_focused);
            return Ok(());
        }

        let block = titled_block_with_count(
            "Path-Find",
            self.table_state.selected(),
            self.rows.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let summary_lines = self.summary_lines();
        let table_area = if summary_lines.is_empty() {
            inner
        } else {
            let [summary_area, rest] =
                Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);
            frame.render_widget(Paragraph::new(summary_lines), summary_area);
            rest
        };

        let header =
            Row::new(vec!["#", "You send", "Hops", "Route"]).style(theme::header_row_style());
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| path_find_table_row(i + 1, r));
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(16),
                Constraint::Length(8),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .column_spacing(1)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        let [tbl_area, sb_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(table_area);

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
