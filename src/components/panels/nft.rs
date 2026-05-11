use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::SelectableTableState,
            theme,
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::NftRow,
};

#[derive(Default)]
pub struct NftPanel {
    nfts: Vec<NftRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
}

impl NftPanel {
    pub fn new() -> Self {
        Self {
            is_focused: true, // NftPanel is standalone, always focused
            ..Self::default()
        }
    }
}

impl Component for NftPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplAccountNfts(nfts) => {
                self.nfts = nfts.to_vec();
                self.table_state.reset_len(self.nfts.len());
                self.received = true;
            }
            Action::SelectNext if !self.nfts.is_empty() && self.is_focused => {
                self.table_state.select_next(self.nfts.len());
            }
            Action::SelectPrev if !self.nfts.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.nfts.len());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.received {
            render_loading(
                frame,
                area,
                "NFTs",
                self.tick,
                "loading NFTs...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.nfts.is_empty() {
            render_empty(frame, area, "NFTs", "(no NFTs)", self.is_focused);
            return Ok(());
        }
        let block = titled_block_with_count(
            "NFTs",
            self.table_state.selected(),
            self.nfts.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let header = Row::new(vec!["NFTokenID", "dNFT", "Taxon", "Serial", "Fee", "URI"])
            .style(theme::header_row_style());
        let rows = self.nfts.iter().map(|n| {
            let short_id = if n.nft_id.chars().count() > 16 {
                format!("{}…", n.nft_id.chars().take(16).collect::<String>())
            } else {
                n.nft_id.clone()
            };
            let short_uri = if n.uri.chars().count() > 40 {
                format!("{}…", n.uri.chars().take(40).collect::<String>())
            } else {
                n.uri.clone()
            };
            let dnft = if n.is_mutable { "yes" } else { "no" };
            Row::new(vec![
                short_id,
                dnft.to_string(),
                n.taxon.to_string(),
                n.serial.to_string(),
                n.transfer_fee.to_string(),
                short_uri,
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(18),
                Constraint::Length(4),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        let [tbl_area, sb_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

        frame.render_stateful_widget(table, tbl_area, self.table_state.table_mut());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(theme::dim_style())
                .thumb_style(theme::accent_style()),
            sb_area,
            self.table_state.scroll_mut(),
        );
        Ok(())
    }
}
