use crate::modes::{Focus, Mode, State, Tab};
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

// A bordered pane of `width` columns; `body` lines are already fitted to the inner width.
fn pane(title: &str, body: Vec<String>, width: usize, focused: bool, t: &Theme) -> Vec<String> {
    let color = if focused { t.accent } else { t.muted };
    let inner = width.saturating_sub(2);
    let text = fit(&format!(" {title} "), inner.min(title.width() + 2));
    let top = format!("╭{text}{}╮", "─".repeat(inner - text.width()));
    let edge = "│".with(color);
    let mut out = vec![top.with(color).to_string()];
    out.extend(body.into_iter().map(|l| format!("{edge}{l}{edge}")));
    out.push(format!("╰{}╯", "─".repeat(inner)).with(color).to_string());
    out
}

pub fn render(
    items: &[String],
    detail: &[String],
    s: &State,
    width: usize,
    height: usize,
    t: &Theme,
) -> String {
    let rows = height.saturating_sub(5).max(1);
    let cursor = s.cursor();
    let start = cursor.saturating_sub(rows - 1);
    // Narrow terminals show only the focused pane.
    let split = width >= 80;
    let list_w = if split { (width / 3).max(20) } else { width };
    let detail_w = if split { width - list_w } else { width };

    let list_inner = list_w.saturating_sub(2).max(1);
    let list_body = (start..start + rows)
        .map(|i| match items.get(i) {
            Some(item) if i == cursor => format!(
                "{}{}",
                "▌".with(t.accent),
                format!(" {}", fit(item, list_inner.saturating_sub(2)))
                    .on(t.select)
                    .attribute(Attribute::Bold)
            ),
            Some(item) => format!("  {}", fit(item, list_inner.saturating_sub(2))),
            None => " ".repeat(list_inner),
        })
        .collect();
    let detail_inner = detail_w.saturating_sub(2).max(1);
    let scroll = s.scroll.min(detail.len().saturating_sub(1));
    let detail_body = (scroll..scroll + rows)
        .map(|i| fit(detail.get(i).map_or("", |l| l.as_str()), detail_inner))
        .collect();

    let tab_name = |tab: Tab| format!("{tab:?}");
    let list_title = tab_name(s.tab);
    let detail_title = items.get(cursor).map_or("Details", |n| n.as_str());
    let list = pane(&list_title, list_body, list_w, s.focus == Focus::List, t);
    let det = pane(
        detail_title,
        detail_body,
        detail_w,
        s.focus == Focus::Detail,
        t,
    );
    let panes: Vec<String> = match (split, s.focus) {
        (true, _) => list.into_iter().zip(det).map(|(a, b)| a + &b).collect(),
        (false, Focus::List) => list,
        (false, Focus::Detail) => det,
    };

    let tabs: String = Tab::ALL
        .iter()
        .map(|&tab| {
            let label = format!(" {} ", tab_name(tab));
            if tab == s.tab {
                label.with(t.mode_fg).on(t.accent).bold().to_string()
            } else {
                label.with(t.muted).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!(" {}  {tabs}\r\n", "◆ FLURRY".with(t.accent).bold());
    for line in panes {
        out += &format!("{line}\r\n");
    }

    let label = format!(" {} ", format!("{:?}", s.mode).to_uppercase());
    let pill = label.clone().with(t.mode_fg).on(mode_color(s.mode)).bold();
    let prefix = match s.mode {
        Mode::Insert => "/",
        Mode::Command => ":",
        Mode::Normal => "",
    };
    // The status line never wraps: the hint is dropped first, then the typed text is cut from the left.
    let hint = "Tab tabs  j/k move  l detail  / search  : command  q quit";
    let place = format!(
        "{}/{}",
        if items.is_empty() { 0 } else { cursor + 1 },
        items.len()
    );
    let pos = if s.filter.is_empty() {
        place
    } else {
        format!("/{} {place}", s.filter)
    };
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
        let mut s = State::new(Tab::Roles);
        let narrow = status_line(&render(&items, &[], &s, 20, 10, &Theme::terminal()));
        assert!(narrow.width() <= 20 && !narrow.contains("quit"));
        s.mode = Mode::Command;
        s.buffer = "a-very-long-command-being-typed".into();
        let cmd = status_line(&render(&items, &[], &s, 20, 10, &Theme::terminal()));
        assert!(cmd.width() <= 20 && cmd.ends_with("typed█"));
    }

    #[test]
    fn render_marks_the_selected_role_and_mode() {
        let items = vec!["ANALYST".to_string(), "ENGINEER".to_string()];
        let mut s = State::new(Tab::Roles);
        s.cursors[Tab::Roles.index()] = 1;
        let detail = vec!["owner: SECURITYADMIN".to_string()];
        let out = render(&items, &detail, &s, 100, 10, &Theme::terminal());
        assert!(out.contains("ENGINEER") && out.contains("NORMAL") && out.contains("2/2"));
        assert!(out.contains("owner: SECURITYADMIN") && out.contains("Warehouses"));
        // A narrow terminal shows only the focused pane.
        let out = render(&items, &detail, &s, 40, 10, &Theme::terminal());
        assert!(!out.contains("SECURITYADMIN"));
    }
}
