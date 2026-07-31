//! dUNL table helpers / draw for the server panel.
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::components::shared::{
    fmt,
    selectable_table::{SelectableTableState, render_selectable_table},
    theme,
    widgets::{render_loading, titled_block_with_count},
};
use crate::xrpl::{DunlSummary, DunlValidatorRow};

const DUNL_FOOTER_LINES: u16 = 1;

pub(super) fn validator_row_label(v: &DunlValidatorRow, max_chars: usize) -> String {
    if let Some(d) = &v.domain {
        fmt::truncate_middle(d, max_chars)
    } else if v.has_manifest {
        "(no domain)".to_string()
    } else {
        fmt::short_hex(&v.validation_public_key, 8, 6)
    }
}

pub(super) fn draw_dunl_loading(frame: &mut Frame, area: Rect, tick: usize, focused: bool) {
    render_loading(
        frame,
        area,
        "dUNL",
        tick,
        "loading validator list...",
        focused,
    );
}

pub(super) fn draw_dunl_panel(
    frame: &mut Frame,
    list_area: Rect,
    d: &DunlSummary,
    dunl_table: &mut SelectableTableState,
    focused: bool,
) {
    let stats = d.stats();
    let exp_short = d
        .expiration_utc
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let dunl_title = format!("dUNL validators · seq {} · exp {exp_short}", d.sequence);
    let dunl_block = titled_block_with_count(
        &dunl_title,
        dunl_table.selected(),
        d.validators.len(),
        focused,
    );
    let dunl_inner = dunl_block.inner(list_area);
    frame.render_widget(dunl_block, list_area);

    let [footer_area, table_area] =
        Layout::vertical([Constraint::Length(DUNL_FOOTER_LINES), Constraint::Fill(1)])
            .areas(dunl_inner);

    let footer = Line::from(vec![
        Span::styled(
            format!(
                "manifest {}/{} · domain {}/{} · master≠ {}",
                stats.with_manifest,
                stats.total,
                stats.with_domain,
                stats.total,
                stats.master_distinct,
            ),
            theme::dim_style(),
        ),
        Span::styled("  │  ", theme::dim_style()),
        Span::styled("j/k scroll", theme::secondary_style()),
        Span::styled("  Enter detail", theme::secondary_style()),
    ]);
    frame.render_widget(Paragraph::new(footer), footer_area);

    if d.validators.is_empty() {
        frame.render_widget(
            Paragraph::new("(no validators in blob)").style(theme::dim_style()),
            table_area,
        );
        return;
    }

    let label_max = table_area.width.saturating_sub(28).clamp(14, 36) as usize;
    let rows = d.validators.iter().enumerate().map(|(i, v)| {
        let name = validator_row_label(v, label_max);
        let seq = v
            .sequence
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_string());
        let master_col = if v.master_differs_from_signing() {
            "≠"
        } else {
            "·"
        };
        let name_style = if v.domain.is_some() {
            theme::success_style()
        } else if v.has_manifest {
            theme::warning_style()
        } else {
            theme::dim_style()
        };
        let master_style = if v.master_differs_from_signing() {
            theme::secondary_style()
        } else {
            theme::dim_style()
        };
        Row::new(vec![
            Cell::from((i + 1).to_string()),
            Cell::from(name).style(name_style),
            Cell::from(seq),
            Cell::from(master_col).style(master_style),
            Cell::from(fmt::short_hex(&v.validation_public_key, 8, 6))
                .style(theme::secondary_style()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Length(18),
        ],
    )
    .header(
        Row::new(vec!["#", "Domain / key", "Seq", "M", "Signing"]).style(theme::header_row_style()),
    );

    render_selectable_table(frame, table_area, table, dunl_table, focused);
}
