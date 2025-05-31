use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::{error::Error, io};

pub enum Mode {
    Inbox,
    View,
    Compose,
}

pub enum ComposeField {
    To,
    Subject,
    Body,
}

// ──────────────────────────────────────────────────────────────────────────────
// App now has four generic parameters: 
//   F = on_send, G = on_view, H = on_refresh, J = on_delete.
// We also add two new fields: `on_delete: J` and `inbox_count: usize`.
// ──────────────────────────────────────────────────────────────────────────────
pub struct App<F, G, H, J>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
    H: FnMut(usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> + 'static,
    J: FnMut(u32) -> Result<(), Box<dyn Error>> + 'static,
{
    items: Vec<(u32, String)>, // (UID, "from    date")
    selected: usize,
    mode: Mode,
    view_buffer: String,
    view_scroll: u16,
    compose_to: String,
    compose_subject: String,
    compose_body: String,
    compose_field: ComposeField,
    compose_scroll: u16,
    on_send: F,      // FnMut(&str, &str, &str) -> Result<(), Error>
    on_view: G,      // FnMut(u32) -> Result<String, Error>
    on_refresh: H,   // FnMut(usize) -> Result<Vec<(u32,String)>, Error>
    on_delete: J,    // FnMut(u32) -> Result<(), Error>
    inbox_count: usize, // how many messages to fetch
}

impl<F, G, H, J> App<F, G, H, J>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
    H: FnMut(usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> + 'static,
    J: FnMut(u32) -> Result<(), Box<dyn Error>> + 'static,
{
    /// Now takes six arguments:
    /// 1) items        : Vec<(UID, "from    date")>
    /// 2) on_view      : FnMut(u32) -> Result<String, Error>
    /// 3) on_send      : FnMut(&str, &str, &str) -> Result<(), Error>
    /// 4) on_refresh   : FnMut(usize) -> Result<Vec<(UID,String)>, Error>
    /// 5) on_delete    : FnMut(u32) -> Result<(), Error>
    /// 6) inbox_count  : usize (initial number of messages)
    pub fn new(
        items: Vec<(u32, String)>,
        on_view: G,
        on_send: F,
        on_refresh: H,
        on_delete: J,
        inbox_count: usize,
    ) -> Self {
        Self {
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
            on_refresh,
            on_delete,
            inbox_count,
        }
    }

    pub fn run(mut self) -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

        loop {
            term.draw(|f| {
                // ────────────────────────────────────────────────────────────────
                // 1) SPLIT THE ENTIRE FRAME INTO TWO ROWS (70% | 30%)
                // ────────────────────────────────────────────────────────────────
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                    .split(f.size());

                // ────────────────────────────────────────────────────────────────
                // 2) TOP PANEL: SHOW “View”, “Compose”, or a placeholder
                // ────────────────────────────────────────────────────────────────
                match self.mode {
                    Mode::Inbox => {
                        let placeholder = Paragraph::new(
                            "Press 'v' to view, 'c' to compose, 'm' to load more, 'd' to delete, 'q' to quit",
                        )
                        .block(Block::default().borders(Borders::ALL).title("Instructions"))
                        .wrap(Wrap { trim: true });
                        f.render_widget(placeholder, rows[0]);
                    }
                    Mode::View => {
                        let p = Paragraph::new(self.view_buffer.as_ref())
                            .block(Block::default().borders(Borders::ALL).title("Message"))
                            .wrap(Wrap { trim: true })
                            .scroll((self.view_scroll, 0));
                        f.render_widget(p, rows[0]);
                    }
                    Mode::Compose => {
                        let compose_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Min(0),
                            ])
                            .split(rows[0]);

                        let to_title = if matches!(self.compose_field, ComposeField::To) {
                            "To*"
                        } else {
                            "To"
                        };
                        let p_to = Paragraph::new(self.compose_to.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(to_title));
                        f.render_widget(p_to, compose_chunks[0]);

                        let sub_title = if matches!(self.compose_field, ComposeField::Subject) {
                            "Subject*"
                        } else {
                            "Subject"
                        };
                        let p_sub = Paragraph::new(self.compose_subject.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(sub_title));
                        f.render_widget(p_sub, compose_chunks[1]);

                        let p_body = Paragraph::new(self.compose_body.as_ref())
                            .block(Block::default().borders(Borders::ALL).title("Body"))
                            .wrap(Wrap { trim: true })
                            .scroll((self.compose_scroll, 0));
                        f.render_widget(p_body, compose_chunks[2]);
                    }
                }

                // ────────────────────────────────────────────────────────────────
                // 3) BOTTOM PANEL: INBOX LIST (always)
                // ────────────────────────────────────────────────────────────────
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
                f.render_stateful_widget(list, rows[1], &mut state);
            })?;

            // ────────────────────────────────────────────────────────────────────
            // 4) HANDLE INPUT (Inbox / View / Compose)
            // ────────────────────────────────────────────────────────────────────
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                match self.mode {
                    Mode::Inbox => match code {
                        KeyCode::Char('q') => break,

                        KeyCode::Char('v') => {
                            let uid = self.items[self.selected].0;
                            self.view_buffer = (self.on_view)(uid)?;
                            self.view_scroll = 0;
                            self.mode = Mode::View;
                        }

                        KeyCode::Char('c') => {
                            self.compose_to.clear();
                            self.compose_subject.clear();
                            self.compose_body.clear();
                            self.compose_field = ComposeField::To;
                            self.compose_scroll = 0;
                            self.mode = Mode::Compose;
                        }

                        // ─── Press 'm' to load MORE emails ───
                        KeyCode::Char('m') => {
                            // Increase how many we fetch by 10
                            self.inbox_count += 10;
                            // Re-fetch with new count
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            // Reset highlight to top
                            self.selected = 0;
                        }

                        // ─── Press 'd' to DELETE the selected message ───
                        KeyCode::Char('d') => {
                            let uid = self.items[self.selected].0;
                            // Call delete_message(uid):
                            (self.on_delete)(uid)?;
                            // Re-fetch the inbox with the same count so the deleted message disappears
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            // Reset highlight to top
                            self.selected = 0;
                        }

                        KeyCode::Down => {
                            if !self.items.is_empty() {
                                self.selected = (self.selected + 1) % self.items.len();
                            }
                        }
                        KeyCode::Up => {
                            if self.selected > 0 {
                                self.selected -= 1;
                            }
                        }
                        _ => {}
                    },

                    Mode::View => match code {
                        KeyCode::Esc => self.mode = Mode::Inbox,
                        KeyCode::Down => {
                            self.view_scroll = self.view_scroll.saturating_add(1)
                        }
                        KeyCode::Up => {
                            self.view_scroll = self.view_scroll.saturating_sub(1)
                        }
                        _ => {}
                    },

                    Mode::Compose => match (code, modifiers) {
                        (KeyCode::Esc, _) => self.mode = Mode::Inbox,
                        (KeyCode::Tab, _) => {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Subject,
                                ComposeField::Subject => ComposeField::Body,
                                ComposeField::Body => ComposeField::To,
                            };
                        }
                        (KeyCode::BackTab, _) => {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Body,
                                ComposeField::Subject => ComposeField::To,
                                ComposeField::Body => ComposeField::Subject,
                            };
                        }
                        (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                            (self.on_send)(
                                &self.compose_to,
                                &self.compose_subject,
                                &self.compose_body,
                            )?;
                            self.mode = Mode::Inbox;
                        }
                        (KeyCode::Char(c), _) => match self.compose_field {
                            ComposeField::To => self.compose_to.push(c),
                            ComposeField::Subject => self.compose_subject.push(c),
                            ComposeField::Body => self.compose_body.push(c),
                        },
                        (KeyCode::Backspace, _) => match self.compose_field {
                            ComposeField::To => {
                                self.compose_to.pop();
                            }
                            ComposeField::Subject => {
                                self.compose_subject.pop();
                            }
                            ComposeField::Body => {
                                self.compose_body.pop();
                            }
                        },
                        (KeyCode::Enter, _) => {
                            if let ComposeField::Body = self.compose_field {
                                self.compose_body.push('\n');
                            } else {
                                self.compose_field = match self.compose_field {
                                    ComposeField::To => ComposeField::Subject,
                                    ComposeField::Subject => ComposeField::Body,
                                    ComposeField::Body => ComposeField::To,
                                };
                            }
                        }
                        (KeyCode::Down, _) => {
                            if let ComposeField::Body = self.compose_field {
                                self.compose_scroll = self.compose_scroll.saturating_add(1);
                            }
                        }
                        (KeyCode::Up, _) => {
                            if let ComposeField::Body = self.compose_field {
                                self.compose_scroll = self.compose_scroll.saturating_sub(1);
                            }
                        }
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

