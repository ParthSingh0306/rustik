use std::io::{stdout, Write};

use crossterm::{cursor, event::read, terminal, ExecutableCommand, QueueableCommand};

fn main() -> anyhow::Result<()> {
    let mut stdout = stdout();
    let cx = 0;
    let cy = 0;

    // Start the raw mode and enter the alternate screen
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;

    // Clear the Screen
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    // wait to input something
    loop {
        stdout.queue(cursor::MoveTo(cx, cy))?;
        stdout.flush()?;

        match read()? {
            crossterm::event::Event::Key(event) => match event.code {
                crossterm::event::KeyCode::Char('q') => break,
                _ => {}
            },
            _ => {}
        }
    }

    // leave the alternate screen and disable raw mode
    stdout.execute(terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    Ok(())
}
