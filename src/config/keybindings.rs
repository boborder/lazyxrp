use super::*;

/// Keys may only trigger payload-free navigation/refresh actions. Anything carrying data
/// (incl. every `*Submit` write) must come from a UI flow with its own confirmation.
fn is_bindable(action: &Action) -> bool {
    matches!(
        action,
        Action::Quit
            | Action::Suspend
            | Action::ClearScreen
            | Action::Render
            | Action::Help
            | Action::RefreshAccount
            | Action::RefreshBook
            | Action::RefreshNfts
            | Action::RefreshLines
            | Action::RefreshTxHistory
            | Action::RefreshLedgerObjects
            | Action::TabNext
            | Action::TabPrev
            | Action::FocusNext
            | Action::FocusPrev
            | Action::SelectNext
            | Action::SelectPrev
            | Action::NetworkSwitchCycle
            | Action::TxDetailToggle
    )
}

#[derive(Clone, Debug, Default)]
pub struct KeyBindings(pub HashMap<Vec<KeyEvent>, Action>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> color_eyre::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Keep the existing on-disk `Splash` section for compatibility while
        // storing one flat map internally until another mode is implemented.
        let parsed_map = HashMap::<String, HashMap<String, Action>>::deserialize(deserializer)?;
        let inner_map = parsed_map
            .into_iter()
            .find(|(mode, _)| mode.eq_ignore_ascii_case("splash"))
            .map(|(_, bindings)| bindings)
            .unwrap_or_default();
        let mut keybindings = HashMap::new();
        for (key_str, cmd) in inner_map {
            if !is_bindable(&cmd) {
                return Err(D::Error::custom(format!(
                    "keybinding \"{key_str}\": action {cmd:?} cannot be bound to a key \
                     (only payload-free navigation/refresh actions are allowed)"
                )));
            }
            let seq = parse_key_sequence(&key_str).map_err(D::Error::custom)?;
            keybindings.insert(seq, cmd);
        }
        Ok(KeyBindings(keybindings))
    }
}

fn parse_key_event(raw: &str) -> color_eyre::Result<KeyEvent, String> {
    let raw_lower = raw.to_ascii_lowercase();
    let (remaining, modifiers) = extract_modifiers(&raw_lower);
    parse_key_code_with_modifiers(remaining, modifiers)
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        match current {
            rest if rest.starts_with("ctrl-") => {
                modifiers.insert(KeyModifiers::CONTROL);
                current = &rest[5..];
            }
            rest if rest.starts_with("alt-") => {
                modifiers.insert(KeyModifiers::ALT);
                current = &rest[4..];
            }
            rest if rest.starts_with("shift-") => {
                modifiers.insert(KeyModifiers::SHIFT);
                current = &rest[6..];
            }
            _ => break,
        };
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    raw: &str,
    mut modifiers: KeyModifiers,
) -> color_eyre::Result<KeyEvent, String> {
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            // SAFETY: c.len() == 1 guarantees at least one char exists.
            let mut c = c
                .chars()
                .next()
                .expect("single-byte key should have a char");
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

pub fn parse_key_sequence(raw: &str) -> color_eyre::Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{}`", raw));
    }
    let raw = if !raw.contains("><") {
        let without_open = raw.strip_prefix('<').unwrap_or(raw);
        without_open.strip_prefix('>').unwrap_or(without_open)
    } else {
        raw
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-027-029/TC-031/TC-032: parse_key_event — modifier composition, case-insensitivity, unknown keys error
    #[test]
    fn parse_key_event_contract() {
        let cases = [
            ("a", Some((KeyCode::Char('a'), KeyModifiers::empty()))),
            ("enter", Some((KeyCode::Enter, KeyModifiers::empty()))),
            ("esc", Some((KeyCode::Esc, KeyModifiers::empty()))),
            ("ctrl-a", Some((KeyCode::Char('a'), KeyModifiers::CONTROL))),
            ("alt-enter", Some((KeyCode::Enter, KeyModifiers::ALT))),
            ("shift-esc", Some((KeyCode::Esc, KeyModifiers::SHIFT))),
            ("CTRL-a", Some((KeyCode::Char('a'), KeyModifiers::CONTROL))),
            ("AlT-eNtEr", Some((KeyCode::Enter, KeyModifiers::ALT))),
            (
                "ctrl-alt-a",
                Some((
                    KeyCode::Char('a'),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                )),
            ),
            (
                "ctrl-shift-enter",
                Some((KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
            ),
            ("shift-a", Some((KeyCode::Char('A'), KeyModifiers::SHIFT))),
            ("backtab", Some((KeyCode::BackTab, KeyModifiers::SHIFT))),
            ("f1", Some((KeyCode::F(1), KeyModifiers::empty()))),
            ("f12", Some((KeyCode::F(12), KeyModifiers::empty()))),
            ("space", Some((KeyCode::Char(' '), KeyModifiers::empty()))),
            ("f13", None),
            ("invalid-key", None),
            ("ctrl-invalid-key", None),
        ];
        for (input, expected) in cases {
            let parsed = parse_key_event(input)
                .ok()
                .map(|ev| (ev.code, ev.modifiers));
            assert_eq!(parsed, expected, "key event for {input:?}");
        }
    }

    /// TC-036
    #[test]
    fn parse_key_sequence_unbalanced_brackets_errors() {
        assert!(parse_key_sequence("<<q>").is_err());
    }
}
