use std::{
    io::{stdout, Write},
    usize,
};

use crossterm::{
    cursor,
    event::{self, read, Event, KeyEventKind},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};

use crate::buffer::Buffer;
use crate::log;

enum Action {
    Quit,

    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,

    AddChar(char),
    NewLine,

    EnterMode(Mode),
}

#[derive(Debug)]
enum Mode {
    Normal,
    Insert,
}

pub struct Editor {
    buffer: Buffer,
    stdout: std::io::Stdout,
    size: (u16, u16),
    vtop: u16,
    vleft: u16,
    cx: u16,
    cy: u16,
    mode: Mode,
}

impl Drop for Editor {
    fn drop(&mut self) {
        _ = self.stdout.flush();
        _ = self.stdout.execute(terminal::LeaveAlternateScreen);
        _ = terminal::disable_raw_mode();
    }
}

impl Editor {
    pub fn new(buffer: Buffer) -> anyhow::Result<Self> {
        let mut stdout = stdout();

        terminal::enable_raw_mode()?;
        stdout
            .execute(terminal::EnterAlternateScreen)?
            .execute(terminal::Clear(terminal::ClearType::All))?;

        Ok(Editor {
            buffer,
            stdout,
            vtop: 0,
            vleft: 0,
            cx: 0,
            cy: 0,
            mode: Mode::Normal,
            size: terminal::size()?,
        })
    }

    fn vheight(&self) -> u16 {
        self.size.1 - 2
    }

    fn vwidth(&self) -> u16 {
        self.size.0
    }

    fn line_length(&self) -> u16 {
        if let Some(line) = self.viewport_line(self.cy) {
            return line.len() as u16;
        }
        0
    }

    fn viewport_line(&self, n: u16) -> Option<String> {
        let buffer_line = self.vtop + n;
        self.buffer.get(buffer_line as usize)
    }

    pub fn draw_viewport(&mut self) -> anyhow::Result<()> {
        let vwidth = self.vwidth() as usize;
        for i in 0..self.vheight() {
            let line = match self.viewport_line(i) {
                None => String::new(), // clear the line!
                Some(s) => s,          // String
            };

            self.stdout
                .queue(cursor::MoveTo(0, i))?
                .queue(style::Print(format!("{line:<width$}", width = vwidth)))?;
        }

        Ok(())
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.draw_viewport()?;
        self.draw_statusline()?;
        self.stdout.queue(cursor::MoveTo(self.cx, self.cy))?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn draw_statusline(&mut self) -> anyhow::Result<()> {
        let mode = format!(" {:?} ", self.mode).to_uppercase();
        let file = format!(" {}", self.buffer.file.as_deref().unwrap_or("No Name"));
        let pos = format!(" {}:{} ", self.cx, self.cy);

        let file_width = self.size.0 - mode.len() as u16 - pos.len() as u16 - 2;

        self.stdout.queue(cursor::MoveTo(0, self.size.1 - 2))?;
        self.stdout.queue(style::PrintStyledContent(
            mode.with(Color::Rgb { r: 0, g: 0, b: 0 })
                .bold()
                .on(Color::Rgb {
                    r: 184,
                    g: 144,
                    b: 243,
                }),
        ))?;

        self.stdout.queue(style::PrintStyledContent(
            ""
                .with(Color::Rgb {
                    r: 184,
                    g: 144,
                    b: 243,
                })
                .on(Color::Rgb {
                    r: 67,
                    g: 70,
                    b: 89,
                }),
        ))?;

        self.stdout.queue(style::PrintStyledContent(
            format!("{:<width$}", file, width = file_width as usize)
                .with(Color::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                })
                .bold()
                .on(Color::Rgb {
                    r: 67,
                    g: 70,
                    b: 89,
                }),
        ))?;

        self.stdout.queue(style::PrintStyledContent(
            ""
                .with(Color::Rgb {
                    r: 184,
                    g: 144,
                    b: 243,
                })
                .on(Color::Rgb {
                    r: 67,
                    g: 70,
                    b: 89,
                }),
        ))?;

        self.stdout.queue(style::PrintStyledContent(
            pos.with(Color::Rgb { r: 0, g: 0, b: 0 })
                .bold()
                .on(Color::Rgb {
                    r: 184,
                    g: 144,
                    b: 243,
                }),
        ))?;

        self.stdout.flush()?;
        Ok(())
    }

    fn check_bounds(&mut self) {
        let line_length = self.line_length();
        if self.cx >= line_length {
            if line_length > 0 {
                self.cx = self.line_length() - 1;
            } else {
                self.cx = 0;
            }
        }
        if self.cx >= self.vwidth() {
            self.cx = self.vwidth() - 1;
        }

        let line_on_buffer = self.cy + self.vtop;
        if line_on_buffer as usize > self.buffer.len() - 1 {
            self.cy = self.buffer.len() as u16 - self.vtop - 1;
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            self.check_bounds();
            self.draw()?;

            if let Event::Key(event) = read()? {
                if event.kind == KeyEventKind::Press {
                    if let Some(action) = self.handle_event(Event::Key(event))? {
                        match action {
                            Action::Quit => break,
                            Action::MoveUp => {
                                if self.cy == 0 {
                                    if self.vtop > 0 {
                                        self.vtop -= 1;
                                    }
                                } else {
                                    self.cy = self.cy.saturating_sub(1);
                                }
                            }
                            Action::MoveDown => {
                                self.cy += 1u16;
                                if self.cy >= self.vheight() {
                                    self.vtop += 1;
                                    self.cy -= 1;
                                }
                            }
                            Action::MoveLeft => {
                                self.cx = self.cx.saturating_sub(1);
                                if self.cx < self.vleft {
                                    self.cx = self.vleft;
                                }
                            }
                            Action::MoveRight => {
                                self.cx += 1u16;
                            }
                            Action::EnterMode(new_mode) => {
                                self.mode = new_mode;
                            }
                            Action::AddChar(c) => {
                                self.stdout.queue(cursor::MoveTo(self.cx, self.cy))?;
                                self.stdout.queue(style::Print(c))?;
                                self.stdout.flush()?;
                                self.cx += 1u16;
                            }
                            Action::NewLine => {
                                self.cx = 0;
                                self.cy += 1u16;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        match self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Insert => self.handle_insert_event(ev),
        }
    }

    fn handle_normal_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event) => match event.code {
                event::KeyCode::Char('q') => Some(Action::Quit),
                event::KeyCode::Left | event::KeyCode::Char('h') => Some(Action::MoveLeft),
                event::KeyCode::Down | event::KeyCode::Char('j') => Some(Action::MoveDown),
                event::KeyCode::Up | event::KeyCode::Char('k') => Some(Action::MoveUp),
                event::KeyCode::Right | event::KeyCode::Char('l') => Some(Action::MoveRight),
                event::KeyCode::Char('i') => Some(Action::EnterMode(Mode::Insert)),
                _ => None,
            },
            _ => None,
        };

        Ok(action)
    }

    fn handle_insert_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event) => match event.code {
                event::KeyCode::Esc => Some(Action::EnterMode(Mode::Normal)),
                event::KeyCode::Char(c) => Some(Action::AddChar(c)),
                event::KeyCode::Enter => Some(Action::NewLine),
                _ => None,
            },
            _ => None,
        };

        Ok(action)
    }
}
