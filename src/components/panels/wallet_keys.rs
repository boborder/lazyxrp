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

    pub(super) fn account_set_modal_key_to_action(&mut self, key: KeyEvent) -> Option<Action> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    /// TC-130: payment_edit_keys filter table — destination is ascii_graphic
    /// only, IOU currency caps at 3 alphabetic chars, amounts accept digits and
    /// at most one '.' (row 3 in IOU mode).
    #[test]
    fn payment_edit_keys_filter_table() {
        // (field, is_iou, row, key, initial, expected_accept, expected_after)
        let cases: &[(&str, bool, usize, char, &str, bool, &str)] = &[
            ("dest", false, 0, 'a', "", true, "a"),
            ("dest", false, 0, ' ', "", false, ""),
            ("dest", false, 0, 'あ', "", false, ""),
            ("dest", true, 0, 'a', "rX", true, "rXa"),
            ("amt", false, 1, '1', "", true, "1"),
            ("amt", false, 1, 'x', "", false, ""),
            ("amt", false, 1, '.', "1.", false, "1."),
            ("cur", true, 1, 'U', "", true, "U"),
            ("cur", true, 1, '4', "", false, ""),
            ("cur", true, 1, 'd', "US", true, "USd"),
            ("cur", true, 1, 'd', "USD", false, "USD"),
            ("iss", true, 2, 'r', "", true, "r"),
            ("iss", true, 2, ' ', "", false, ""),
            ("amt", true, 3, '.', "1.", false, "1."),
            ("amt", true, 3, '5', "1", true, "15"),
        ];
        for (field, is_iou, row, c, initial, accept, after) in cases {
            let mut dest = String::new();
            let mut amt = String::new();
            let mut currency = String::new();
            let mut issuer = String::new();
            match *field {
                "dest" => dest.push_str(initial),
                "amt" => amt.push_str(initial),
                "cur" => currency.push_str(initial),
                _ => issuer.push_str(initial),
            }
            let got = WalletPanel::payment_edit_keys(
                &mut dest,
                &mut amt,
                &mut currency,
                &mut issuer,
                *is_iou,
                *row,
                &key(*c),
            );
            let result = match *field {
                "dest" => dest.as_str(),
                "amt" => amt.as_str(),
                "cur" => currency.as_str(),
                _ => issuer.as_str(),
            };
            assert_eq!(got, *accept, "{field} row={row} c={c}");
            assert_eq!(result, *after, "{field} row={row} c={c}");
        }
    }

    /// TC-131: account_set_edit_keys only edits field_row 2-4 while editing;
    /// tick_size / transfer_rate accept digits only, domain accepts any char.
    #[test]
    fn account_set_edit_keys_contract() {
        let mut panel = WalletPanel::new(false);
        panel.open_account_set_composer();
        panel.is_form_editing = true;

        // Flag rows 0/1 are navigated, not typed into.
        panel.field_row = 0;
        assert!(!panel.account_set_edit_keys(&key('a')));
        panel.field_row = 1;
        assert!(!panel.account_set_edit_keys(&key('a')));
        assert!(panel.domain.is_empty());

        // Domain row accepts arbitrary characters.
        panel.field_row = 2;
        assert!(panel.account_set_edit_keys(&key('a')));
        assert_eq!(panel.domain, "a");

        // Numeric rows reject non-digits but still report "editing".
        panel.field_row = 3;
        assert!(panel.account_set_edit_keys(&key('x')));
        assert!(panel.tick_size.is_empty());
        assert!(panel.account_set_edit_keys(&key('7')));
        assert_eq!(panel.tick_size, "7");

        panel.field_row = 4;
        assert!(panel.account_set_edit_keys(&key('2')));
        assert_eq!(panel.transfer_rate, "2");
        assert!(
            panel.account_set_edit_keys(&KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()))
        );
        assert!(panel.transfer_rate.is_empty());

        // Not editing → no field mutation.
        panel.is_form_editing = false;
        panel.field_row = 3;
        assert!(!panel.account_set_edit_keys(&key('1')));
    }
}
