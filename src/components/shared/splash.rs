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
    let lines: Vec<&'static str> = ASCII_ART.trim_start_matches('\n').lines().collect();
    let n = lines.len().max(1);

    // 十分な高さがない場合は下の部分だけ表示（ロゴの下半分が重要）
    let skip = if max_lines >= n {
        0
    } else {
        n.saturating_sub(max_lines)
    };
    let visible: Vec<&'static str> = lines.into_iter().skip(skip).collect();
    let vn = visible.len().max(1);
    let head = (tick / 2) % vn;

    visible
        .into_iter()
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
    command_tx: Option<UnboundedSender<Action>>,
    config: Arc<Config>,
    tick: usize,
}

impl Default for SplashScreen {
    fn default() -> Self {
        Self {
            command_tx: None,
            config: Arc::new(Config::default()),
            tick: 0,
        }
    }
}

impl Component for SplashScreen {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        self.command_tx = Some(tx);
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
