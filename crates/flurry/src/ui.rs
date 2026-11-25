use crate::modes::{Focus, Mode, State, SyncStatus, Tab};
use crossterm::style::{Attribute, Color, Stylize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum ThemeName {
    /// The terminal's own ANSI palette.
    #[default]
    Terminal,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

pub struct Theme {
    accent: Color,
    mode_fg: Color,
    muted: Color,
    select: Color,
    modes: [Color; 3],
}

impl Theme {
    pub fn new(name: ThemeName) -> Self {
        let flavor = match name {
            ThemeName::Terminal => return Self::terminal(),
            ThemeName::Latte => catppuccin::PALETTE.latte,
            ThemeName::Frappe => catppuccin::PALETTE.frappe,
            ThemeName::Macchiato => catppuccin::PALETTE.macchiato,
            ThemeName::Mocha => catppuccin::PALETTE.mocha,
        };
        let c = flavor.colors;
        let rgb = |x: catppuccin::Color| Color::Rgb {
            r: x.rgb.r,
            g: x.rgb.g,
            b: x.rgb.b,
        };
        Theme {
            accent: rgb(c.mauve),
            mode_fg: rgb(c.base),
            muted: rgb(c.overlay1),
            select: rgb(c.surface1),
            modes: [rgb(c.green), rgb(c.yellow), rgb(c.pink)],
        }
    }

    // Colors come from the terminal's own ANSI palette, so the TUI follows whatever theme it opens in.
    pub fn terminal() -> Self {
        Theme {
            accent: Color::Cyan,
            mode_fg: Color::Black,
            muted: Color::DarkGrey,
            select: Color::Blue,
            modes: [Color::Green, Color::Yellow, Color::Magenta],
        }
    }

    fn mode(&self, mode: Mode) -> Color {
        self.modes[mode as usize]
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

fn sync_pane(status: &SyncStatus, width: usize, rows: usize, t: &Theme) -> Vec<String> {
    let title = if status.finished {
        format!("Sync {} finished", status.profile)
    } else {
        format!("Syncing {}...", status.profile)
    };
    let inner = width.saturating_sub(2).max(1);
    let shown = status.lines.len().saturating_sub(rows);
    let body = (shown..shown + rows)
        .map(|i| fit(status.lines.get(i).map_or("", |l| l.as_str()), inner))
        .collect();
    pane(&title, body, width, true, t)
}

pub fn list_rows(height: usize) -> usize {
    height.saturating_sub(5).max(1)
}

// Moves the window only as far as needed to keep `selected` visible.
fn follow(top: usize, selected: usize, rows: usize, len: usize) -> usize {
    let top = top.min(len.saturating_sub(rows));
    if selected < top {
        selected
    } else if selected >= top + rows {
        selected + 1 - rows
    } else {
        top
    }
}

pub fn render(
    items: &[String],
    detail: &[String],
    s: &mut State,
    width: usize,
    height: usize,
    t: &Theme,
) -> String {
    let picked = s.picker.as_ref();
    let detail = picked.map_or(detail, |p| p.profiles.as_slice());
    let scroll = picked.map_or(s.scroll, |p| p.cursor);
    let focus = if picked.is_some() {
        Focus::Detail
    } else {
        s.focus
    };
    let rows = list_rows(height);
    let cursor = s.cursor();
    let start = follow(s.list_top, cursor, rows, items.len());
    s.list_top = start;
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
    // The selected detail line is only highlighted while the details have focus.
    let selected = scroll.min(detail.len().saturating_sub(1));
    let top = follow(s.detail_top, selected, rows, detail.len());
    s.detail_top = top;
    let detail_body = (top..top + rows)
        .map(|i| {
            let line = fit(detail.get(i).map_or("", |l| l.as_str()), detail_inner);
            if focus == Focus::Detail && i == selected && i < detail.len() {
                line.on(t.select).attribute(Attribute::Bold).to_string()
            } else {
                line
            }
        })
        .collect();

    let tab_name = |tab: Tab| format!("{tab:?}");
    let list_title = tab_name(s.tab);
    let detail_title = match &s.picker {
        Some(_) => "Sync from profile",
        None => items.get(cursor).map_or("Details", |n| n.as_str()),
    };
    let list = pane(&list_title, list_body, list_w, focus == Focus::List, t);
    let det = pane(
        detail_title,
        detail_body,
        detail_w,
        focus == Focus::Detail,
        t,
    );
    let panes: Vec<String> = match (&s.sync_status, split, focus) {
        (Some(status), _, _) => sync_pane(status, width, rows, t),
        (None, true, _) => list.into_iter().zip(det).map(|(a, b)| a + &b).collect(),
        (None, false, Focus::List) => list,
        (None, false, Focus::Detail) => det,
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
    let pill = label.clone().with(t.mode_fg).on(t.mode(s.mode)).bold();
    let prefix = match s.mode {
        Mode::Insert => "/",
        Mode::Command => ":",
        Mode::Normal => "",
    };
    // The status line never wraps: the hint is dropped first, then the typed text is cut from the left.
    let hint = match &s.notice {
        Some(notice) => notice.as_str(),
        None if s.sync_status.as_ref().is_some_and(|st| st.finished) => "Enter close",
        None if s.sync_status.is_some() => "Esc/q cancel",
        None if s.picker.is_some() => "j/k ^d/^u choose  Enter sync  Esc/q cancel",
        None if s.focus == Focus::Detail => {
            "Enter open  ^O back  y copy  h list  / search  R sync  q quit"
        }
        None => "Tab tabs  j/k ^d/^u move  l detail  y copy  / search  R sync  q quit",
    };
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
        if let Some(notice) = s.notice.as_ref().filter(|_| full > width) {
            let room = width.saturating_sub(label.width() + 1);
            out += &format!("{pill} {}", fit(notice, room).trim_end().with(t.accent));
        } else if full <= width {
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
    fn named_theme_renders_truecolor_and_terminal_does_not() {
        let items = vec!["ANALYST".to_string()];
        let mut s = State::new(Tab::Roles);
        let mocha = render(&items, &[], &mut s, 40, 10, &Theme::new(ThemeName::Mocha));
        let plain = render(
            &items,
            &[],
            &mut s,
            40,
            10,
            &Theme::new(ThemeName::Terminal),
        );
        assert!(mocha.contains("38;2;") && !plain.contains("38;2;"));
    }

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
        let narrow = status_line(&render(&items, &[], &mut s, 20, 10, &Theme::terminal()));
        assert!(narrow.width() <= 20 && !narrow.contains("quit"));
        s.mode = Mode::Command;
        s.buffer = "a-very-long-command-being-typed".into();
        let cmd = status_line(&render(&items, &[], &mut s, 20, 10, &Theme::terminal()));
        assert!(cmd.width() <= 20 && cmd.ends_with("typed█"));
    }

    #[test]
    fn a_notice_that_does_not_fit_is_truncated_not_dropped() {
        let mut s = State::new(Tab::Roles);
        s.notice = Some("No Snowflake connections found; see docs/getting-started.md".into());
        let line = status_line(&render(&[], &[], &mut s, 40, 10, &Theme::terminal()));
        assert!(line.width() <= 40 && line.contains("No Snowflake") && line.contains('…'));
    }

    #[test]
    fn render_marks_the_selected_role_and_mode() {
        let items = vec!["ANALYST".to_string(), "ENGINEER".to_string()];
        let mut s = State::new(Tab::Roles);
        s.cursors[Tab::Roles.index()] = 1;
        let detail = vec!["owner: SECURITYADMIN".to_string()];
        let out = render(&items, &detail, &mut s, 100, 10, &Theme::terminal());
        assert!(out.contains("ENGINEER") && out.contains("NORMAL") && out.contains("2/2"));
        assert!(out.contains("owner: SECURITYADMIN") && out.contains("Warehouses"));
        // A narrow terminal shows only the focused pane.
        let out = render(&items, &detail, &mut s, 40, 10, &Theme::terminal());
        assert!(!out.contains("SECURITYADMIN"));
    }

    #[test]
    fn picker_replaces_the_detail_pane_with_the_profiles() {
        let items = vec!["ANALYST".to_string()];
        let mut s = State::new(Tab::Roles);
        s.picker = Some(crate::modes::Picker {
            profiles: vec!["dev".into(), "prod".into()],
            cursor: 1,
        });
        let out = render(
            &items,
            &["owner: X".into()],
            &mut s,
            100,
            10,
            &Theme::terminal(),
        );
        assert!(out.contains("Sync from profile") && out.contains("prod"));
        assert!(!out.contains("owner: X") && status_line(&out).contains("Esc/q cancel"));
    }

    #[test]
    fn sync_status_replaces_the_panes_and_shows_the_latest_lines() {
        let items = vec!["ANALYST".to_string()];
        let mut s = State::new(Tab::Roles);
        s.sync_status = Some(SyncStatus {
            profile: "prod".into(),
            lines: (0..20).map(|i| format!("step {i}")).collect(),
            finished: false,
        });
        let out = render(&items, &[], &mut s, 100, 10, &Theme::terminal());
        assert!(out.contains("Syncing prod...") && out.contains("step 19"));
        assert!(!out.contains("step 0 ") && !out.contains("ANALYST"));
        assert!(status_line(&out).contains("Esc/q cancel"));
    }

    #[test]
    fn detail_selection_follows_the_cursor_and_notice_replaces_the_hint() {
        let items = vec!["ANALYST".to_string()];
        let detail: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let mut s = State::new(Tab::Roles);
        s.focus = Focus::Detail;
        s.scroll = 15;
        let out = render(&items, &detail, &mut s, 100, 10, &Theme::terminal());
        assert!(out.contains("line 15") && !out.contains("line 10 "));
        s.notice = Some("GHOST is not in the snapshot".into());
        let out = render(&items, &detail, &mut s, 100, 10, &Theme::terminal());
        assert!(status_line(&out).contains("GHOST is not in the snapshot"));
    }

    #[test]
    fn window_stays_put_while_the_cursor_moves_inside_it() {
        let items: Vec<String> = (0..30).map(|i| format!("ROLE{i:02}")).collect();
        let mut s = State::new(Tab::Roles);
        s.cursors[s.tab.index()] = 20;
        render(&items, &[], &mut s, 100, 10, &Theme::terminal());
        assert_eq!(s.list_top, 16);
        s.cursors[s.tab.index()] = 18;
        let out = render(&items, &[], &mut s, 100, 10, &Theme::terminal());
        assert_eq!(s.list_top, 16);
        assert!(out.contains("ROLE16") && out.contains("ROLE20"));
        s.cursors[s.tab.index()] = 15;
        render(&items, &[], &mut s, 100, 10, &Theme::terminal());
        assert_eq!(s.list_top, 15);
    }
}
