use std::{
    io::{stdout, Write},
    usize,
};

use crossterm::{
    cursor,
    event::{self, read, Event, KeyEventKind, KeyModifiers},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};

use crate::buffer::Buffer;
use crate::log;

enum Action {
    Undo,
    Quit,

    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,

    MoveToLineStart,
    MoveToLineEnd,

    InsertCharAtCursorPos(char),
    DeleteCharAtCursorPos,
    DeleteCurrentLine,

    NewLine,

    EnterMode(Mode),
    SetWaitingCmd(char),
    InsertLineAt(usize, Option<String>),
    MoveLineToViewportCenter,
}

impl Action {
    fn execute(&self, editor: &mut Editor) {
        match self {
            Action::Quit => {}
            Action::MoveUp => {
                if editor.cy == 0 {
                    if editor.vtop > 0 {
                        editor.vtop -= 1;
                    }
                } else {
                    editor.cy = editor.cy.saturating_sub(1);
                }
            }
            Action::MoveDown => {
                editor.cy += 1u16;
                if editor.cy >= editor.vheight() {
                    editor.vtop += 1;
                    editor.cy -= 1;
                }
            }
            Action::MoveLeft => {
                editor.cx = editor.cx.saturating_sub(1);
                if editor.cx < editor.vleft {
                    editor.cx = editor.vleft;
                }
            }
            Action::MoveRight => {
                editor.cx += 1u16;
            }
            Action::MoveToLineStart => {
                editor.cx = 0;
            }
            Action::MoveToLineEnd => {
                editor.cx = editor.line_length().saturating_sub(1);
            }
            Action::PageUp => {
                if editor.vtop > 0 {
                    editor.vtop = editor.vtop.saturating_sub(editor.vheight() as usize);
                }
            }
            Action::PageDown => {
                if editor.buffer.len() > editor.vtop + editor.vheight() as usize {
                    editor.vtop += editor.vheight() as usize;
                }
            }
            Action::EnterMode(new_mode) => {
                editor.mode = *new_mode;
            }
            Action::InsertCharAtCursorPos(c) => {
                editor.buffer.insert(editor.cx, editor.buffer_line(), *c);
                editor.cx += 1;
            }
            Action::DeleteCharAtCursorPos => {
                if editor.line_length() > 0 {
                    editor.buffer.remove(editor.cx, editor.buffer_line());
                }
            }
            Action::NewLine => {
                editor.cx = 0;
                editor.cy += 1u16;
            }
            Action::SetWaitingCmd(cmd) => {
                editor.waiting_command = Some(*cmd);
            }
            Action::DeleteCurrentLine => {
                let line = editor.buffer_line();
                let contents = editor.current_line_contents();

                editor.buffer.remove_line(editor.buffer_line());
                editor
                    .undo_action
                    .push(Action::InsertLineAt(line, contents));
            }
            Action::Undo => {
                if let Some(undo_action) = editor.undo_action.pop() {
                    undo_action.execute(editor);
                };
            }
            Action::InsertLineAt(y, contents) => {
                if let Some(contents) = contents {
                    editor.buffer.insert_line(*y, contents.to_string());
                }
            }
            Action::MoveLineToViewportCenter => {
                let viewport_center = editor.vheight() / 2;
                let distance_to_center = editor.cy as isize - viewport_center as isize;

                log!("Distance to center: {}", distance_to_center);
                if distance_to_center > 0 {
                    // if distance_to_center is negative, we need to move the scroll up
                    let distance_to_center = distance_to_center.abs() as usize;
                    if editor.vtop > distance_to_center {
                        let new_vtop = editor.vtop + distance_to_center;
                        editor.vtop = new_vtop;
                        editor.cy = viewport_center;
                    }
                } else if distance_to_center < 0 {
                    // if distance_to_center is negative, we need to move the scroll down
                    let distance_to_center = distance_to_center.abs() as usize;
                    let distance_to_go = editor.vtop + distance_to_center;
                    let new_vtop = editor.vtop.saturating_sub(distance_to_center);
                    if editor.buffer.len() > distance_to_go && new_vtop != editor.vtop {
                        editor.vtop = new_vtop;
                        editor.cy = viewport_center;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Mode {
    Normal,
    Insert,
}

pub struct Editor {
    buffer: Buffer,
    stdout: std::io::Stdout,
    size: (u16, u16),
    vtop: usize,
    vleft: u16,
    cx: u16,
    cy: u16,
    mode: Mode,
    waiting_command: Option<char>,
    undo_action: Vec<Action>,
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
            waiting_command: None,
            undo_action: vec![],
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

    fn buffer_line(&self) -> usize {
        self.vtop + self.cy as usize
    }

    fn viewport_line(&self, n: u16) -> Option<String> {
        let buffer_line = self.vtop + n as usize;
        self.buffer.get(buffer_line)
    }

    fn set_cursor_style(&mut self) -> anyhow::Result<()> {
        self.stdout.queue(match self.waiting_command {
            Some(_) => cursor::SetCursorStyle::SteadyUnderScore,
            _ => match self.mode {
                Mode::Normal => cursor::SetCursorStyle::DefaultUserShape,
                Mode::Insert => cursor::SetCursorStyle::SteadyBar,
            },
        })?;

        Ok(())
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
        self.stdout.queue(cursor::Hide)?;
        self.set_cursor_style()?;
        self.draw_viewport()?;
        self.draw_statusline()?;
        self.stdout.queue(cursor::MoveTo(self.cx, self.cy))?;
        self.stdout.queue(cursor::Show)?;
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

        let line_on_buffer = self.cy as usize + self.vtop;
        if line_on_buffer > self.buffer.len() - 1 {
            self.cy = (self.buffer.len() as usize - self.vtop - 1) as u16;
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            self.check_bounds();
            self.draw()?;

            if let Event::Key(event) = read()? {
                if event.kind == KeyEventKind::Press {
                    if let Some(action) = self.handle_event(Event::Key(event))? {
                        if matches!(action, Action::Quit) {
                            break;
                        }
                        action.execute(self);
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
        log!("Event : {:?}", ev);

        if let Some(cmd) = self.waiting_command {
            self.waiting_command = None;
            return self.handle_waiting_command(cmd, ev);
        }

        let action = match ev {
            event::Event::Key(event) => {
                let code = event.code;
                let modifiers = event.modifiers;
                match code {
                    event::KeyCode::Char('u') => Some(Action::Undo),
                    event::KeyCode::Char('q') => Some(Action::Quit),
                    event::KeyCode::Left | event::KeyCode::Char('h') => Some(Action::MoveLeft),
                    event::KeyCode::Down | event::KeyCode::Char('j') => Some(Action::MoveDown),
                    event::KeyCode::Up | event::KeyCode::Char('k') => Some(Action::MoveUp),
                    event::KeyCode::Right | event::KeyCode::Char('l') => Some(Action::MoveRight),
                    event::KeyCode::Char('i') => Some(Action::EnterMode(Mode::Insert)),
                    event::KeyCode::Char('0') | event::KeyCode::Home => {
                        Some(Action::MoveToLineStart)
                    }
                    event::KeyCode::Char('$') | event::KeyCode::End => Some(Action::MoveToLineEnd),
                    event::KeyCode::Char('b') => {
                        if matches!(modifiers, KeyModifiers::CONTROL) {
                            log!("ctrl + b Pressed");
                            Some(Action::PageUp)
                        } else {
                            None
                        }
                    }
                    event::KeyCode::Char('f') => {
                        if matches!(modifiers, KeyModifiers::CONTROL) {
                            log!("ctrl + F Pressed");
                            Some(Action::PageDown)
                        } else {
                            None
                        }
                    }
                    event::KeyCode::Char('x') => Some(Action::DeleteCharAtCursorPos),
                    event::KeyCode::Char('d') => Some(Action::SetWaitingCmd('d')),
                    event::KeyCode::Char('z') => Some(Action::SetWaitingCmd('z')),
                    _ => None,
                }
            }
            _ => None,
        };

        Ok(action)
    }

    fn handle_waiting_command(
        &self,
        cmd: char,
        ev: event::Event,
    ) -> anyhow::Result<Option<Action>> {
        let action = match cmd {
            'd' => match ev {
                event::Event::Key(event) => match event.code {
                    event::KeyCode::Char('d') => Some(Action::DeleteCurrentLine),
                    _ => None,
                },
                _ => None,
            },
            'z' => match ev {
                event::Event::Key(event) => match event.code {
                    event::KeyCode::Char('z') => Some(Action::MoveLineToViewportCenter),
                    _ => None,
                },
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
                event::KeyCode::Char(c) => Some(Action::InsertCharAtCursorPos(c)),
                event::KeyCode::Enter => Some(Action::NewLine),
                _ => None,
            },
            _ => None,
        };

        Ok(action)
    }

    fn current_line_contents(&self) -> Option<String> {
        self.buffer.get(self.buffer_line())
    }

    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        self.stdout.execute(terminal::LeaveAlternateScreen)?;
        self.stdout.execute(cursor::Show)?;
        self.stdout.flush()?;
        Ok(())
    }
}
