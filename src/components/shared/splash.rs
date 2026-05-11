use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
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

        // vertical: pad top, art (14 lines), status (1 line), hint (1 line), pad bottom
        let [top_pad, content, _] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(16),
            Constraint::Min(0),
        ])
        .areas(inner);
        let _ = top_pad;

        let [art_area, status_area, hint_area] = Layout::vertical([
            Constraint::Length(14),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(content);

        let [_, art_centered, _] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(57),
            Constraint::Min(0),
        ])
        .areas(art_area);
        let art = Paragraph::new(ASCII_ART)
            .alignment(Alignment::Left)
            .style(theme::accent_style().add_modifier(Modifier::BOLD));
        frame.render_widget(art, art_centered);

        let spin = spinner(self.tick);
        let server = &self.config.xrpl.rpc_server;
        let status_line = Line::from(vec![
            Span::styled(spin, theme::accent_style()),
            Span::styled(" Connecting to ", theme::dim_style()),
            Span::styled(
                server.as_deref().unwrap_or("xrplcluster.com"),
                Style::new()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(" ...", theme::dim_style()),
        ]);
        frame.render_widget(
            Paragraph::new(status_line).alignment(Alignment::Center),
            status_area,
        );

        let hint = Paragraph::new("press q to quit")
            .alignment(Alignment::Center)
            .style(theme::dim_style());
        frame.render_widget(hint, hint_area);
        Ok(())
    }
}
