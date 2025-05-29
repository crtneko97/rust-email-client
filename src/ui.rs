use crossterm::
{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::
{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::{error::Error, io};

pub enum Mode 
{
    Inbox,
    View,
    Compose,
}

pub enum ComposeField 
{
    To,
    Subject,
    Body,
}

pub struct App<F, G> 
{
    items: Vec<(u32, String)>,
    selected: usize,
    mode: Mode,
    view_buffer: String,
    view_scroll: u16,
    compose_to: String,
    compose_subject: String,
    compose_body: String,
    compose_field: ComposeField,
    compose_scroll: u16,
    on_send: F,
    on_view: G,
}

impl<F, G> App<F, G>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
{
    pub fn new(items: Vec<(u32, String)>, on_view: G, on_send: F) -> Self 
    {
        Self 
        {
            items,
            selected: 0,
            mode: Mode::Inbox,
            view_buffer: String::new(),
            view_scroll: 0,
            compose_to: String::new(),
            compose_subject: String::new(),
            compose_body: String::new(),
            compose_field: ComposeField::To,
            compose_scroll: 0,
            on_send,
            on_view,
        }
    }

    pub fn run(mut self) -> Result<(), Box<dyn Error>> 
    {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

        loop 
        {
            term.draw(|f| 
                {
                let size = f.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(65),
                        Constraint::Percentage(25),
                        Constraint::Length(3),
                    ])
                    .split(size);

                let top = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(chunks[0]);

                let list_items: Vec<ListItem> = self
                    .items
                    .iter()
                    .map(|(_, txt)| ListItem::new(txt.clone()))
                    .collect();
                let mut state = ListState::default();
                state.select(Some(self.selected));
                let list = List::new(list_items)
                    .block(Block::default().borders(Borders::ALL).title("Inbox"))
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                    .highlight_symbol(">> ");
                f.render_stateful_widget(list, top[0], &mut state);

                match self.mode 
                {
                    Mode::Inbox => 
                    {
                        f.render_widget(
                            Block::default()
                                .borders(Borders::ALL)
                                .title("Press 'v' to view, 'c' to compose"),
                            top[1],
                        );
                    }
                    Mode::View => 
                    {
                        let p = Paragraph::new(self.view_buffer.as_ref())
                            .block(Block::default().borders(Borders::ALL).title("Message"))
                            .scroll((self.view_scroll, 0));
                        f.render_widget(p, top[1]);
                    }
                    Mode::Compose => 
                    {
                        let fields = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Min(0),
                            ])
                            .split(top[1]);
                        let to_title = if matches!(self.compose_field, ComposeField::To) { "To*" } else { "To" };
                        let p_to = Paragraph::new(self.compose_to.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(to_title));
                        f.render_widget(p_to, fields[0]);
                        let sub_title = if matches!(self.compose_field, ComposeField::Subject) { "Subject*" } else { "Subject" };
                        let p_sub = Paragraph::new(self.compose_subject.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(sub_title));
                        f.render_widget(p_sub, fields[1]);
                        let p_body = Paragraph::new(self.compose_body.as_ref())
                            .block(Block::default().borders(Borders::ALL).title("Body"))
                            .scroll((self.compose_scroll, 0));
                        f.render_widget(p_body, fields[2]);
                    }
                }

                let help = match self.mode 
                {
                    Mode::Compose => "Esc=Cancel Tab=NextField Shift+Tab=PrevField Ctrl+S=Send",
                    _ => "Up/Down=Move  v=View  c=Compose  q=Quit",
                };
                let instr = Paragraph::new(help)
                    .block(Block::default().borders(Borders::ALL).title("Help"));
                f.render_widget(instr, chunks[2]);
            })?;

            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? 
            {
                match self.mode 
                {
                    Mode::Inbox => match code 
                    {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('v') => 
                        {
                            let uid = self.items[self.selected].0;
                            self.view_buffer = (self.on_view)(uid)?;
                            self.view_scroll = 0;
                            self.mode = Mode::View;
                        }
                        KeyCode::Char('c') => 
                        {
                            self.compose_to.clear();
                            self.compose_subject.clear();
                            self.compose_body.clear();
                            self.compose_field = ComposeField::To;
                            self.compose_scroll = 0;
                            self.mode = Mode::Compose;
                        }
                        KeyCode::Down => 
                        {
                            if !self.items.is_empty() 
                            {
                                self.selected = (self.selected + 1) % self.items.len();
                            }
                        }
                        KeyCode::Up => 
                        {
                            if self.selected > 0 {
                                self.selected -= 1;
                            }
                        }
                        _ => {}
                    },
                    Mode::View => match code 
                    {
                        KeyCode::Esc => self.mode = Mode::Inbox,
                        KeyCode::Down => self.view_scroll = self.view_scroll.saturating_add(1),
                        KeyCode::Up => self.view_scroll = self.view_scroll.saturating_sub(1),
                        _ => {}
                    },
                    Mode::Compose => match (code, modifiers) 
                    {
                        (KeyCode::Esc, _) => self.mode = Mode::Inbox,
                        (KeyCode::Tab, _) => {
                            self.compose_field = match self.compose_field 
                            {
                                ComposeField::To => ComposeField::Subject,
                                ComposeField::Subject => ComposeField::Body,
                                ComposeField::Body => ComposeField::To,
                            };
                        }
                        (KeyCode::BackTab, _) => {
                            self.compose_field = match self.compose_field 
                            {
                                ComposeField::To => ComposeField::Body,
                                ComposeField::Subject => ComposeField::To,
                                ComposeField::Body => ComposeField::Subject,
                            };
                        }
                        (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                            (self.on_send)(&self.compose_to, &self.compose_subject, &self.compose_body)?;
                            self.mode = Mode::Inbox;
                        }
                        (KeyCode::Char(c), _) => {
                            match self.compose_field 
                            {
                                ComposeField::To => self.compose_to.push(c),
                                ComposeField::Subject => self.compose_subject.push(c),
                                ComposeField::Body => self.compose_body.push(c),
                            }
                        }
                        (KeyCode::Backspace, _) => {
                            match self.compose_field 
                            {
                                ComposeField::To => { self.compose_to.pop(); }
                                ComposeField::Subject => { self.compose_subject.pop(); }
                                ComposeField::Body => { self.compose_body.pop(); }
                            }
                        }
                        (KeyCode::Enter, _) => {
                            if let ComposeField::Body = self.compose_field 
                            {
                                self.compose_body.push('\n');
                            } 
                            else 
                            {
                                self.compose_field = match self.compose_field 
                                {
                                    ComposeField::To => ComposeField::Subject,
                                    ComposeField::Subject => ComposeField::Body,
                                    ComposeField::Body => ComposeField::To,
                                };
                            }
                        }
                        (KeyCode::Down, _) => if let ComposeField::Body = self.compose_field 
                        {
                            self.compose_scroll = self.compose_scroll.saturating_add(1);
                        },
                        (KeyCode::Up, _) => if let ComposeField::Body = self.compose_field 
                        {
                            self.compose_scroll = self.compose_scroll.saturating_sub(1);
                        },
                        _ => {}
                    },
                }
            }
        }

        disable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen)?;
        Ok(())
    }
}

