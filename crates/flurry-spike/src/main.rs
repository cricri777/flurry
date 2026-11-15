mod config;
mod modes;
mod snowflake;
mod sqlite;
mod sso;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use modes::{Key, Mode, State, handle_key};
use std::io::{Write, stdout};

const ITEMS: [&str; 4] = ["ROLE_A", "ROLE_B", "ROLE_C", "ROLE_D"];

fn tui() -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut s = State {
        mode: Mode::Normal,
        cursor: 0,
        buffer: String::new(),
    };
    loop {
        let mut out =
            String::from("FLURRY spike - j/k move, / search, : command, Esc normal, q quit\r\n");
        for (i, item) in ITEMS.iter().enumerate() {
            out += &format!("{} {item}\r\n", if i == s.cursor { ">" } else { " " });
        }
        out += &format!("-- {:?} -- {}\r\n", s.mode, s.buffer);
        execute!(
            stdout(),
            crossterm::cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        )?;
        print!("{out}");
        stdout().flush()?;
        let Event::Key(k) = event::read()? else {
            continue;
        };
        let key = match k.code {
            KeyCode::Char('q') if s.mode == Mode::Normal => break,
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Esc => Key::Esc,
            KeyCode::Enter => Key::Enter,
            KeyCode::Backspace => Key::Backspace,
            _ => continue,
        };
        s = handle_key(s, key, ITEMS.len());
    }
    execute!(stdout(), LeaveAlternateScreen)?;
    terminal::disable_raw_mode()
}

#[tokio::main]
async fn main() -> Result<(), String> {
    match std::env::args().nth(1).as_deref() {
        Some("sqlite") => println!("sqlite ok, rows: {}", sqlite::run("spike.sqlite")?),
        Some("snowflake") => snowflake::run(std::env::args().nth(2).as_deref()).await?,
        Some("tui") => tui().map_err(|e| e.to_string())?,
        _ => eprintln!("usage: flurry-spike sqlite|snowflake [connection]|tui"),
    }
    Ok(())
}
