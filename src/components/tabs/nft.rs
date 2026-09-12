use std::sync::mpsc::{self, Receiver};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Paragraph, Row, Table},
};
use ratatui_image::{
    Resize, StatefulImage,
    errors::Errors,
    picker::Picker,
    thread::{ResizeRequest, ResizeResponse, ThreadProtocol},
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

fn spawn_image_worker() -> (ThreadProtocol, Receiver<Result<ResizeResponse, Errors>>) {
    let (worker_tx, worker_rx) = mpsc::channel::<ResizeRequest>();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(request) = worker_rx.recv() {
            let _ = result_tx.send(request.resize_encode());
        }
    });
    (ThreadProtocol::new(worker_tx, None), result_rx)
}

#[derive(Default)]
pub struct NftTab {
    nfts: Vec<NftRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
    picker: Option<Picker>,
    image_state: Option<ThreadProtocol>,
    image_result_rx: Option<Receiver<Result<ResizeResponse, Errors>>>,
    requested_nft_id: Option<String>,
    image_loading: bool,
    image_error: Option<String>,
}

impl NftTab {
    fn request_selected_image(&mut self) -> Option<Action> {
        let nft = self
            .table_state
            .selected()
            .and_then(|index| self.nfts.get(index))?;
        if self.requested_nft_id.as_deref() == Some(nft.nft_id.as_str()) {
            return None;
        }
        self.requested_nft_id = Some(nft.nft_id.clone());
        if let Some(state) = self.image_state.as_mut() {
            state.empty_protocol();
        }
        self.image_error = None;
        self.image_loading = !nft.uri.is_empty();
        (!nft.uri.is_empty()).then(|| Action::NftImageRequest {
            nft_id: nft.nft_id.clone(),
            uri: nft.uri.clone(),
        })
    }

    fn poll_image_resize(&mut self) {
        let Some(rx) = self.image_result_rx.as_ref() else {
            return;
        };
        while let Ok(result) = rx.try_recv() {
            match result {
                Ok(response) => {
                    if self
                        .image_state
                        .as_mut()
                        .is_some_and(|state| state.update_resized_protocol(response))
                    {
                        self.image_loading = false;
                        self.image_error = None;
                    }
                }
                Err(error) => {
                    self.image_error = Some(error.to_string());
                    self.image_loading = false;
                }
            }
        }
    }

    fn handle_image_loaded(&mut self, nft_id: &str, bytes: &[u8]) {
        if self.requested_nft_id.as_deref() != Some(nft_id) {
            return;
        }
        let Some(picker) = self.picker.as_ref() else {
            self.image_error = Some("image picker not initialized".to_owned());
            self.image_loading = false;
            return;
        };
        let dyn_img = match image::load_from_memory(bytes) {
            Ok(image) => image,
            Err(error) => {
                self.image_error = Some(error.to_string());
                self.image_loading = false;
                return;
            }
        };
        let protocol = picker.new_resize_protocol(dyn_img);
        if let Some(state) = self.image_state.as_mut() {
            state.replace_protocol(protocol);
            self.image_loading = true;
            self.image_error = None;
        }
    }

    fn has_preview_protocol(&self) -> bool {
        self.image_state
            .as_ref()
            .and_then(|state| state.protocol_type())
            .is_some()
    }

    fn draw_preview(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title(" Preview ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let show_image = self.has_preview_protocol() && self.image_error.is_none();
        if show_image && let Some(state) = self.image_state.as_mut() {
            frame.render_stateful_widget(
                StatefulImage::new().resize(Resize::Fit(None)),
                inner,
                state,
            );
        }

        if !show_image || self.image_loading {
            let message = self
                .image_error
                .as_deref()
                .or_else(|| self.image_loading.then_some("loading image…"))
                .unwrap_or("select NFT with image URI");
            frame.render_widget(Paragraph::new(message).style(theme::dim_style()), inner);
        }
    }
}

impl Component for NftTab {
    fn init(&mut self, _area: ratatui::layout::Size) -> color_eyre::Result<()> {
        self.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        let (state, result_rx) = spawn_image_worker();
        self.image_state = Some(state);
        self.image_result_rx = Some(result_rx);
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let len = self.nfts.len();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.nfts.get(idx))
            .map(|nft| (nft.raw_json.clone(), ArcValue::default()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
        }

        match action {
            Action::Tick => {
                self.tick = self.tick.wrapping_add(1);
                self.poll_image_resize();
            }
            Action::XrplAccountNfts(nfts) => {
                self.nfts = nfts.to_vec();
                self.table_state.reset_len(self.nfts.len());
                self.received = true;
                return Ok(self.request_selected_image());
            }
            _a if self
                .table_state
                .handle_row_select(action, self.is_focused, self.nfts.len()) =>
            {
                return Ok(self.request_selected_image());
            }
            Action::NftImageLoaded { nft_id, bytes } => {
                self.handle_image_loaded(nft_id, bytes.as_ref());
            }
            Action::NftImageError { nft_id, message }
                if self.requested_nft_id.as_deref() == Some(nft_id) =>
            {
                self.image_error = Some(message.clone());
                self.image_loading = false;
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
        let [table_area, preview_area] =
            Layout::horizontal([Constraint::Percentage(65), Constraint::Fill(1)]).areas(inner);

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
                dnft.to_owned(),
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
        render_selectable_table(
            frame,
            table_area,
            table,
            &mut self.table_state,
            self.is_focused,
        );
        self.draw_preview(frame, preview_area);
        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_nft(id: &str, uri: &str) -> NftRow {
        NftRow {
            nft_id: id.to_string(),
            taxon: 0,
            serial: 1,
            transfer_fee: 0,
            uri: uri.to_string(),
            is_mutable: false,
            raw_json: ArcValue::default(),
        }
    }

    fn focused_tab() -> color_eyre::Result<NftTab> {
        let mut tab = NftTab {
            is_focused: true,
            ..NftTab::default()
        };
        tab.init(ratatui::layout::Size::new(80, 24))?;
        Ok(tab)
    }

    /// TC-109: selected NFT with URI emits `NftImageRequest`.
    #[test]
    fn account_nfts_with_uri_emits_image_request() -> color_eyre::Result<()> {
        let mut tab = focused_tab()?;
        let action = tab.update(&Action::XrplAccountNfts(vec![sample_nft(
            "NFT_A",
            "ipfs://image-a",
        )]))?;
        assert_eq!(
            action,
            Some(Action::NftImageRequest {
                nft_id: "NFT_A".into(),
                uri: "ipfs://image-a".into(),
            })
        );
        Ok(())
    }

    /// TC-110: NFT without URI emits no preview request.
    #[test]
    fn account_nfts_without_uri_emits_no_image_request() -> color_eyre::Result<()> {
        let mut tab = focused_tab()?;
        let action = tab.update(&Action::XrplAccountNfts(vec![sample_nft("NFT_B", "")]))?;
        assert_eq!(action, None);
        Ok(())
    }

    #[test]
    fn select_next_emits_image_request_for_newly_selected_nft() -> color_eyre::Result<()> {
        let mut tab = focused_tab()?;
        tab.update(&Action::XrplAccountNfts(vec![
            sample_nft("NFT_A", "ipfs://a"),
            sample_nft("NFT_B", "ipfs://b"),
        ]))?;
        let action = tab.update(&Action::SelectNext)?;
        assert_eq!(
            action,
            Some(Action::NftImageRequest {
                nft_id: "NFT_B".into(),
                uri: "ipfs://b".into(),
            })
        );
        Ok(())
    }

    #[test]
    fn duplicate_selection_emits_no_image_request() -> color_eyre::Result<()> {
        let mut tab = focused_tab()?;
        tab.update(&Action::XrplAccountNfts(vec![sample_nft(
            "NFT_A", "ipfs://a",
        )]))?;
        let action = tab.update(&Action::XrplAccountNfts(vec![sample_nft(
            "NFT_A", "ipfs://a",
        )]))?;
        assert_eq!(action, None);
        Ok(())
    }
}
