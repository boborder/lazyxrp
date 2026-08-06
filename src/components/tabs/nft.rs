use std::sync::mpsc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect, Size},
    widgets::{Block, Paragraph, Row, Table},
};
use ratatui_image::{Image, Resize, picker::Picker, protocol::Protocol};

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
    picker: Option<Picker>,
    image_bytes: Option<Vec<u8>>,
    image_protocol: Option<Protocol>,
    image_rx: Option<mpsc::Receiver<Result<Protocol, String>>>,
    image_nft_id: Option<String>,
    requested_nft_id: Option<String>,
    image_size: Size,
    image_loading: bool,
    image_error: Option<String>,
    action_tx: Option<tokio::sync::mpsc::UnboundedSender<Action>>,
}

impl NftTab {
    pub fn new() -> Self {
        Self {
            is_focused: true,
            ..Self::default()
        }
    }

    fn request_selected_image(&mut self) -> Option<Action> {
        let nft = self
            .table_state
            .selected()
            .and_then(|index| self.nfts.get(index))?;
        if self.requested_nft_id.as_deref() == Some(nft.nft_id.as_str()) {
            return None;
        }
        self.image_rx = None;
        self.requested_nft_id = Some(nft.nft_id.clone());
        self.image_nft_id = Some(nft.nft_id.clone());
        self.image_bytes = None;
        self.image_protocol = None;
        self.image_error = None;
        self.image_loading = !nft.uri.is_empty();
        (!nft.uri.is_empty()).then(|| Action::NftImageRequest {
            nft_id: nft.nft_id.clone(),
            uri: nft.uri.clone(),
        })
    }

    fn poll_image_result(&mut self) {
        let Some(rx) = self.image_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(protocol)) => {
                self.image_protocol = Some(protocol);
                self.image_loading = false;
            }
            Ok(Err(error)) => {
                self.image_error = Some(error);
                self.image_loading = false;
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.image_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.image_error = Some("image worker stopped".to_owned());
                self.image_loading = false;
            }
        }
    }

    fn start_image_encode(&mut self, size: Size) {
        let (Some(picker), Some(bytes)) = (self.picker.clone(), self.image_bytes.clone()) else {
            return;
        };
        if size.width == 0 || size.height == 0 || self.image_rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.image_rx = Some(rx);
        self.image_loading = true;
        let image_nft_id = self.image_nft_id.clone();
        let action_tx = self.action_tx.clone();
        std::thread::spawn(move || {
            let result = image::load_from_memory(&bytes)
                .map_err(|error| error.to_string())
                .and_then(|image| {
                    picker
                        .new_protocol(image, size, Resize::Fit(None))
                        .map_err(|error| error.to_string())
                });
            let _ = tx.send(result);
            if let (Some(action_tx), Some(nft_id)) = (action_tx, image_nft_id) {
                let _ = action_tx.send(Action::NftImageReady { nft_id });
            }
        });
    }

    fn handle_image_loaded(&mut self, nft_id: &str, bytes: Vec<u8>) {
        if self.requested_nft_id.as_deref() != Some(nft_id) {
            return;
        }
        self.image_bytes = Some(bytes);
        self.image_protocol = None;
        self.image_error = None;
        self.image_loading = true;
        self.start_image_encode(self.image_size);
    }

    fn draw_preview(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title(" Preview ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let size = Size::new(inner.width, inner.height);
        if size != self.image_size {
            self.image_size = size;
            self.image_protocol = None;
            self.start_image_encode(size);
        }
        if let Some(protocol) = self.image_protocol.as_ref() {
            frame.render_widget(Image::new(protocol), inner);
        } else {
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
    fn init(&mut self, _area: Size) -> color_eyre::Result<()> {
        self.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        Ok(())
    }

    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.action_tx = Some(action_tx);
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        if self.detail.visible {
            match action {
                Action::TxDetailToggle => self.detail.close(),
                Action::SelectNext | Action::FocusNext => {
                    self.detail.scroll = self.detail.scroll.saturating_add(1)
                }
                Action::SelectPrev | Action::FocusPrev => {
                    self.detail.scroll = self.detail.scroll.saturating_sub(1)
                }
                Action::Quit => {}
                _ => {}
            }
            return Ok(None);
        }

        match action {
            Action::Tick => {
                self.tick = self.tick.wrapping_add(1);
                self.poll_image_result();
            }
            Action::XrplAccountNfts(nfts) => {
                self.nfts = nfts.to_vec();
                self.table_state.reset_len(self.nfts.len());
                self.received = true;
                return Ok(self.request_selected_image());
            }
            Action::SelectNext if !self.nfts.is_empty() && self.is_focused => {
                self.table_state.select_next(self.nfts.len());
                return Ok(self.request_selected_image());
            }
            Action::SelectPrev if !self.nfts.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.nfts.len());
                return Ok(self.request_selected_image());
            }
            Action::NftImageLoaded { nft_id, bytes } => {
                self.handle_image_loaded(nft_id, bytes.clone());
            }
            Action::NftImageError { nft_id, message }
                if self.requested_nft_id.as_deref() == Some(nft_id) =>
            {
                self.image_error = Some(message.clone());
                self.image_loading = false;
            }
            Action::NftImageReady { nft_id }
                if self.requested_nft_id.as_deref() == Some(nft_id) =>
            {
                self.poll_image_result();
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
