use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
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
/// 2. ロゴ領域はパネル横幅の 80% に収め、長い行は折り返し
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

fn splash_ascii_lines(tick: usize) -> Vec<Line<'static>> {
    let lines: Vec<&'static str> = ASCII_ART.trim_start_matches('\n').lines().collect();
    let n = lines.len().max(1);
    let head = (tick / 2) % n;
    lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let ring_dist = (n + i - head) % n;
            let style = match ring_dist {
                0 => theme::accent_style().add_modifier(Modifier::BOLD),
                1 => Style::new().fg(theme::TITLE).add_modifier(Modifier::BOLD),
                2 => theme::accent_style(),
                _ => theme::dim_style(),
            };
            Line::from(Span::styled(text, style))
        })
        .collect()
}

fn trailing_dots(tick: usize) -> &'static str {
    match tick % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    }
}

fn splash_wrap_width(inner_w: u16) -> u16 {
    ((inner_w as u32 * 8 / 10).max(1)) as u16
}

fn splash_art_column(art_area: Rect) -> Rect {
    let w = splash_wrap_width(art_area.width);
    let side = art_area.width.saturating_sub(w) / 2;
    Rect {
        x: art_area.x + side,
        y: art_area.y,
        width: w,
        height: art_area.height,
    }
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

        let [top_pad, content, _] =
            Layout::vertical([Constraint::Min(0), Constraint::Min(0), Constraint::Min(0)])
                .areas(inner);
        let _ = top_pad;

        let [art_area, status_area, hint_area] = Layout::vertical([
            Constraint::Min(14),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(content);

        let art_col = splash_art_column(art_area);
        let art = Paragraph::new(splash_ascii_lines(self.tick))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(art, art_col);

        let spin = spinner(self.tick);
        let server = &self.config.xrpl.rpc_server;
        let dim = theme::dim_style();
        let status_line = Line::from(vec![
            Span::styled(spin, theme::accent_style()),
            Span::styled(" Connecting to ", dim),
            Span::styled(
                server.as_deref().unwrap_or("xrplcluster.com"),
                Style::new()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::ITALIC),
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
