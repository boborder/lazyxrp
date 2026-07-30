//! Modal key handling for the wallet composer.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{ACCOUNT_SET_ROWS, FLAG_OPTIONS, WalletPanel};
use crate::action::Action;

impl WalletPanel {
    pub(super) fn account_set_edit_keys(&mut self, key: &KeyEvent) -> bool {
        if !self.is_form_editing || self.field_row < 2 {
            return false;
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                match self.field_row {
                    2 => self.domain.push(c),
                    3 if c.is_ascii_digit() => self.tick_size.push(c),
                    4 if c.is_ascii_digit() => self.transfer_rate.push(c),
                    _ => {}
                }
                true
            }
            KeyCode::Backspace => {
                match self.field_row {
                    2 => {
                        self.domain.pop();
                    }
                    3 => {
                        self.tick_size.pop();
                    }
                    4 => {
                        self.transfer_rate.pop();
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn payment_edit_keys(
        dest: &mut String,
        amt: &mut String,
        currency: &mut String,
        issuer: &mut String,
        is_iou: bool,
        row: usize,
        key: &KeyEvent,
    ) -> bool {
        // In IOU mode, rows 1 (currency) and 2 (issuer) are text fields;
        // row 3 is amount. In XRP mode, row 1 is amount.
        let target_row = if is_iou && row == 1 {
            currency
        } else if is_iou && row == 2 {
            issuer
        } else {
            amt
        };
        let is_dest = row == 0;
        let is_iou_text = is_iou && (row == 1 || row == 2);

        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let accept = if is_dest {
                    c.is_ascii_graphic()
                } else if is_iou_text && row == 1 {
                    c.is_ascii_alphabetic() && target_row.len() < 3
                } else if is_iou_text && row == 2 {
                    c.is_ascii_graphic()
                } else if !is_iou_text {
                    c.is_ascii_digit() || (c == '.' && !target_row.contains('.'))
                } else {
                    false
                };
                if accept {
                    if is_dest {
                        dest.push(c);
                    } else {
                        target_row.push(c);
                    }
                    true
                } else {
                    false
                }
            }
            KeyCode::Backspace => {
                if is_dest {
                    dest.pop();
                } else {
                    target_row.pop();
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_account_set_modal_keys(&mut self, key: KeyEvent) -> Option<Action> {
        if self.account_set_edit_keys(&key) {
            return None;
        }
        match key.code {
            KeyCode::Char('e') | KeyCode::Char('E')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.is_form_editing = !self.is_form_editing;
                return Some(Action::SetKeymapSuppression(true));
            }
            KeyCode::Char('s') | KeyCode::Char('S')
                if key.modifiers.contains(KeyModifiers::CONTROL) || !self.is_form_editing =>
            {
                return self.queue_submit_account_set();
            }
            KeyCode::Char('[') => {
                self.field_row = (self.field_row + ACCOUNT_SET_ROWS - 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Char(']') => {
                self.field_row = (self.field_row + 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Tab => {
                self.field_row = (self.field_row + 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::BackTab => {
                self.field_row = (self.field_row + ACCOUNT_SET_ROWS - 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Char(',') if self.field_row <= 1 => {
                if self.field_row == 0 {
                    self.set_flag_ix =
                        (self.set_flag_ix + FLAG_OPTIONS.len() - 1) % FLAG_OPTIONS.len();
                } else {
                    self.clear_flag_ix =
                        (self.clear_flag_ix + FLAG_OPTIONS.len() - 1) % FLAG_OPTIONS.len();
                }
            }
            KeyCode::Char('.') if self.field_row <= 1 => {
                if self.field_row == 0 {
                    self.set_flag_ix = (self.set_flag_ix + 1) % FLAG_OPTIONS.len();
                } else {
                    self.clear_flag_ix = (self.clear_flag_ix + 1) % FLAG_OPTIONS.len();
                }
            }
            _ => {}
        }
        None
    }
}
