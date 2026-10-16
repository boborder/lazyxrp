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

/// Validator domain, TOML text and HTTP headers come from remote hosts: strip controls and
/// bidi/format characters (TC-236) before they reach the terminal.
fn clean(s: &str) -> String {
    crate::xrpl::sanitize_text(s, 512)
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
    let label_style = theme::dim_style();
    let value_style = theme::accent_style();
    let success_style = theme::success_style();
    let warning_style = theme::warning_style();

    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!("Validator #{}", index + 1),
            theme::accent_style(),
        )]),
        Line::from(""),
    ];

    let push = |lines: &mut Vec<Line<'static>>, key: &str, val: String, style: Style| {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<16}"), label_style),
            Span::styled(clean(&val), style),
        ]));
    };

    push(
        &mut lines,
        "Domain",
        row.domain.clone().unwrap_or_else(|| "—".to_string()),
        if row.domain.is_some() {
            success_style
        } else {
            warning_style
        },
    );
    push(
        &mut lines,
        "Manifest",
        if row.has_manifest {
            "present".to_string()
        } else {
            "missing".to_string()
        },
        if row.has_manifest {
            success_style
        } else {
            warning_style
        },
    );
    push(
        &mut lines,
        "Sequence",
        row.sequence
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_string()),
        value_style,
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
            warning_style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("dUNL list", label_style),
        Span::raw(format!(
            " · seq {} · {} validators · exp {}",
            dunl.sequence, dunl.validator_count, dunl.expiration_utc
        )),
    ]));
    if let Some(days) = dunl.days_until_expiry() {
        let style = if days < 14 {
            warning_style
        } else {
            label_style
        };
        lines.push(Line::from(Span::styled(
            format!("  expires in {days} day(s)"),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "xrp-ledger.toml",
        label_style,
    )]));
    let status_style = if (200..300).contains(&status) {
        success_style
    } else if status >= 400 {
        theme::error_style()
    } else {
        warning_style
    };
    lines.push(Line::from(vec![
        Span::styled("  HTTP status:", label_style),
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
            Span::styled("  Content-Type:", label_style),
            Span::styled(clean(ct), value_style),
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
            let verified_style = if data.validator_found {
                success_style
            } else {
                warning_style
            };
            lines.push(Line::from(vec![
                Span::styled("  Domain verified:", label_style),
                Span::styled(
                    if data.validator_found { "Yes" } else { "No" },
                    verified_style,
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Validators listed:", label_style),
                Span::styled(data.validator_count.to_string(), value_style),
            ]));
            if let Some(att) = &data.attestation {
                lines.push(Line::from(vec![
                    Span::styled("  Attestation:", label_style),
                    Span::styled(clean(att), theme::secondary_style()),
                ]));
            }
        }
        Some(Err(e)) => {
            lines.push(Line::from(Span::styled(
                clean(&format!("  Error: {e}")),
                theme::error_style(),
            )));
        }
    }

    if let Some(raw) = raw {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled("Raw TOML", label_style)]));
        for line_text in raw.lines() {
            lines.push(Line::from(Span::styled(
                clean(line_text),
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

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn detail_rendered(state: &mut ValidatorDetail) -> String {
        crate::test_support::render_to_string(100, 30, |frame| {
            render_validator_detail(frame, frame.area(), state)
        })
    }

    fn row(domain: Option<&str>, master: Option<&str>) -> DunlValidatorRow {
        DunlValidatorRow {
            validation_public_key: "EDFB01CA58D69A2B".into(),
            has_manifest: true,
            domain: domain.map(str::to_string),
            sequence: Some(7),
            master_public_key: master.map(str::to_string),
        }
    }

    fn dunl() -> DunlSummary {
        DunlSummary {
            fetched_at: SystemTime::now(),
            validator_count: 2,
            sequence: 123,
            expiration_ripple: 0,
            expiration_utc: "2026-01-01T00:00:00Z".into(),
            validators: vec![],
        }
    }

    /// TC-236: validator domain, TOML text, attestation and parse errors come from remote
    /// hosts; bidi/format characters (U+202E) must not reach the rendered lines.
    #[test]
    fn validator_detail_strips_bidi_and_controls_from_remote_text() {
        let evil = "ev\u{202E}il\u{1b}[2J.org";
        let row = row(Some(evil), None);
        let data = XrplTomlData {
            attestation: Some("att\u{202E}est".into()),
            validator_found: true,
            ..Default::default()
        };
        let lines = validator_detail_lines(
            &row,
            0,
            &dunl(),
            &Some(Ok(data)),
            Some("a = \"x\u{202E}y\"\nb\u{2066}=1"),
            200,
            Some("text/\u{202E}plain"),
        );
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        for bad in ['\u{202E}', '\u{2066}', '\u{1b}'] {
            assert!(!text.contains(bad), "{bad:?} leaked into {text:?}");
        }
        assert!(text.contains("evil[2J.org"), "visible text preserved");
        assert!(text.contains("attest") && text.contains("xy"));
    }

    /// DUNL expiring `secs_from_now` seconds from the real clock; callers add a 12h margin so
    /// whole-day truncation never straddles a day boundary.
    fn dunl_expiring_in(secs_from_now: i64) -> DunlSummary {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        DunlSummary {
            expiration_ripple: (now + secs_from_now - crate::xrpl::RIPPLE_EPOCH_UNIX) as u64,
            ..dunl()
        }
    }

    fn expiry_line(dunl: &DunlSummary) -> Line<'static> {
        validator_detail_lines(&row(None, None), 0, dunl, &None, None, 0, None)
            .into_iter()
            .find(|l| l.spans.iter().any(|s| s.content.contains("expires in")))
            .expect("expiry line")
    }

    #[test]
    fn validator_detail_expiry_warns_under_14_days_and_handles_expired() {
        const DAY: i64 = 86_400;
        let far = expiry_line(&dunl_expiring_in(20 * DAY + DAY / 2));
        assert_eq!(far.spans[0].content, "  expires in 20 day(s)");
        assert_eq!(far.spans[0].style, theme::dim_style());

        let soon = expiry_line(&dunl_expiring_in(13 * DAY + DAY / 2));
        assert_eq!(soon.spans[0].content, "  expires in 13 day(s)");
        assert_eq!(soon.spans[0].style, theme::warning_style());

        let expired = expiry_line(&dunl_expiring_in(-10 * DAY - DAY / 2));
        assert_eq!(expired.spans[0].content, "  expires in -10 day(s)");
        assert_eq!(expired.spans[0].style, theme::warning_style());
    }

    #[test]
    fn validator_detail_renders_identity_dunl_and_fetching_state() {
        let row = row(Some("xrplf.org"), None);
        let mut state = ValidatorDetail {
            visible: true,
            lines: validator_detail_lines(&row, 0, &dunl(), &None, None, 200, Some("text/plain")),
            ..Default::default()
        };

        let out = detail_rendered(&mut state);
        assert!(out.contains("Validator #1"));
        assert!(out.contains("xrplf.org"));
        assert!(out.contains("EDFB01CA58D69A2B"));
        assert!(out.contains("seq 123"));
        assert!(out.contains("2 validators"));
        assert!(out.contains("200"));
        assert!(out.contains("text/plain"));
        assert!(!out.contains("Raw TOML"));
    }

    #[test]
    fn validator_detail_renders_verified_toml_rotated_master_and_raw_dump() {
        let row = row(Some("xrplf.org"), Some("EDFFFF0099AABBCC"));
        let toml: Option<Result<XrplTomlData, String>> = Some(Ok(XrplTomlData {
            domain: "xrplf.org".into(),
            validator_found: true,
            attestation: Some("sig 0xAB".into()),
            validator_count: 3,
        }));
        let mut state = ValidatorDetail {
            visible: true,
            lines: validator_detail_lines(
                &row,
                1,
                &dunl(),
                &toml,
                Some("[SERVER]\nnode = \"wss://a.b\""),
                200,
                None,
            ),
            ..Default::default()
        };

        let out = detail_rendered(&mut state);
        assert!(out.contains("Validator #2"));
        assert!(out.contains("EDFFFF0099AABBCC"));
        assert!(out.contains("Domain verified:Yes"));
        assert!(out.contains("sig 0xAB"));
        assert!(out.contains("node = \"wss://a.b\""));
    }

    #[test]
    fn validator_detail_renders_missing_manifest_and_toml_error_state() {
        let row = DunlValidatorRow {
            has_manifest: false,
            ..row(None, None)
        };
        let toml: Option<Result<XrplTomlData, String>> = Some(Err("boom".to_string()));
        let mut state = ValidatorDetail {
            visible: true,
            lines: validator_detail_lines(&row, 0, &dunl(), &toml, None, 404, None),
            ..Default::default()
        };

        let out = detail_rendered(&mut state);
        assert!(out.contains("missing"));
        assert!(out.contains("404"));
        assert!(out.contains("Error: boom"));
        assert!(!out.contains("Domain verified:"));
    }
}
