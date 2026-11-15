use crate::modes::{Mode, State};
use crossterm::style::{Attribute, Color, Stylize};

// Colors come from the terminal's own ANSI palette, so the TUI follows whatever theme it opens in.
pub struct Theme {
    accent: Color,
    mode_fg: Color,
    muted: Color,
    select: Color,
}

impl Theme {
    pub fn terminal() -> Self {
        Theme {
            accent: Color::Cyan,
            mode_fg: Color::Black,
            muted: Color::DarkGrey,
            select: Color::Blue,
        }
    }
}

fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Normal => Color::Green,
        Mode::Insert => Color::Yellow,
        Mode::Command => Color::Magenta,
    }
}

fn fit(text: &str, width: usize) -> String {
    let n = text.chars().count();
    if n <= width {
        format!("{text}{}", " ".repeat(width - n))
    } else {
        let head: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

pub fn render(items: &[String], s: &State, width: usize, height: usize, t: &Theme) -> String {
    let inner = width.saturating_sub(4).max(1);
    let rows = height.saturating_sub(5).max(1);
    let start = s.cursor.saturating_sub(rows - 1);
    let rule = "─".repeat(width.saturating_sub(2));
    let mut out = String::new();

    let title = format!(" {}  roles ", "◆ FLURRY".with(t.accent).bold());
    out += &format!("{title}\r\n");
    out += &format!("{}\r\n", format!("╭{rule}╮").with(t.muted));

    for i in start..start + rows {
        let (bar, line) = match items.get(i) {
            Some(item) if i == s.cursor => (
                "▌".with(t.accent).to_string(),
                format!(" {}", fit(item, inner))
                    .on(t.select)
                    .attribute(Attribute::Bold)
                    .to_string(),
            ),
            Some(item) => (" ".into(), format!(" {}", fit(item, inner))),
            None => (" ".into(), " ".repeat(inner + 1)),
        };
        let edge = "│".with(t.muted);
        out += &format!("{edge}{bar}{line}{edge}\r\n");
    }

    out += &format!("{}\r\n", format!("╰{rule}╯").with(t.muted));

    let pill = format!(" {} ", format!("{:?}", s.mode).to_uppercase())
        .with(t.mode_fg)
        .on(mode_color(s.mode))
        .bold();
    let prefix = match s.mode {
        Mode::Insert => "/",
        Mode::Command => ":",
        Mode::Normal => "",
    };
    let hint = "j/k move  / search  : command  q quit".with(t.muted);
    let pos = format!("{}/{}", s.cursor + 1, items.len()).with(t.accent);
    if s.mode == Mode::Normal {
        out += &format!("{pill} {hint}  {pos}");
    } else {
        out += &format!("{pill} {prefix}{}█", s.buffer);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abc…");
    }

    #[test]
    fn render_marks_the_selected_role_and_mode() {
        let items = vec!["ANALYST".to_string(), "ENGINEER".to_string()];
        let s = State {
            mode: Mode::Normal,
            cursor: 1,
            buffer: String::new(),
        };
        let out = render(&items, &s, 40, 10, &Theme::terminal());
        assert!(out.contains("ENGINEER") && out.contains("NORMAL") && out.contains("2/2"));
    }
}
