//! Validator detail overlay for the server panel.
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use crate::components::shared::{theme, widgets::centered_popup_rect};
use crate::xrpl::{DunlSummary, DunlValidatorRow, XrplTomlData};

#[derive(Default)]
pub(super) struct ValidatorDetail {
    pub(super) visible: bool,
    pub(super) scroll: usize,
    pub(super) lines: Vec<Line<'static>>,
    pub(super) toml: Option<Result<XrplTomlData, String>>,
    pub(super) toml_raw: Option<String>,
    pub(super) status: u16,
    pub(super) content_type: Option<String>,
}

impl ValidatorDetail {
    // methods remain crate-visible via pub(super) where needed
    pub(super) fn open(&mut self, row: &DunlValidatorRow, index: usize, dunl: &DunlSummary) {
        self.visible = true;
        self.scroll = 0;
        self.toml_raw = None;
        self.status = 0;
        self.content_type = None;
        self.toml = if row.domain.is_some() {
            None
        } else {
            Some(Err(
                "no domain in manifest — cannot fetch xrp-ledger.toml".to_string()
            ))
        };
        self.lines = validator_detail_lines(
            row,
            index,
            dunl,
            &self.toml,
            self.toml_raw.as_deref(),
            self.status,
            self.content_type.as_deref(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_toml(
        &mut self,
        row: &DunlValidatorRow,
        index: usize,
        dunl: &DunlSummary,
        status: u16,
        content_type: Option<String>,
        raw: Option<String>,
        result: Result<XrplTomlData, String>,
    ) {
        self.toml = Some(result);
        self.toml_raw = raw;
        self.status = status;
        self.content_type = content_type;
        self.lines = validator_detail_lines(
            row,
            index,
            dunl,
            &self.toml,
            self.toml_raw.as_deref(),
            self.status,
            self.content_type.as_deref(),
        );
    }

    pub(super) fn close(&mut self) {
        self.visible = false;
        self.scroll = 0;
        self.lines.clear();
        self.toml = None;
        self.toml_raw = None;
    }
}

pub(super) fn validator_detail_lines(
    row: &DunlValidatorRow,
    index: usize,
    dunl: &DunlSummary,
    toml: &Option<Result<XrplTomlData, String>>,
    raw: Option<&str>,
    status: u16,
    content_type: Option<&str>,
) -> Vec<Line<'static>> {
    let label = theme::dim_style();
    let value = theme::accent_style();
    let ok = theme::success_style();
    let warn = theme::warning_style();

    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!("Validator #{}", index + 1),
            theme::accent_style(),
        )]),
        Line::from(""),
    ];

    let push = |lines: &mut Vec<Line<'static>>, key: &str, val: String, style: Style| {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<16}"), label),
            Span::styled(val, style),
        ]));
    };

    push(
        &mut lines,
        "Domain",
        row.domain.clone().unwrap_or_else(|| "—".to_string()),
        if row.domain.is_some() { ok } else { warn },
    );
    push(
        &mut lines,
        "Manifest",
        if row.has_manifest {
            "present".to_string()
        } else {
            "missing".to_string()
        },
        if row.has_manifest { ok } else { warn },
    );
    push(
        &mut lines,
        "Sequence",
        row.sequence
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_string()),
        value,
    );
    push(
        &mut lines,
        "Signing key",
        row.validation_public_key.clone(),
        theme::secondary_style(),
    );
    push(
        &mut lines,
        "Master key",
        row.master_public_key
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        theme::secondary_style(),
    );
    if row.master_differs_from_signing() {
        lines.push(Line::from(Span::styled(
            "  (master ≠ signing — rotated or dual-key setup)",
            warn,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("dUNL list", label),
        Span::raw(format!(
            " · seq {} · {} validators · exp {}",
            dunl.sequence, dunl.validator_count, dunl.expiration_utc
        )),
    ]));
    if let Some(days) = dunl.days_until_expiry() {
        let style = if days < 14 { warn } else { label };
        lines.push(Line::from(Span::styled(
            format!("  expires in {days} day(s)"),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled("xrp-ledger.toml", label)]));
    let status_style = if (200..300).contains(&status) {
        ok
    } else if status >= 400 {
        theme::error_style()
    } else {
        warn
    };
    lines.push(Line::from(vec![
        Span::styled("  HTTP status:", label),
        Span::styled(
            if status > 0 {
                status.to_string()
            } else {
                "—".to_string()
            },
            status_style,
        ),
    ]));
    if let Some(ct) = content_type {
        lines.push(Line::from(vec![
            Span::styled("  Content-Type:", label),
            Span::styled(ct.to_string(), value),
        ]));
    }
    match toml {
        None if row.domain.is_some() => {
            lines.push(Line::from(Span::styled(
                "  fetching...",
                theme::dim_style(),
            )));
        }
        None => {}
        Some(Ok(data)) => {
            let verified_style = if data.validator_found { ok } else { warn };
            lines.push(Line::from(vec![
                Span::styled("  Domain verified:", label),
                Span::styled(
                    if data.validator_found { "Yes" } else { "No" },
                    verified_style,
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Validators listed:", label),
                Span::styled(data.validator_count.to_string(), value),
            ]));
            if let Some(att) = &data.attestation {
                lines.push(Line::from(vec![
                    Span::styled("  Attestation:", label),
                    Span::styled(att.clone(), theme::secondary_style()),
                ]));
            }
        }
        Some(Err(e)) => {
            lines.push(Line::from(Span::styled(
                format!("  Error: {e}"),
                theme::error_style(),
            )));
        }
    }

    if let Some(raw) = raw {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled("Raw TOML", label)]));
        for line_text in raw.lines() {
            lines.push(Line::from(Span::styled(
                line_text.to_string(),
                theme::dim_style(),
            )));
        }
    }

    lines
}

pub(super) fn render_validator_detail(frame: &mut Frame, area: Rect, state: &mut ValidatorDetail) {
    if !state.visible {
        return;
    }

    let height = (state.lines.len() as u16 + 2).clamp(14, 24);
    let popup = centered_popup_rect(area, 44, height);

    frame.render_widget(Clear, popup);

    let title = state
        .lines
        .first()
        .and_then(|l| l.spans.first())
        .map(|s| s.content.to_string())
        .unwrap_or_else(|| "Validator".to_string());

    let block = theme::panel_block(&title, true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let line_count = state.lines.len();
    let paragraph = Paragraph::new(state.lines.clone())
        .wrap(Wrap { trim: true })
        .scroll((state.scroll as u16, 0));

    let [content_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

    frame.render_widget(paragraph, content_area);

    let content_height = content_area.height as usize;
    let max_scroll = line_count.saturating_sub(content_height);
    let mut sb_state = ScrollbarState::new(max_scroll).position(state.scroll);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::secondary_style()),
        sb_area,
        &mut sb_state,
    );
}
