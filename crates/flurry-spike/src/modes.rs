#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(Clone, Debug)]
pub struct State {
    pub mode: Mode,
    pub cursor: usize,
    pub buffer: String,
}

pub enum Key {
    Char(char),
    Up,
    Down,
    Esc,
    Enter,
    Backspace,
}

// Pure transition so mode behavior is testable without a terminal.
pub fn handle_key(mut s: State, key: Key, size: usize) -> State {
    match (s.mode, key) {
        (Mode::Normal, Key::Char('j') | Key::Down) => s.cursor = (s.cursor + 1).min(size - 1),
        (Mode::Normal, Key::Char('k') | Key::Up) => s.cursor = s.cursor.saturating_sub(1),
        (Mode::Normal, Key::Char('/')) => (s.mode, s.buffer) = (Mode::Insert, String::new()),
        (Mode::Normal, Key::Char(':')) => (s.mode, s.buffer) = (Mode::Command, String::new()),
        (Mode::Normal, _) => {}
        (_, Key::Esc) => (s.mode, s.buffer) = (Mode::Normal, String::new()),
        (_, Key::Enter) => s.mode = Mode::Normal,
        (_, Key::Backspace) => {
            s.buffer.pop();
        }
        (_, Key::Char(c)) => s.buffer.push(c),
        _ => {}
    }
    s
}
