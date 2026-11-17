#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tab {
    Users,
    Roles,
    Warehouses,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Users, Tab::Roles, Tab::Warehouses];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    List,
    Detail,
}

#[derive(Clone, Debug)]
pub struct State {
    pub mode: Mode,
    pub tab: Tab,
    pub focus: Focus,
    /// List cursor of each tab, indexed by `Tab::index`.
    pub cursors: [usize; 3],
    pub scroll: usize,
    pub buffer: String,
    /// Name search applied to the active list.
    pub filter: String,
    pub quit: bool,
}

impl State {
    pub fn new(tab: Tab) -> Self {
        State {
            mode: Mode::Normal,
            tab,
            focus: Focus::List,
            cursors: [0; 3],
            scroll: 0,
            buffer: String::new(),
            filter: String::new(),
            quit: false,
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursors[self.tab.index()]
    }

    fn switch(&mut self, tab: Tab) {
        self.tab = tab;
        self.focus = Focus::List;
        self.scroll = 0;
        self.filter.clear();
    }
}

#[derive(Clone, Copy)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
    Esc,
    Enter,
    Backspace,
}

// Pure transition so mode behavior is testable without a terminal; `size` is the length of the focused pane.
pub fn handle_key(mut s: State, key: Key, size: usize) -> State {
    let last = size.saturating_sub(1);
    let i = s.tab.index();
    let next = Tab::ALL[(i + 1) % 3];
    let prev = Tab::ALL[(i + 2) % 3];
    match (s.mode, key) {
        (Mode::Normal, Key::Char('j') | Key::Down) => move_to(&mut s, |n| (n + 1).min(last)),
        (Mode::Normal, Key::Char('k') | Key::Up) => move_to(&mut s, |n| n.saturating_sub(1)),
        (Mode::Normal, Key::Char('G')) => move_to(&mut s, |_| last),
        (Mode::Normal, Key::Char('q')) => s.quit = true,
        (Mode::Normal, Key::Char('/')) => (s.mode, s.buffer) = (Mode::Insert, s.filter.clone()),
        (Mode::Normal, Key::Char(':')) => (s.mode, s.buffer) = (Mode::Command, String::new()),
        (Mode::Normal, Key::Tab) => s.switch(next),
        (Mode::Normal, Key::BackTab) => s.switch(prev),
        (Mode::Normal, Key::Char(c @ '1'..='3')) => s.switch(Tab::ALL[c as usize - '1' as usize]),
        (Mode::Normal, Key::Char('l') | Key::Right | Key::Enter) => s.focus = Focus::Detail,
        (Mode::Normal, Key::Char('h') | Key::Left) => s.focus = Focus::List,
        (Mode::Normal, Key::Esc) if s.focus == Focus::Detail => s.focus = Focus::List,
        (Mode::Normal, Key::Esc) => {
            s.filter.clear();
            s.cursors[i] = 0;
        }
        (Mode::Normal, _) => {}
        (Mode::Insert, Key::Esc) => {
            s.filter.clear();
            (s.mode, s.buffer, s.cursors[i]) = (Mode::Normal, String::new(), 0);
        }
        (Mode::Command, Key::Esc) => (s.mode, s.buffer) = (Mode::Normal, String::new()),
        (Mode::Insert, Key::Enter) => s.mode = Mode::Normal,
        (Mode::Command, Key::Enter) => {
            s.quit = matches!(s.buffer.as_str(), "q" | "quit");
            (s.mode, s.buffer) = (Mode::Normal, String::new());
        }
        (_, Key::Backspace) => {
            s.buffer.pop();
            search(&mut s);
        }
        (_, Key::Char(c)) => {
            s.buffer.push(c);
            search(&mut s);
        }
        _ => {}
    }
    s
}

// Moves the list cursor, or scrolls the detail pane when it has focus.
fn move_to(s: &mut State, f: impl Fn(usize) -> usize) {
    match s.focus {
        Focus::List => {
            let i = s.tab.index();
            s.cursors[i] = f(s.cursors[i]);
            s.scroll = 0;
        }
        Focus::Detail => s.scroll = f(s.scroll),
    }
}

// Typing in search mode filters the list live.
fn search(s: &mut State) {
    if s.mode == Mode::Insert {
        s.filter = s.buffer.clone();
        s.cursors[s.tab.index()] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> State {
        State::new(Tab::Roles)
    }

    fn press(s: State, keys: &[Key]) -> State {
        keys.iter().fold(s, |s, k| handle_key(s, *k, 3))
    }

    #[test]
    fn j_and_k_move_within_bounds() {
        let s = press(
            start(),
            &[
                Key::Char('k'),
                Key::Char('j'),
                Key::Down,
                Key::Down,
                Key::Char('j'),
            ],
        );
        assert_eq!(s.cursor(), 2);
        let s = press(s, &[Key::Up, Key::Char('k'), Key::Char('k')]);
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn slash_and_colon_enter_modes_and_collect_text() {
        let s = press(
            start(),
            &[
                Key::Char('/'),
                Key::Char('a'),
                Key::Char('b'),
                Key::Backspace,
            ],
        );
        assert_eq!((s.mode, s.buffer.as_str()), (Mode::Insert, "a"));
        let s = press(start(), &[Key::Char(':'), Key::Char('x')]);
        assert_eq!((s.mode, s.buffer.as_str()), (Mode::Command, "x"));
    }

    #[test]
    fn search_filters_live_and_esc_clears_it() {
        let s = press(start(), &[Key::Char('/'), Key::Char('x')]);
        assert_eq!(s.filter, "x");
        let s = press(s, &[Key::Enter]);
        assert_eq!((s.mode, s.filter.as_str()), (Mode::Normal, "x"));
        let s = press(s, &[Key::Esc]);
        assert_eq!(s.filter, "");
    }

    #[test]
    fn movement_keys_are_text_in_insert_mode() {
        let s = press(start(), &[Key::Char('/'), Key::Char('j')]);
        assert_eq!((s.cursor(), s.buffer.as_str()), (0, "j"));
    }

    #[test]
    fn tabs_cycle_and_keep_their_own_cursor() {
        let s = press(start(), &[Key::Char('j'), Key::Tab]);
        assert_eq!((s.tab, s.cursor()), (Tab::Warehouses, 0));
        let s = press(s, &[Key::Tab, Key::BackTab, Key::BackTab]);
        assert_eq!((s.tab, s.cursor()), (Tab::Roles, 1));
        let s = press(s, &[Key::Char('1')]);
        assert_eq!((s.tab, s.cursor()), (Tab::Users, 0));
    }

    #[test]
    fn detail_focus_scrolls_instead_of_moving_the_cursor() {
        let s = press(start(), &[Key::Char('l'), Key::Char('j')]);
        assert_eq!((s.focus, s.scroll, s.cursor()), (Focus::Detail, 1, 0));
        let s = press(s, &[Key::Esc]);
        assert_eq!(s.focus, Focus::List);
    }

    #[test]
    fn quit_comes_from_q_or_the_quit_command() {
        assert!(press(start(), &[Key::Char('q')]).quit);
        let s = press(start(), &[Key::Char(':'), Key::Char('q'), Key::Enter]);
        assert!(s.quit);
        let s = press(start(), &[Key::Char('/'), Key::Char('q'), Key::Enter]);
        assert!(!s.quit);
    }
}
