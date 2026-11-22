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

/// A related entity the detail pane can open.
#[derive(Clone, PartialEq, Debug)]
pub struct Link {
    pub tab: Tab,
    pub name: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Picker {
    pub profiles: Vec<String>,
    pub cursor: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SyncStatus {
    pub profile: String,
    pub lines: Vec<String>,
    pub finished: bool,
}

// Where the user was before following a link.
#[derive(Clone, Debug)]
struct View {
    tab: Tab,
    cursor: usize,
    filter: String,
    scroll: usize,
}

#[derive(Clone, Debug)]
pub struct State {
    pub mode: Mode,
    pub tab: Tab,
    pub focus: Focus,
    /// List cursor of each tab, indexed by `Tab::index`.
    pub cursors: [usize; 3],
    /// Selected detail line.
    pub scroll: usize,
    pub buffer: String,
    /// Name search applied to the active list.
    pub filter: String,
    /// Entity a followed link opens; the caller resolves it with `resolve`.
    pub goto: Option<String>,
    /// One-shot message for the status line.
    pub notice: Option<String>,
    pub picker: Option<Picker>,
    pub sync: Option<String>,
    pub sync_status: Option<SyncStatus>,
    history: Vec<View>,
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
            goto: None,
            notice: None,
            picker: None,
            sync: None,
            sync_status: None,
            history: Vec::new(),
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

    fn follow(&mut self, link: &Link) {
        self.history.push(View {
            tab: self.tab,
            cursor: self.cursor(),
            filter: self.filter.clone(),
            scroll: self.scroll,
        });
        self.switch(link.tab);
        self.focus = Focus::Detail;
        self.goto = Some(link.name.clone());
    }

    fn back(&mut self) {
        if let Some(v) = self.history.pop() {
            (self.tab, self.filter, self.scroll) = (v.tab, v.filter, v.scroll);
            self.cursors[v.tab.index()] = v.cursor;
            self.focus = Focus::Detail;
        }
    }

    /// Moves the cursor to the followed entity in the unfiltered `items`, or goes back if it is missing.
    pub fn resolve(&mut self, items: &[String]) {
        let Some(name) = self.goto.take() else {
            return;
        };
        match items.iter().position(|i| *i == name) {
            Some(i) => self.cursors[self.tab.index()] = i,
            None => {
                self.back();
                self.notice = Some(format!("{name} is not in the snapshot"));
            }
        }
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
    /// `Ctrl+o`: back to where the last followed link was opened.
    Back,
}

// Pure transition so mode behavior is testable without a terminal; `size` is the length of the focused pane
// and `link` the link of the selected detail line.
pub fn handle_key(mut s: State, key: Key, size: usize, link: Option<&Link>) -> State {
    let last = size.saturating_sub(1);
    let i = s.tab.index();
    let next = Tab::ALL[(i + 1) % 3];
    let prev = Tab::ALL[(i + 2) % 3];
    s.notice = None;
    if let Some(status) = &s.sync_status {
        if matches!(key, Key::Esc | Key::Char('q'))
            || (status.finished && matches!(key, Key::Enter))
        {
            s.sync_status = None;
        }
        return s;
    }
    if let Some(picker) = s.picker.as_mut() {
        let last = picker.profiles.len().saturating_sub(1);
        match key {
            Key::Char('j') | Key::Down => picker.cursor = (picker.cursor + 1).min(last),
            Key::Char('k') | Key::Up => picker.cursor = picker.cursor.saturating_sub(1),
            Key::Enter => {
                s.sync = picker.profiles.get(picker.cursor).cloned();
                s.picker = None;
            }
            Key::Esc | Key::Char('q') => s.picker = None,
            _ => {}
        }
        return s;
    }
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
        (Mode::Normal, Key::Char('l') | Key::Right | Key::Enter) => match (s.focus, link) {
            (Focus::Detail, Some(link)) => s.follow(link),
            _ => s.focus = Focus::Detail,
        },
        (Mode::Normal, Key::Backspace | Key::Back) => s.back(),
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
        keys.iter().fold(s, |s, k| handle_key(s, *k, 3, None))
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

    #[test]
    fn following_a_link_opens_the_entity_and_back_returns() {
        let link = Link {
            tab: Tab::Warehouses,
            name: "WH_B".into(),
        };
        let s = press(start(), &[Key::Char('j'), Key::Char('l'), Key::Char('j')]);
        let mut s = handle_key(s, Key::Enter, 3, Some(&link));
        assert_eq!(
            (s.tab, s.focus, s.goto.as_deref()),
            (Tab::Warehouses, Focus::Detail, Some("WH_B"))
        );
        s.resolve(&["WH_A".into(), "WH_B".into()]);
        assert_eq!((s.cursor(), s.scroll, s.goto.clone()), (1, 0, None));
        let s = press(s, &[Key::Back]);
        assert_eq!(
            (s.tab, s.cursor(), s.scroll, s.focus),
            (Tab::Roles, 1, 1, Focus::Detail)
        );
        // An empty history leaves the view as it is.
        let s = press(s, &[Key::Backspace]);
        assert_eq!((s.tab, s.cursor()), (Tab::Roles, 1));
    }

    fn with_picker(cursor: usize) -> State {
        let mut s = start();
        s.picker = Some(Picker {
            profiles: vec!["dev".into(), "prod".into()],
            cursor,
        });
        s
    }

    #[test]
    fn picker_moves_within_bounds_and_enter_chooses_the_profile() {
        let s = press(
            with_picker(0),
            &[Key::Char('k'), Key::Char('j'), Key::Down, Key::Enter],
        );
        assert_eq!((s.sync.as_deref(), s.picker), (Some("prod"), None));
    }

    #[test]
    fn sync_status_swallows_keys_until_esc_or_enter_once_finished() {
        let mut s = start();
        s.sync_status = Some(SyncStatus {
            profile: "dev".into(),
            lines: Vec::new(),
            finished: false,
        });
        let s = press(s, &[Key::Char('R'), Key::Enter]);
        assert!(s.sync_status.is_some());
        let mut done = s.clone();
        done.sync_status.as_mut().unwrap().finished = true;
        assert!(press(done, &[Key::Enter]).sync_status.is_none());
        assert!(press(s.clone(), &[Key::Esc]).sync_status.is_none());
        let closed = press(s, &[Key::Char('q')]);
        assert!(closed.sync_status.is_none() && !closed.quit);
    }

    #[test]
    fn picker_swallows_other_keys_and_esc_cancels() {
        let s = press(with_picker(0), &[Key::Tab, Key::Char('/'), Key::Char('x')]);
        assert!(!s.quit && s.picker.is_some() && s.mode == Mode::Normal);
        assert!(press(s.clone(), &[Key::Esc]).picker.is_none());
        let s = press(s, &[Key::Char('q')]);
        assert!(s.picker.is_none() && s.sync.is_none() && !s.quit);
    }

    #[test]
    fn enter_without_a_link_only_focuses_the_details() {
        let s = press(start(), &[Key::Enter, Key::Enter]);
        assert_eq!((s.tab, s.focus, s.goto), (Tab::Roles, Focus::Detail, None));
    }

    #[test]
    fn a_missing_target_goes_back_with_a_notice() {
        let link = Link {
            tab: Tab::Users,
            name: "GHOST".into(),
        };
        let s = press(
            start(),
            &[Key::Char('/'), Key::Char('a'), Key::Enter, Key::Char('l')],
        );
        let mut s = handle_key(s, Key::Enter, 3, Some(&link));
        s.resolve(&["JANE".into()]);
        assert_eq!((s.tab, s.filter.as_str()), (Tab::Roles, "a"));
        assert_eq!(s.notice.as_deref(), Some("GHOST is not in the snapshot"));
        assert_eq!(press(s, &[Key::Char('j')]).notice, None);
    }
}
