use crate::modes::{Mode, State};
use crossterm::style::{Attribute, Color, Stylize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

// Pads or truncates to exactly `width` terminal columns, counting wide characters as two.
fn fit(text: &str, width: usize) -> String {
    let n = text.width();
    if n <= width {
        return format!("{text}{}", " ".repeat(width - n));
    }
    let (mut head, mut used) = (String::new(), 0);
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width.saturating_sub(1) {
            break;
        }
        head.push(c);
        used += w;
    }
    format!("{head}…{}", " ".repeat(width.saturating_sub(used + 1)))
}

// Keeps the end of `text` within `width` columns, so the text being typed stays visible.
fn tail(text: &str, width: usize) -> String {
    let (mut out, mut used) = (Vec::new(), 0);
    for c in text.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.into_iter().rev().collect()
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

    let label = format!(" {} ", format!("{:?}", s.mode).to_uppercase());
    let pill = label.clone().with(t.mode_fg).on(mode_color(s.mode)).bold();
    let prefix = match s.mode {
        Mode::Insert => "/",
        Mode::Command => ":",
        Mode::Normal => "",
    };
    // The status line never wraps: the hint is dropped first, then the typed text is cut from the left.
    let hint = "j/k move  / search  : command  q quit";
    let pos = format!("{}/{}", s.cursor + 1, items.len());
    if s.mode == Mode::Normal {
        let full = label.width() + 1 + hint.width() + 2 + pos.width();
        if full <= width {
            out += &format!("{pill} {}  {}", hint.with(t.muted), pos.with(t.accent));
        } else {
            let room = width.saturating_sub(label.width() + 1);
            out += &format!("{pill} {}", tail(&pos, room).with(t.accent));
        }
    } else {
        let room = width.saturating_sub(label.width() + 1 + prefix.width() + 1);
        out += &format!("{pill} {prefix}{}█", tail(&s.buffer, room));
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
    fn fit_counts_wide_characters_as_two_columns() {
        assert_eq!(fit("日本", 4).width(), 4);
        assert_eq!(fit("日本語", 4), "日… ");
    }

    // Last screen line with ANSI escapes removed.
    fn status_line(out: &str) -> String {
        let mut plain = String::new();
        let mut esc = false;
        for c in out.rsplit("\r\n").next().unwrap().chars() {
            match (esc, c) {
                (false, '\x1b') => esc = true,
                (true, c) if c.is_ascii_alphabetic() => esc = false,
                (false, c) => plain.push(c),
                _ => {}
            }
        }
        plain
    }

    #[test]
    fn status_line_never_exceeds_the_width() {
        let items = vec!["ANALYST".to_string()];
        let mut s = State {
            mode: Mode::Normal,
            cursor: 0,
            buffer: String::new(),
        };
        let narrow = status_line(&render(&items, &s, 20, 10, &Theme::terminal()));
        assert!(narrow.width() <= 20 && !narrow.contains("quit"));
        s.mode = Mode::Command;
        s.buffer = "a-very-long-command-being-typed".into();
        let cmd = status_line(&render(&items, &s, 20, 10, &Theme::terminal()));
        assert!(cmd.width() <= 20 && cmd.ends_with("typed█"));
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
