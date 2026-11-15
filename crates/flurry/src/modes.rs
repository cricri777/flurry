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

#[derive(Clone, Copy)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> State {
        State {
            mode: Mode::Normal,
            cursor: 0,
            buffer: String::new(),
        }
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
        assert_eq!(s.cursor, 2);
        let s = press(s, &[Key::Up, Key::Char('k'), Key::Char('k')]);
        assert_eq!(s.cursor, 0);
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
        let s = press(start(), &[Key::Char(':'), Key::Char('q')]);
        assert_eq!((s.mode, s.buffer.as_str()), (Mode::Command, "q"));
    }

    #[test]
    fn esc_clears_and_enter_keeps_the_buffer() {
        let typed = [Key::Char('/'), Key::Char('x')];
        let s = press(start(), &[typed[0], typed[1], Key::Esc]);
        assert_eq!((s.mode, s.buffer.as_str()), (Mode::Normal, ""));
        let s = press(start(), &[typed[0], typed[1], Key::Enter]);
        assert_eq!((s.mode, s.buffer.as_str()), (Mode::Normal, "x"));
    }

    #[test]
    fn movement_keys_are_text_in_insert_mode() {
        let s = press(start(), &[Key::Char('/'), Key::Char('j')]);
        assert_eq!((s.cursor, s.buffer.as_str()), (0, "j"));
    }
}
