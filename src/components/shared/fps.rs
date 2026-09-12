use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Span,
    widgets::Paragraph,
};

use crate::action::Action;
use crate::components::Component;

#[derive(Debug, Clone, PartialEq)]
pub struct FpsCounter {
    last_tick_update: Instant,
    tick_count: u32,
    ticks_per_second: f64,

    last_frame_update: Instant,
    frame_count: u32,
    frames_per_second: f64,
}

impl Default for FpsCounter {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            last_tick_update: now,
            tick_count: 0,
            ticks_per_second: 0.0,
            last_frame_update: now,
            frame_count: 0,
            frames_per_second: 0.0,
        }
    }
}

impl FpsCounter {
    /// Returns `true` when the on-screen tick-rate label should refresh.
    fn app_tick(&mut self) -> bool {
        self.tick_count += 1;
        let now = Instant::now();
        let elapsed = (now - self.last_tick_update).as_secs_f64();
        if elapsed >= 1.0 {
            self.ticks_per_second = self.tick_count as f64 / elapsed;
            self.last_tick_update = now;
            self.tick_count = 0;
            return true;
        }
        false
    }

    /// Returns `true` when the on-screen FPS label should refresh.
    fn render_tick(&mut self) -> bool {
        self.frame_count += 1;
        let now = Instant::now();
        let elapsed = (now - self.last_frame_update).as_secs_f64();
        if elapsed >= 1.0 {
            self.frames_per_second = self.frame_count as f64 / elapsed;
            self.last_frame_update = now;
            self.frame_count = 0;
            return true;
        }
        false
    }

    /// Update counters; `true` if the FPS widget text changed.
    pub fn note_action(&mut self, action: &Action) -> bool {
        match action {
            Action::Tick => self.app_tick(),
            Action::Render => self.render_tick(),
            _ => false,
        }
    }
}

impl Component for FpsCounter {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let _ = self.note_action(action);
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, _] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let message = format!(
            "{:.2} ticks/sec, {:.2} FPS",
            self.ticks_per_second, self.frames_per_second
        );
        let span = Span::styled(message, Style::new().dim());
        let paragraph = Paragraph::new(span).right_aligned();
        frame.render_widget(paragraph, top);
        Ok(())
    }
}

#[cfg(test)]
impl FpsCounter {
    pub(crate) fn recorded_frames(&self) -> u32 {
        self.frame_count
    }

    pub(crate) fn recorded_tick_rate(&self) -> f64 {
        self.ticks_per_second
    }

    /// Seed an elapsed tick window so rate calculation is deterministic offline.
    pub(crate) fn seed_tick_window(&mut self, elapsed: std::time::Duration, pending_ticks: u32) {
        self.last_tick_update = Instant::now() - elapsed;
        self.tick_count = pending_ticks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-103: FPS overlay counts actual draws, not ticks alone.
    #[test]
    fn render_counts_frames_but_tick_alone_does_not() {
        let mut fps = FpsCounter::default();
        assert!(!fps.note_action(&Action::Tick));
        assert_eq!(fps.recorded_frames(), 0);
        assert!(!fps.note_action(&Action::Render));
        assert_eq!(fps.recorded_frames(), 1);
    }

    /// TC-104: tick-rate label refreshes after a one-second measurement window.
    #[test]
    fn tick_rate_updates_after_one_second_window() {
        let mut fps = FpsCounter::default();
        fps.seed_tick_window(std::time::Duration::from_secs(2), 7);
        assert!(fps.note_action(&Action::Tick));
        assert!((fps.recorded_tick_rate() - 4.0).abs() < 0.01);
    }
}
