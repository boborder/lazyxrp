use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
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
        let len = self.rows.len();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.rows.get(idx))
            .map(|row| (row.raw_json.clone(), ArcValue::default()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
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
            _ if self
                .table_state
                .handle_row_select(action, self.is_focused, len) => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
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
            let empty_routes_message = if self.dest_summary.is_empty() {
                "No payment routes found for this pair.".to_string()
            } else {
                format!(
                    "Receive {} — no routes (try another issuer or currency on Overview)",
                    self.dest_summary
                )
            };
            render_empty(
                frame,
                area,
                "Path-Find",
                &empty_routes_message,
                self.is_focused,
            );
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
        .column_spacing(1);

        render_selectable_table(
            frame,
            table_area,
            table,
            &mut self.table_state,
            self.is_focused,
        );

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrpl::PathFindSnapshot;

    #[test]
    fn path_find_panel_renders_loading_empty_and_route_summary() {
        let mut panel = PathFindPanel::default();
        let loading = crate::test_support::render_to_string(100, 12, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(loading.contains("finding payment routes"));

        panel
            .update(&Action::XrplPathFind(PathFindSnapshot {
                dest_summary: "100 USD".into(),
                rows: vec![PathFindRow {
                    send: "2 XRP".into(),
                    hops: "XRP → USD".into(),
                    path: "rIssuer".into(),
                    raw_json: ArcValue::default(),
                }],
            }))
            .unwrap();
        let out = crate::test_support::render_to_string(100, 12, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(out.contains("Receive 100 USD"));
        assert!(out.contains("2 XRP"));
        assert!(out.contains("XRP"));
    }
}
