use std::{
    collections::HashMap,
    io::{stdout, Write},
    mem, usize,
};

use serde::{Deserialize, Serialize};

use crossterm::{
    cursor::{self, Hide, MoveTo, Show},
    event::{self, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{self, Color, StyledContent, Stylize},
    terminal::{self, Clear, ClearType},
    ExecutableCommand, QueueableCommand,
};

use crate::{
    buffer::Buffer,
    config::KeyAction,
    highlighter::Highlighter,
    theme::{Style, Theme},
};

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Action {
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
    DeleteLineAt(usize),

    NewLine,

    EnterMode(Mode),
    SetWaitingKeyAction(Box<KeyAction>),
    InsertLineAt(usize, Option<String>),
    MoveLineToViewportCenter,
    InsertLineAtCursor,
    InsertLineBelowCursor,
    MoveToBottom,
    MoveToTop,
    RemoveCharAt(usize, usize),
    UndoMultiple(Vec<Action>),
    DeletePreviousChar,
}

impl Action {}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    Insert,
}

#[derive(Debug, Clone)]
pub struct StyleInfo {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

impl StyleInfo {
    pub fn contains(&self, pos: usize) -> bool {
        pos >= self.start && pos < self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    c: char,
    style: Style,
}

#[derive(Debug, Clone)]
pub struct RenderBuffer {
    cells: Vec<Cell>,
    width: usize,
    #[allow(unused)]
    height: usize,
}

impl RenderBuffer {
    #[allow(unused)]
    fn new_with_contents(width: usize, height: usize, style: Style, contents: Vec<String>) -> Self {
        let mut cells = vec![];
        for line in contents {
            for c in line.chars() {
                cells.push(Cell {
                    c,
                    style: style.clone(),
                });
            }
            for _ in 0..width.saturating_sub(line.len()) {
                cells.push(Cell {
                    c: ' ',
                    style: style.clone(),
                });
            }
        }
        RenderBuffer {
            cells,
            width,
            height,
        }
    }

    fn new(width: usize, height: usize, default_style: Style) -> Self {
        let cells = vec![
            Cell {
                c: ' ',
                style: default_style.clone(),
            };
            width * height
        ];

        RenderBuffer {
            cells,
            width,
            height,
        }
    }

    fn set_char(&mut self, x: usize, y: usize, c: char, style: &Style) {
        let pos = (y * self.width) + x;
        self.cells[pos] = Cell {
            c,
            style: style.clone(),
        };
    }

    fn set_text(&mut self, x: usize, y: usize, s: &str, style: &Style) {
        let pos = (y * self.width) + x;
        for (i, c) in s.chars().enumerate() {
            self.cells[pos + i] = Cell {
                c,
                style: style.clone(),
            };
        }
    }

    fn diff(&self, other: &RenderBuffer) -> Vec<Change> {
        let mut changes = vec![];

        for (pos, cell) in self.cells.iter().enumerate() {
            if *cell != other.cells[pos] {
                let y = pos / self.width;
                let x = pos % self.width;
                changes.push(Change { x, y, cell });
            }
        }

        changes
    }
}

pub struct Change<'a> {
    x: usize,
    y: usize,
    cell: &'a Cell,
}

pub struct Editor {
    config: Config,
    theme: Theme,
    highlighter: Highlighter,
    buffer: Buffer,
    stdout: std::io::Stdout,
    size: (u16, u16),
    vtop: usize,
    vleft: usize,
    cx: usize,
    cy: usize,
    vx: usize,
    mode: Mode,
    waiting_key_action: Option<KeyAction>,
    undo_actions: Vec<Action>,
    insert_undo_actions: Vec<Action>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        _ = self.stdout.flush();
        _ = self.stdout.execute(terminal::LeaveAlternateScreen);
        _ = terminal::disable_raw_mode();
    }
}

impl Editor {
    pub fn with_size(
        width: usize,
        height: usize,
        config: Config,
        theme: Theme,
        buffer: Buffer,
    ) -> anyhow::Result<Self> {
        let stdout = stdout();

        let vx = buffer.len().to_string().len() + 2;
        let size = (width as u16, height as u16);
        let highlighter = Highlighter::new(&theme)?;

        Ok(Editor {
            config,
            theme,
            highlighter,
            buffer,
            stdout,
            vtop: 0,
            vleft: 0,
            cx: 0,
            cy: 0,
            vx,
            mode: Mode::Normal,
            size,
            waiting_key_action: None,
            undo_actions: vec![],
            insert_undo_actions: vec![],
        })
    }

    pub fn new(config: Config, theme: Theme, buffer: Buffer) -> anyhow::Result<Self> {
        let size = terminal::size()?;
        Self::with_size(size.0 as usize, size.1 as usize, config, theme, buffer)
    }

    fn vheight(&self) -> usize {
        self.size.1 as usize - 2
    }

    fn vwidth(&self) -> usize {
        self.size.0 as usize
    }

    fn line_length(&self) -> usize {
        if let Some(line) = self.viewport_line(self.cy) {
            return line.len();
        }
        0
    }

    fn buffer_line(&self) -> usize {
        self.vtop + self.cy as usize
    }

    fn viewport_line(&self, n: usize) -> Option<String> {
        let buffer_line = self.vtop + n;
        self.buffer.get(buffer_line)
    }

    fn set_cursor_style(&mut self) -> anyhow::Result<()> {
        self.stdout.queue(match self.waiting_key_action {
            Some(_) => cursor::SetCursorStyle::SteadyUnderScore,
            _ => match self.mode {
                Mode::Normal => cursor::SetCursorStyle::DefaultUserShape,
                Mode::Insert => cursor::SetCursorStyle::SteadyBar,
            },
        })?;

        Ok(())
    }

    pub fn highlight(&mut self, code: &str) -> anyhow::Result<Vec<StyleInfo>> {
        self.highlighter.highlight(code)
    }

    fn fill_line(&mut self, buffer: &mut RenderBuffer, x: usize, y: usize, style: &Style) {
        let width = self.vwidth().saturating_sub(x);
        let line_fill = " ".repeat(width);
        buffer.set_text(x, y, &line_fill, style);
    }

    pub fn draw_viewport(&mut self, buffer: &mut RenderBuffer) -> anyhow::Result<()> {
        let vbuffer = self.buffer.viewport(self.vtop, self.vheight() as usize);
        let style_info = self.highlight(&vbuffer)?;
        let vheight = self.vheight();
        let default_style = self.theme.style.clone();

        let mut x = self.vx;
        let mut y = 0;
        let mut iter = vbuffer.chars().enumerate().peekable();

        while let Some((pos, c)) = iter.next() {
            if c == '\n' || iter.peek().is_none() {
                if c != '\n' {
                    buffer.set_char(x, y, c, &default_style);
                    x += 1;
                }
                self.fill_line(buffer, x, y, &default_style);
                x = self.vx;
                y += 1;
                if y > vheight {
                    break;
                }
                continue;
            }

            if x < self.vwidth() {
                if let Some(style) = determine_style_for_position(&style_info, pos) {
                    buffer.set_char(x, y, c, &style);
                } else {
                    buffer.set_char(x, y, c, &default_style);
                }
            }

            x += 1;
        }

        while y < vheight {
            self.fill_line(buffer, 0, y, &default_style);
            y += 1;
        }

        self.draw_gutter(buffer);

        Ok(())
    }

    fn gutter_width(&self) -> usize {
        let len = self.buffer.len().to_string().len();
        len + 1
    }

    fn draw_gutter(&mut self, buffer: &mut RenderBuffer) {
        let width = self.gutter_width();
        let fg = self
            .theme
            .gutter_style
            .fg
            .unwrap_or(self.theme.style.fg.expect("fg is defined for theme"));
        let bg = self
            .theme
            .gutter_style
            .bg
            .unwrap_or(self.theme.style.bg.expect("bg is defined for theme"));

        for n in 0..self.vheight() as usize {
            let line_number = n + 1 + self.vtop as usize;

            let text = if line_number <= self.buffer.len() {
                line_number.to_string()
            } else {
                " ".repeat(width)
            };

            buffer.set_text(
                0,
                n,
                &format!("{text:>width$} ", width = width,),
                &Style {
                    fg: Some(fg),
                    bg: Some(bg),
                    ..Default::default()
                },
            );
        }
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        // self.stdout.queue(cursor::Hide)?;
        // self.set_cursor_style()?;
        // self.draw_gutter()?;
        // self.draw_viewport()?;
        // self.draw_statusline()?;
        // self.stdout
        //     .queue(cursor::MoveTo(self.vx + self.cx, self.cy))?;
        // self.stdout.queue(cursor::Show)?;
        // self.stdout.flush()?;

        todo!();

        // Ok(())
    }

    fn draw_cursor(&mut self, buffer: &mut RenderBuffer) -> anyhow::Result<()> {
        self.set_cursor_style()?;
        self.stdout
            .queue(cursor::MoveTo((self.vx + self.cx) as u16, self.cy as u16))?;
        self.draw_statusline(buffer);
        Ok(())
    }

    pub fn draw_statusline(&mut self, buffer: &mut RenderBuffer) {
        let mode = format!(" {:?} ", self.mode).to_uppercase();
        let file = format!(" {}", self.buffer.file.as_deref().unwrap_or("No Name"));
        let pos = format!(" {}:{} ", self.cx + 1, self.cy + self.vtop + 1);

        let file_width = self.size.0 - mode.len() as u16 - pos.len() as u16 - 2;
        let y = self.size.1 as usize - 2;

        let transition_style = Style {
            fg: self.theme.statusline_style.outer_style.bg,
            bg: self.theme.statusline_style.inner_style.bg,
            ..Default::default()
        };

        buffer.set_text(0, y, &mode, &self.theme.statusline_style.outer_style);

        buffer.set_text(
            mode.len(),
            y,
            &self.theme.statusline_style.outer_chars[1].to_string(),
            &transition_style,
        );

        buffer.set_text(
            mode.len() + 1,
            y,
            &format!("{:<width$}", file, width = file_width as usize),
            &self.theme.statusline_style.inner_style,
        );

        buffer.set_text(
            mode.len() + 1 + file_width as usize,
            y,
            &self.theme.statusline_style.outer_chars[2].to_string(),
            &transition_style,
        );

        buffer.set_text(
            mode.len() + 2 + file_width as usize,
            y,
            &pos,
            &self.theme.statusline_style.outer_style,
        );
    }

    fn is_insert(&self) -> bool {
        matches!(self.mode, Mode::Insert)
    }

    fn check_bounds(&mut self) {
        let line_length = self.line_length();

        if self.cx >= line_length && !self.is_insert() {
            if line_length > 0 {
                self.cx = self.line_length() - 1;
            } else if !self.is_insert() {
                self.cx = 0;
            }
        }
        if self.cx >= self.vwidth() {
            self.cx = self.vwidth() - 1;
        }

        let line_on_buffer = self.cy as usize + self.vtop;
        if line_on_buffer > self.buffer.len().saturating_sub(1) {
            self.cy = self.buffer.len() - self.vtop - 1;
        }
    }

    fn render_diff(&mut self, change_set: Vec<Change>) -> anyhow::Result<()> {
        for change in change_set {
            let x = change.x;
            let y = change.y;
            let cell = change.cell;

            self.stdout.queue(MoveTo(x as u16, y as u16))?;
            if let Some(bg) = cell.style.bg {
                self.stdout.queue(style::SetBackgroundColor(bg))?;
            }
            if let Some(fg) = cell.style.fg {
                self.stdout.queue(style::SetForegroundColor(fg))?;
            }
            self.stdout.queue(style::Print(cell.c))?;
        }

        self.set_cursor_style()?;
        self.stdout
            .queue(cursor::MoveTo((self.vx + self.cx) as u16, self.cy as u16))?
            .flush()?;

        Ok(())
    }

    // Draw the current render buffer to the terminal
    fn render(&mut self, buffer: &mut RenderBuffer) -> anyhow::Result<()> {
        self.draw_viewport(buffer)?;
        self.draw_gutter(buffer);
        self.draw_statusline(buffer);

        self.stdout
            .queue(Clear(ClearType::All))?
            .queue(cursor::MoveTo(0, 0))?;

        let mut current_style = &self.theme.style;

        for cell in buffer.cells.iter() {
            if cell.style != *current_style {
                if let Some(bg) = cell.style.bg {
                    self.stdout.queue(style::SetBackgroundColor(bg))?;
                }
                if let Some(fg) = cell.style.fg {
                    self.stdout.queue(style::SetForegroundColor(fg))?;
                }
                current_style = &cell.style;
            }
            self.stdout.queue(style::Print(cell.c))?;
        }

        self.draw_cursor(buffer)?;
        self.stdout.flush()?;

        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        terminal::enable_raw_mode()?;
        self.stdout
            .execute(terminal::EnterAlternateScreen)?
            .execute(terminal::Clear(terminal::ClearType::All))?;

        let mut buffer = RenderBuffer::new(
            self.size.0 as usize,
            self.size.1 as usize,
            self.theme.style.clone(),
        );

        self.render(&mut buffer)?;

        loop {
            let current_buffer = buffer.clone();
            self.check_bounds();

            let ev = read()?;

            if let event::Event::Resize(width, height) = ev {
                self.size = (width, height);
                buffer = RenderBuffer::new(
                    self.size.0 as usize,
                    self.size.1 as usize,
                    self.theme.style.clone(),
                );
                self.render(&mut buffer)?;
                continue;
            }

            if let Some(action) = self.handle_event(ev) {
                let quit = match action {
                    KeyAction::Single(action) => self.execute(&action, &mut buffer)?,
                    KeyAction::Multiple(actions) => {
                        let mut quit = false;
                        for action in actions {
                            if self.execute(&action, &mut buffer)? {
                                quit = true;
                                break;
                            }
                        }
                        quit
                    }
                    KeyAction::Nested(actions) => {
                        self.waiting_key_action = Some(KeyAction::Nested(actions));
                        false
                    }
                };

                if quit {
                    break;
                }
            }

            self.stdout.execute(Hide)?;
            self.draw_statusline(&mut buffer);
            self.render_diff(buffer.diff(&current_buffer))?;
            self.draw_cursor(&mut buffer)?;
            self.stdout.execute(Show)?;
        }

        Ok(())
    }

    fn handle_event(&mut self, ev: event::Event) -> Option<KeyAction> {
        if let event::Event::Resize(width, height) = ev {
            self.size = (width, height);
            return None;
        }

        if let Some(ka) = self.waiting_key_action.take() {
            return self.handle_waiting_command(ka, ev);
        }

        match self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Insert => self.handle_insert_event(ev),
        }
    }

    fn handle_insert_event(&mut self, ev: event::Event) -> Option<KeyAction> {
        if let Some(ka) = event_to_key_action(&self.config.keys.insert, &ev) {
            return Some(ka);
        }

        match ev {
            Event::Key(event) => match event.code {
                KeyCode::Char(c) => KeyAction::Single(Action::InsertCharAtCursorPos(c)).into(),
                _ => None,
            },
            _ => None,
        }
    }

    fn handle_normal_event(&mut self, ev: event::Event) -> Option<KeyAction> {
        event_to_key_action(&self.config.keys.normal, &ev)
    }

    fn handle_waiting_command(&mut self, ka: KeyAction, ev: event::Event) -> Option<KeyAction> {
        let KeyAction::Nested(nested_mappings) = ka else {
            panic!("Expected nested key action");
        };

        event_to_key_action(&nested_mappings, &ev)
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

    fn draw_line(&mut self, buffer: &mut RenderBuffer) {
        let line = self.viewport_line(self.cy).unwrap_or_default();
        let style_info = self.highlight(&line).unwrap_or_default();
        let default_style = self.theme.style.clone();

        let mut x = self.vx;
        let mut iter = line.chars().enumerate().peekable();

        while let Some((pos, c)) = iter.next() {
            if c == '\n' || iter.peek().is_none() {
                if c != '\n' {
                    buffer.set_char(x, self.cy, c, &default_style);
                    x += 1;
                }
                self.fill_line(buffer, x, self.cy, &default_style);
                break;
            }

            if x < self.vwidth() {
                if let Some(style) = determine_style_for_position(&style_info, pos) {
                    buffer.set_char(x, self.cy, c, &style);
                } else {
                    buffer.set_char(x, self.cy, c, &default_style);
                }
            }
            x += 1;
        }
    }

    fn execute(&mut self, action: &Action, buffer: &mut RenderBuffer) -> anyhow::Result<bool> {
        match action {
            Action::Quit => return Ok(true),
            Action::MoveUp => {
                if self.cy == 0 {
                    if self.vtop > 0 {
                        self.vtop -= 1;
                        self.draw_viewport(buffer)?;
                    }
                } else {
                    self.cy = self.cy.saturating_sub(1);
                }
            }
            Action::MoveDown => {
                self.cy += 1;
                if self.cy >= self.vheight() {
                    self.vtop += 1;
                    self.cy -= 1;
                    self.draw_viewport(buffer)?;
                }
            }
            Action::MoveLeft => {
                self.cx = self.cx.saturating_sub(1);
                if self.cx < self.vleft {
                    self.cx = self.vleft;
                }
            }
            Action::MoveRight => {
                self.cx += 1;
            }
            Action::MoveToLineStart => {
                self.cx = 0;
            }
            Action::MoveToLineEnd => {
                self.cx = self.line_length().saturating_sub(1);
            }
            Action::PageUp => {
                if self.vtop > 0 {
                    self.vtop = self.vtop.saturating_sub(self.vheight() as usize);
                    self.draw_viewport(buffer)?;
                }
            }
            Action::PageDown => {
                if self.buffer.len() > self.vtop + self.vheight() as usize {
                    self.vtop += self.vheight() as usize;
                }
            }
            Action::EnterMode(new_mode) => {
                if !self.is_insert() && matches!(new_mode, Mode::Insert) {
                    self.insert_undo_actions = Vec::new();
                }
                if self.is_insert() && matches!(new_mode, Mode::Normal) {
                    if !self.insert_undo_actions.is_empty() {
                        let actions = mem::take(&mut self.insert_undo_actions);
                        self.undo_actions.push(Action::UndoMultiple(actions));
                    }
                }
                self.mode = *new_mode;
                self.draw_statusline(buffer);
            }
            Action::InsertCharAtCursorPos(c) => {
                self.insert_undo_actions
                    .push(Action::RemoveCharAt(self.cx, self.buffer_line()));
                self.buffer.insert(self.cx, self.buffer_line(), *c);
                self.cx += 1;
                self.draw_line(buffer);
            }
            Action::RemoveCharAt(cx, line) => {
                self.buffer.remove(*cx, *line);
                self.draw_line(buffer);
            }
            Action::DeleteCharAtCursorPos => {
                self.buffer.remove(self.cx, self.buffer_line());
                self.draw_line(buffer);
            }
            Action::NewLine => {
                self.cx = 0;
                self.cy += 1;
                self.buffer.insert_line(self.buffer_line(), String::new());
                self.draw_viewport(buffer)?;
            }
            Action::SetWaitingKeyAction(key_action) => {
                self.waiting_key_action = Some(*(key_action.clone()));
            }
            Action::DeleteCurrentLine => {
                let line = self.buffer_line();
                let contents = self.current_line_contents();

                self.buffer.remove_line(self.buffer_line());
                self.undo_actions.push(Action::InsertLineAt(line, contents));
                self.draw_viewport(buffer)?;
            }
            Action::Undo => {
                if let Some(undo_action) = self.undo_actions.pop() {
                    self.execute(&undo_action, buffer)?;
                };
            }
            Action::InsertLineAt(y, contents) => {
                if let Some(contents) = contents {
                    self.buffer.insert_line(*y, contents.to_string());
                    self.draw_viewport(buffer)?;
                }
            }
            Action::MoveLineToViewportCenter => {
                let viewport_center = self.vheight() / 2;
                let distance_to_center = self.cy as isize - viewport_center as isize;

                if distance_to_center > 0 {
                    // if distance_to_center is negative, we need to move the scroll up
                    let distance_to_center = distance_to_center.abs() as usize;
                    if self.vtop > distance_to_center {
                        let new_vtop = self.vtop + distance_to_center;
                        self.vtop = new_vtop;
                        self.cy = viewport_center;
                        self.draw_viewport(buffer)?;
                    }
                } else if distance_to_center < 0 {
                    // if distance_to_center is negative, we need to move the scroll down
                    let distance_to_center = distance_to_center.abs() as usize;
                    let distance_to_go = self.vtop + distance_to_center;
                    let new_vtop = self.vtop.saturating_sub(distance_to_center);
                    if self.buffer.len() > distance_to_go && new_vtop != self.vtop {
                        self.vtop = new_vtop;
                        self.cy = viewport_center;
                        self.draw_viewport(buffer)?;
                    }
                }
            }
            Action::InsertLineAtCursor => {
                self.undo_actions
                    .push(Action::DeleteLineAt(self.buffer_line()));
                self.buffer.insert_line(self.buffer_line(), String::new());
                self.cx = 0;
                self.draw_viewport(buffer)?;
            }
            Action::InsertLineBelowCursor => {
                self.undo_actions
                    .push(Action::DeleteLineAt(self.buffer_line() + 1));
                self.buffer
                    .insert_line(self.buffer_line() + 1, String::new());
                self.cy += 1;
                self.cx = 0;
                self.draw_viewport(buffer)?;
            }
            Action::MoveToTop => {
                self.vtop = 0;
                self.cy = 0;
                self.draw_viewport(buffer)?;
            }
            Action::MoveToBottom => {
                if self.buffer.len() > self.vheight() as usize {
                    self.vtop = self.buffer.len() - self.vheight() as usize;
                    self.cy = self.vheight() - 1;
                    self.draw_viewport(buffer)?;
                } else {
                    self.cy = self.buffer.len() - 1;
                }
            }
            Action::UndoMultiple(actions) => {
                for action in actions.iter().rev() {
                    self.execute(&action, buffer)?;
                }
            }
            Action::DeleteLineAt(y) => {
                self.buffer.remove_line(*y);
                self.draw_viewport(buffer)?;
            }
            Action::DeletePreviousChar => {
                if self.cx > 0 {
                    self.cx -= 1;
                    self.buffer.remove(self.cx, self.buffer_line());
                    self.draw_line(buffer);
                }
            }
        }

        Ok(false)
    }
}

fn event_to_key_action(mappings: &HashMap<String, KeyAction>, ev: &Event) -> Option<KeyAction> {
    match ev {
        event::Event::Key(KeyEvent {
            code, modifiers, ..
        }) => {
            let key = match code {
                // KeyCode::Char('q') => return Ok(Some(Action::Quit)),
                KeyCode::Char(c) => format!("{c}"),
                _ => format!("{code:?}"),
            };

            let key = match *modifiers {
                KeyModifiers::CONTROL => format!("Ctrl-{key}"),
                KeyModifiers::ALT => format!("ALT-{key}"),
                _ => key,
            };

            mappings.get(&key).cloned()
        }
        _ => None,
    }
}

fn determine_style_for_position(style_info: &Vec<StyleInfo>, pos: usize) -> Option<Style> {
    if let Some(s) = style_info.iter().find(|ci| ci.contains(pos)) {
        return Some(s.style.clone());
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_set_char() {
        let mut buffer = RenderBuffer::new(2, 2, Style::default());
        buffer.set_char(2, 2, 'a', &Style::default());
        // assert_eq!(buffer.cells[0].c, 'a');
    }

    #[test]
    fn test_set_text() {
        let mut buffer = RenderBuffer::new(3, 15, Style::default());
        buffer.set_text(
            2,
            2,
            "Hello, world!",
            &Style {
                fg: Some(Color::Rgb { r: 0, g: 0, b: 0 }),
                bg: Some(Color::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                }),
                bold: false,
                italic: true,
            },
        );
        let start = 2 * 3 + 2;
        assert_eq!(buffer.cells[start].c, 'H');
        assert_eq!(
            buffer.cells[start].style.fg,
            Some(Color::Rgb { r: 0, g: 0, b: 0 })
        );
        assert_eq!(
            buffer.cells[start].style.bg,
            Some(Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(buffer.cells[start].style.italic, true);
        assert_eq!(buffer.cells[start + 1].c, 'e');
        assert_eq!(buffer.cells[start + 2].c, 'l');
        assert_eq!(buffer.cells[start + 3].c, 'l');
        assert_eq!(buffer.cells[start + 4].c, 'o');
        assert_eq!(buffer.cells[start + 5].c, ',');
        assert_eq!(buffer.cells[start + 6].c, ' ');
        assert_eq!(buffer.cells[start + 7].c, 'w');
        assert_eq!(buffer.cells[start + 8].c, 'o');
        assert_eq!(buffer.cells[start + 9].c, 'r');
        assert_eq!(buffer.cells[start + 10].c, 'l');
        assert_eq!(buffer.cells[start + 11].c, 'd');
        assert_eq!(buffer.cells[start + 12].c, '!');
    }

    #[test]
    fn test_diff() {
        let buffer1 = RenderBuffer::new(3, 3, Style::default());
        let mut buffer2 = RenderBuffer::new(3, 3, Style::default());
        buffer2.set_char(
            0,
            0,
            'a',
            &Style {
                fg: Some(Color::Rgb { r: 0, g: 0, b: 0 }),
                bg: Some(Color::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                }),
                bold: false,
                italic: false,
            },
        );
        let diff = buffer2.diff(&buffer1);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].x, 0);
        assert_eq!(diff[0].y, 0);
        assert_eq!(diff[0].cell.c, 'a');
    }

    #[test]
    fn test_draw_viewport() {
        let contents = "hello\nworld!";
        let config = Config::default();
        let theme = Theme::default();
        let buffer = Buffer::new(None, contents.to_string());
        // log!("buffer: {buffer:?}");
        let mut render_buffer = RenderBuffer::new(10, 10, Style::default());
        let mut editor = Editor::with_size(10, 10, config, theme, buffer).unwrap();
        editor.draw_viewport(&mut render_buffer).unwrap();
        // println!("{}", render_buffer.dump());
        assert_eq!(render_buffer.cells[0].c, ' ');
        assert_eq!(render_buffer.cells[1].c, '1');
        assert_eq!(render_buffer.cells[2].c, ' ');
        assert_eq!(render_buffer.cells[3].c, 'h');
        assert_eq!(render_buffer.cells[4].c, 'e');
        assert_eq!(render_buffer.cells[5].c, 'l');
        assert_eq!(render_buffer.cells[6].c, 'l');
        assert_eq!(render_buffer.cells[7].c, 'o');
        assert_eq!(render_buffer.cells[8].c, ' ');
        assert_eq!(render_buffer.cells[9].c, ' ');
    }

    #[test]
    fn test_buffer_diff() {
        let contents1 = vec![" 1:2 ".to_string()];
        let contents2 = vec![" 1:3 ".to_string()];
        let buffer1 = RenderBuffer::new_with_contents(5, 1, Style::default(), contents1);
        let buffer2 = RenderBuffer::new_with_contents(5, 1, Style::default(), contents2);
        let diff = buffer2.diff(&buffer1);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].x, 3);
        assert_eq!(diff[0].y, 0);
        assert_eq!(diff[0].cell.c, '3');
    }
}
