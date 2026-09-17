use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::components::Component;
use crate::{
    action::Action,
    components::shared::{theme, widgets::spinner},
    config::Config,
};

/// Splash の tick 駆動アニメ
/// 1. ASCII ロゴの行ごとウェーブ（強調が縦に伝わる）
/// 2. ロゴは「中央上部」に固定配置（横中央 + 上寄せ）
/// 3. ステータス末尾のドット呼吸（`. .. ...`）
/// 4. quit ヒントの括弧スタイル周期 `[ ]` / `< >` / `( )`
const ASCII_ART: &str = r#"
         _                   __  ______  ____
        | |    __ _ _____   _\ \/ /  _ \|  _ \
        | |   / _` |_  / | | |\  /| |_) | |_) |
        | |__| (_| |/ /| |_| |/  \|  _ <|  __/
        |_____\__,_/___|\__, /_/\_\_| \_\_|
                        |___/
 __  ______  ____    _             _
 \ \/ /  _ \|  _ \  | |    ___  __| | __ _  ___ _ ___
  \  /| |_) | |_) | | |   / _ \/ _` |/ _` |/ _ \ '__|
  /  \|  _ <|  __/  | |__|  __/ (_| | (_| |  __/ |
 /_/\_\_| \_\_|     |_____\___|\__,_|\__, |\___|_|
                                     |___/
"#;

fn trailing_dots(tick: usize) -> &'static str {
    match tick % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    }
}

fn ascii_art_max_width() -> u16 {
    ASCII_ART
        .trim_start_matches('\n')
        .lines()
        .map(|l| l.chars().count() as u16)
        .max()
        .unwrap_or(1)
}

fn splash_art_column(art_area: Rect) -> Rect {
    // ASCII の崩れを避けるため、折り返し前提ではなく「必要幅を中央配置」。
    let w = ascii_art_max_width().min(art_area.width.max(1));
    let side = art_area.width.saturating_sub(w) / 2;
    Rect {
        x: art_area.x + side,
        y: art_area.y,
        width: w,
        height: art_area.height,
    }
}

/// 利用可能な高さに応じて ASCII アートの表示行を調整
fn splash_ascii_lines_for_height(tick: usize, max_lines: usize) -> Vec<Line<'static>> {
    let lines = ASCII_ART.trim_start_matches('\n').lines();
    let n = lines.clone().count();

    // 十分な高さがない場合は下の部分だけ表示（ロゴの下半分が重要）
    let skip = n.saturating_sub(max_lines);
    let vn = (n - skip).max(1);
    let head = (tick / 2) % vn;

    lines
        .skip(skip)
        .enumerate()
        .map(|(i, text)| {
            let ring_dist = (vn + i - head) % vn;
            let style = match ring_dist {
                0 => theme::accent_style().bold(),
                1 => Style::new().fg(theme::TITLE).bold(),
                2 => theme::accent_style(),
                _ => theme::dim_style(),
            };
            Line::from(Span::styled(text, style))
        })
        .collect()
}

fn quit_hint_line(tick: usize) -> Line<'static> {
    let frame = (tick / 8) % 3;
    let (open, close) = match frame {
        0 => ("[ ", " ]"),
        1 => ("< ", " >"),
        _ => ("( ", " )"),
    };
    let dim = theme::dim_style();
    Line::from(vec![
        Span::styled(open, dim),
        Span::styled("press q to quit", dim),
        Span::styled(close, dim),
    ])
}

pub struct SplashScreen {
    config: Arc<Config>,
    tick: usize,
}

impl Default for SplashScreen {
    fn default() -> Self {
        Self {
            config: Arc::new(Config::default()),
            tick: 0,
        }
    }
}

impl Component for SplashScreen {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        if let Action::Tick = action {
            self.tick = self.tick.wrapping_add(1);
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = theme::panel_block("LazyXRP", true);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // 小さいウィンドウ対応：最低限 status + hint は必ず表示
        const STATUS_H: u16 = 1;
        const HINT_H: u16 = 1;
        const ART_FULL: u16 = 14;
        let min_needed = STATUS_H + HINT_H;

        // 「中央上部」に固定：上寄せで表示し、残りは下へ流す
        let art_h = if inner.height >= ART_FULL + min_needed {
            ART_FULL
        } else {
            inner.height.saturating_sub(min_needed)
        };
        let total_needed = art_h + min_needed;

        let [content, _] =
            Layout::vertical([Constraint::Length(total_needed), Constraint::Min(0)]).areas(inner);

        let [art_area, status_area, hint_area] = Layout::vertical([
            Constraint::Length(art_h),
            Constraint::Length(STATUS_H),
            Constraint::Length(HINT_H),
        ])
        .areas(content);

        let art_col = splash_art_column(art_area);
        let art_lines = splash_ascii_lines_for_height(self.tick, art_h as usize);
        let art = Paragraph::new(art_lines).alignment(Alignment::Left);
        frame.render_widget(art, art_col);

        let spin = spinner(self.tick);
        let server = &self.config.xrpl.rpc_server;
        let dim = theme::dim_style();
        let status_line = Line::from(vec![
            Span::styled(spin, theme::accent_style()),
            Span::styled(" Connecting to ", dim),
            Span::styled(
                server.as_deref().unwrap_or("xrplcluster.com"),
                Style::new().fg(theme::ACCENT).italic(),
            ),
            Span::styled(trailing_dots(self.tick), dim),
        ]);
        frame.render_widget(
            Paragraph::new(status_line).alignment(Alignment::Center),
            status_area,
        );

        let hint = Paragraph::new(quit_hint_line(self.tick)).alignment(Alignment::Center);
        frame.render_widget(hint, hint_area);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art_lines() -> Vec<&'static str> {
        ASCII_ART.trim_start_matches('\n').lines().collect()
    }

    #[test]
    fn trailing_dots_cycles_through_empty_one_two_three_dots() {
        assert_eq!(trailing_dots(0), "");
        assert_eq!(trailing_dots(1), ".");
        assert_eq!(trailing_dots(2), "..");
        assert_eq!(trailing_dots(3), "...");
        assert_eq!(trailing_dots(4), "");
        assert_eq!(trailing_dots(7), "...");
    }

    #[test]
    fn splash_art_column_centers_art_width_in_wide_area() {
        let col = splash_art_column(Rect::new(0, 5, 120, 12));
        let art_w = ascii_art_max_width();
        assert_eq!(col.width, art_w, "wide area must show the full art width");
        assert_eq!(
            col.x,
            (120 - art_w) / 2,
            "art must be centered horizontally"
        );
        assert_eq!((col.y, col.height), (5, 12), "y/height must pass through");
    }

    #[test]
    fn splash_art_column_fills_narrow_area_without_centering() {
        let col = splash_art_column(Rect::new(3, 0, 10, 8));
        assert_eq!(
            (col.x, col.width, col.y, col.height),
            (3, 10, 0, 8),
            "narrow area must be used as-is, no centering"
        );
    }

    #[test]
    fn splash_art_column_keeps_single_column_when_area_has_no_width() {
        let col = splash_art_column(Rect::new(4, 2, 0, 6));
        assert_eq!(
            (col.x, col.width, col.height),
            (4, 1, 6),
            "zero-width area must clamp to one column"
        );
    }

    #[test]
    fn splash_ascii_lines_show_every_art_line_when_height_allows() {
        for tick in [0usize, 5, 9] {
            let lines = splash_ascii_lines_for_height(tick, 12);
            let art = art_lines();
            assert_eq!(lines.len(), art.len(), "full height must show all lines");
            for (line, expected) in lines.iter().zip(art) {
                assert_eq!(line.spans[0].content.to_string(), expected);
            }
        }
    }

    #[test]
    fn splash_ascii_lines_show_bottom_lines_when_height_is_tight() {
        let art = art_lines();
        let lines = splash_ascii_lines_for_height(0, 5);
        assert_eq!(lines.len(), 5, "tight height must show exactly 5 lines");
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(
                line.spans[0].content.to_string(),
                art[art.len() - 5 + i],
                "tight height must keep the bottom (logo lower half)"
            );
        }
    }
}
