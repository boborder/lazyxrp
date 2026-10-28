use super::*;

#[derive(Clone, Debug, Default)]
pub struct Styles(pub HashMap<String, Style>);

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Keep the existing on-disk `Splash` section for compatibility; styles
        // are flat internally because no second mode currently exists.
        let parsed_map = HashMap::<String, HashMap<String, String>>::deserialize(deserializer)?;
        let inner_map = parsed_map
            .into_iter()
            .find(|(mode, _)| mode.eq_ignore_ascii_case("splash"))
            .map(|(_, styles)| styles)
            .unwrap_or_default();
        let styles = inner_map
            .into_iter()
            .map(|(name, style)| (name, parse_style(&style)))
            .collect();
        Ok(Styles(styles))
    }
}

pub fn parse_style(line: &str) -> Style {
    let (foreground, background) =
        line.split_at(line.to_lowercase().find("on ").unwrap_or(line.len()));
    let foreground = extract_color_and_modifiers(foreground);
    let background = extract_color_and_modifiers(&background.replace("on ", ""));

    let mut style = Style::default();
    if let Some(fg) = parse_color(&foreground.0) {
        style = style.fg(fg);
    }
    if let Some(bg) = parse_color(&background.0) {
        style = style.bg(bg);
    }
    style = style.add_modifier(foreground.1 | background.1);
    style
}

fn extract_color_and_modifiers(color_str: &str) -> (String, Modifier) {
    let color = color_str
        .replace("grey", "gray")
        .replace("bright ", "")
        .replace("bold ", "")
        .replace("underline ", "")
        .replace("inverse ", "");

    let mut modifiers = Modifier::empty();
    if color_str.contains("underline") {
        modifiers |= Modifier::UNDERLINED;
    }
    if color_str.contains("bold") {
        modifiers |= Modifier::BOLD;
    }
    if color_str.contains("inverse") {
        modifiers |= Modifier::REVERSED;
    }

    (color, modifiers)
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim_start();
    let s = s.trim_end();
    if s.contains("bright color") {
        let s = s.trim_start_matches("bright ");
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c.checked_add(8)?))
    } else if s.contains("color") {
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("gray") {
        let c = 232u8.checked_add(
            s.trim_start_matches("gray")
                .parse::<u8>()
                .unwrap_or_default(),
        )?;
        Some(Color::Indexed(c))
    } else if let Some(rgb_str) = s.strip_prefix("rgb") {
        if rgb_str.len() >= 3 {
            let bytes = rgb_str.as_bytes();
            let digit = |i: usize| (bytes[i] as char).to_digit(10).unwrap_or_default() as u8;
            let (red, green, blue) = (digit(0), digit(1), digit(2));
            // xterm 6x6x6 cube: each channel is 0..=5
            if red > 5 || green > 5 || blue > 5 {
                return None;
            }
            let c = 16 + red * 36 + green * 6 + blue;
            Some(Color::Indexed(c))
        } else {
            None
        }
    } else if s == "bold black" {
        Some(Color::Indexed(8))
    } else if s == "bold red" {
        Some(Color::Indexed(9))
    } else if s == "bold green" {
        Some(Color::Indexed(10))
    } else if s == "bold yellow" {
        Some(Color::Indexed(11))
    } else if s == "bold blue" {
        Some(Color::Indexed(12))
    } else if s == "bold magenta" {
        Some(Color::Indexed(13))
    } else if s == "bold cyan" {
        Some(Color::Indexed(14))
    } else if s == "bold white" {
        Some(Color::Indexed(15))
    } else if s == "black" {
        Some(Color::Indexed(0))
    } else if s == "red" {
        Some(Color::Indexed(1))
    } else if s == "green" {
        Some(Color::Indexed(2))
    } else if s == "yellow" {
        Some(Color::Indexed(3))
    } else if s == "blue" {
        Some(Color::Indexed(4))
    } else if s == "magenta" {
        Some(Color::Indexed(5))
    } else if s == "cyan" {
        Some(Color::Indexed(6))
    } else if s == "white" {
        Some(Color::Indexed(7))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-019-022, TC-023: parse_style — empty yields default, fg/bg mapping, modifiers combined, gray==grey
    #[test]
    fn parse_style_spec_mapping() {
        // Empty spec yields the default style.
        assert_eq!(parse_style(""), Style::default(), "empty style spec");

        let cases = [
            ("red", Some(Color::Indexed(1)), None),
            ("on blue", None, Some(Color::Indexed(4))),
            (
                "red on blue",
                Some(Color::Indexed(1)),
                Some(Color::Indexed(4)),
            ),
        ];
        for (input, fg, bg) in cases {
            let style = parse_style(input);
            assert_eq!(style.fg, fg, "fg for {input:?}");
            assert_eq!(style.bg, bg, "bg for {input:?}");
        }

        let gray = parse_style("underline bold inverse gray");
        let grey = parse_style("underline bold inverse grey");
        assert_eq!(gray.fg, grey.fg, "gray and grey must map to the same color");
        for style in [gray, grey] {
            assert!(style.add_modifier.contains(Modifier::UNDERLINED));
            assert!(style.add_modifier.contains(Modifier::BOLD));
            assert!(style.add_modifier.contains(Modifier::REVERSED));
            assert!(style.fg.is_some());
        }
    }

    /// TC-024/TC-025: parse_color — RGB shorthand → indexed palette, unknown → None
    #[test]
    fn parse_color_contract() {
        let color = parse_color("rgb123");
        let expected = 16 + 36 + 2 * 6 + 3;
        assert_eq!(color, Some(Color::Indexed(expected)));
        assert_eq!(parse_color("unknown"), None);
        // Out-of-range user values must not overflow u8 (panic in debug, wrap in release).
        assert_eq!(parse_color("rgb555"), Some(Color::Indexed(231)));
        assert_eq!(parse_color("rgb999"), None);
        assert_eq!(parse_color("gray0"), Some(Color::Indexed(232)));
        assert_eq!(parse_color("gray23"), Some(Color::Indexed(255)));
        assert_eq!(parse_color("gray24"), None);
        assert_eq!(parse_color("bright color7"), Some(Color::Indexed(15)));
        assert_eq!(parse_color("bright color250"), None);
    }
}
