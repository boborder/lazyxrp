use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::{ArcValue, NftRow},
};

#[derive(Default)]
pub struct NftTab {
    nfts: Vec<NftRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
}

impl NftTab {
    pub fn new() -> Self {
        Self {
            is_focused: true, // NftTab is standalone, always focused
            ..Self::default()
        }
    }
}

impl Component for NftTab {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        if self.detail.visible {
            match action {
                Action::TxDetailToggle => {
                    self.detail.close();
                    return Ok(None);
                }
                Action::SelectNext | Action::FocusNext => {
                    self.detail.scroll = self.detail.scroll.saturating_add(1);
                    return Ok(None);
                }
                Action::SelectPrev | Action::FocusPrev => {
                    self.detail.scroll = self.detail.scroll.saturating_sub(1);
                    return Ok(None);
                }
                Action::Quit => return Ok(None),
                _ => return Ok(None),
            }
        }
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
            Action::TxDetailToggle if self.is_focused && !self.nfts.is_empty() => {
                if let Some(idx) = self.table_state.selected()
                    && let Some(nft) = self.nfts.get(idx)
                {
                    self.detail.open(nft.raw_json.clone(), ArcValue::default());
                }
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
        .header(header);

        render_selectable_table(frame, inner, table, &mut self.table_state, self.is_focused);

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}
