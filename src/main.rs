use std::io::{stdout, Write};

use crossterm::{
    cursor,
    event::{self, read},
    terminal, ExecutableCommand, QueueableCommand,
};

enum Action {
    Quit,

    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
}

enum Mode {
    Normal,
    Insert,
}

fn handle_event(mode: &Mode, ev: event::Event) -> anyhow::Result<Option<Action>> {
    match mode {
        Mode::Normal => handle_normal_mode(ev),
        Mode::Insert => handle_insert_mode(ev),
    }
}

fn handle_normal_mode(ev: event::Event) -> anyhow::Result<Option<Action>> {
    match ev {
        event::Event::Key(event) => match event.code {
            event::KeyCode::Char('q') => Ok(Some(Action::Quit)),
            event::KeyCode::Char('h') => Ok(Some(Action::MoveLeft)),
            event::KeyCode::Char('j') => Ok(Some(Action::MoveDown)),
            event::KeyCode::Char('k') => Ok(Some(Action::MoveUp)),
            event::KeyCode::Char('l') => Ok(Some(Action::MoveRight)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn handle_insert_mode(ev: event::Event) -> anyhow::Result<Option<Action>> {
    unimplemented!("Insert mode is not implemented yet");
}

fn main() -> anyhow::Result<()> {
    let mut stdout = stdout();
    let mut mode = Mode::Normal;
    let mut cx = 0;
    let mut cy = 0;

    // Start the raw mode and enter the alternate screen
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;

    // Clear the Screen
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    // wait to input something
    loop {
        stdout.queue(cursor::MoveTo(cx, cy))?;
        stdout.flush()?;

        if let Some(action) = handle_event(&mode, read()?)? {
            match action {
                Action::Quit => break,
                Action::MoveUp => {
                    cy = cy.saturating_sub(1);
                }
                Action::MoveDown => {
                    cy += 1u16;
                }
                Action::MoveLeft => {
                    cx = cx.saturating_sub(1);
                }
                Action::MoveRight => {
                    cx += 1u16;
                }
            }
        }
    }

    // leave the alternate screen and disable raw mode
    stdout.execute(terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    Ok(())
}
