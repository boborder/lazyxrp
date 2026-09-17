use ratatui::{
    style::{Color, Style},
    widgets::{Block, BorderType},
};

// Royal Blue palette + turquoise secondary
pub const BORDER: Color = Color::Rgb(65, 105, 225); // Royal Blue
pub const TITLE: Color = Color::Rgb(100, 149, 237); // Cornflower Blue
pub const ACCENT: Color = Color::Rgb(30, 144, 255); // Dodger Blue
/// Second accent for hashes / metadata (pairs with ACCENT on blue terminals).
pub const SECONDARY: Color = Color::Rgb(64, 224, 208); // Turquoise
pub const MUTED: Color = Color::Rgb(119, 136, 153); // Light Slate Gray
pub const SUCCESS: Color = Color::Rgb(60, 179, 113); // Medium Sea Green
pub const ERROR: Color = Color::Rgb(220, 20, 60); // Crimson
pub const WARNING: Color = Color::Rgb(255, 165, 0); // Orange
pub const HIGHLIGHT_FG: Color = Color::Rgb(255, 255, 255); // White
/// Light sky blue for account flag chips (brand accent sibling).
pub const FLAG: Color = Color::Rgb(100, 200, 255);
pub const HIGHLIGHT_BG: Color = BORDER;
/// Foreground for values drawn on ACCENT fills (BarChart labels, etc.).
/// Dark slate — readable on ACCENT without hardcoded Color::Black.
pub const CHART_VALUE_FG: Color = Color::Rgb(15, 23, 42); // Slate-900

pub fn panel_block(title: &str, is_focused: bool) -> Block<'static> {
    panel_block_owned(format!(" {title} "), is_focused)
}

/// Like [`panel_block`], but `title` is used as-is (caller supplies spacing/count text).
pub fn panel_block_owned(title: String, is_focused: bool) -> Block<'static> {
    let border_color = if is_focused { ACCENT } else { MUTED };
    let title_color = if is_focused { TITLE } else { MUTED };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border_color))
        .title_style(Style::new().fg(title_color).bold())
        .title(title)
}

pub fn header_row_style() -> Style {
    Style::new().fg(ACCENT).bold().underlined()
}

pub fn selected_row_style(is_focused: bool) -> Style {
    if is_focused {
        Style::new().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).bold()
    } else {
        Style::new().fg(MUTED).reversed()
    }
}

pub fn dim_style() -> Style {
    Style::new().fg(MUTED)
}

pub fn accent_style() -> Style {
    Style::new().fg(ACCENT)
}

pub fn secondary_style() -> Style {
    Style::new().fg(SECONDARY)
}

pub fn success_style() -> Style {
    Style::new().fg(SUCCESS)
}

pub fn error_style() -> Style {
    Style::new().fg(ERROR)
}

pub fn warning_style() -> Style {
    Style::new().fg(WARNING)
}

pub fn flag_style() -> Style {
    Style::new().fg(FLAG).bold()
}

pub fn chart_value_style() -> Style {
    Style::new().fg(CHART_VALUE_FG).bg(ACCENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn color_to_hex(c: Color) -> String {
        match c {
            Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
            other => panic!("expected Rgb constant, got {other:?}"),
        }
    }

    fn parse_frontmatter_colors(design_md: &str) -> BTreeMap<String, String> {
        let mut lines = design_md.lines();
        assert_eq!(
            lines.next(),
            Some("---"),
            "DESIGN.md must start with YAML frontmatter"
        );
        let mut yaml = String::new();
        for line in lines {
            if line == "---" {
                break;
            }
            yaml.push_str(line);
            yaml.push('\n');
        }
        let mut colors = BTreeMap::new();
        let mut in_colors = false;
        for line in yaml.lines() {
            if line == "colors:" {
                in_colors = true;
                continue;
            }
            if !in_colors {
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if !line.starts_with(' ') {
                in_colors = false;
                continue;
            }
            let trimmed = line.trim();
            let (key, value) = trimmed
                .split_once(':')
                .expect("colors entry must be key: value");
            let hex = value.trim().trim_matches('"');
            colors.insert(key.to_string(), hex.to_string());
        }
        assert!(
            !colors.is_empty(),
            "frontmatter colors block must not be empty"
        );
        colors
    }

    /// TC-128: DESIGN.md YAML color tokens stay aligned with `theme.rs` (Google design.md frontmatter).
    #[test]
    fn design_md_frontmatter_colors_match_theme_constants() {
        let design_md = include_str!("../../../DESIGN.md");
        let tokens = parse_frontmatter_colors(design_md);

        let expected = [
            ("primary", ACCENT),
            ("border", BORDER),
            ("title", TITLE),
            ("secondary", SECONDARY),
            ("muted", MUTED),
            ("success", SUCCESS),
            ("error", ERROR),
            ("warning", WARNING),
            ("flag", FLAG),
            ("highlight-fg", HIGHLIGHT_FG),
            ("chart-value-fg", CHART_VALUE_FG),
        ];
        for (token, constant) in expected {
            let yaml_hex = tokens
                .get(token)
                .unwrap_or_else(|| panic!("DESIGN.md colors.{token} missing"));
            assert_eq!(
                yaml_hex,
                &color_to_hex(constant),
                "DESIGN.md colors.{token} must match theme.rs"
            );
        }
        assert_eq!(
            color_to_hex(BORDER),
            color_to_hex(HIGHLIGHT_BG),
            "BORDER and HIGHLIGHT_BG must stay identical (frontmatter maps border only)"
        );
    }
}
