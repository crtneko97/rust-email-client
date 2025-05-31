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
    /// A transient “Confirm Deletion” state after pressing `d` once
    ConfirmDelete,
}

pub enum ComposeField {
    To,
    Subject,
    Body,
}

// ──────────────────────────────────────────────────────────────────────────────
// App now has four generic parameters: 
//   F = on_send, G = on_view, H = on_refresh, J = on_delete.
// We also add two new fields: `on_delete: J`, `inbox_count: usize`, and 
// `tooltip: String` for the bottom row.
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
    tooltip: String,    // bottom row text
}

impl<F, G, H, J> App<F, G, H, J>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
    H: FnMut(usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> + 'static,
    J: FnMut(u32) -> Result<(), Box<dyn Error>> + 'static,
{
    /// Now takes seven arguments:
    /// 1) items        : Vec<(UID, "from    date")>
    /// 2) on_view      : FnMut(u32) -> Result<String, Error>
    /// 3) on_send      : FnMut(&str, &str, &str) -> Result<(), Error>
    /// 4) on_refresh   : FnMut(usize) -> Result<Vec<(UID,String)>, Error>
    /// 5) on_delete    : FnMut(u32) -> Result<(), Error>
    /// 6) inbox_count  : usize (initial number of messages)
    /// 7) tooltip      : String (initially empty)
    pub fn new(
        items: Vec<(u32, String)>,
        on_view: G,
        on_send: F,
        on_refresh: H,
        on_delete: J,
        inbox_count: usize,
        tooltip: String,
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
            tooltip,
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
                // 1) SPLIT THE ENTIRE FRAME INTO TWO ROWS (Main / Tooltip  = 90% | 10%)
                // ────────────────────────────────────────────────────────────────
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(90), Constraint::Percentage(10)])
                    .split(f.size());

                // ────────────────────────────────────────────────────────────────
                // 2) MAIN ROW (chunks[0]): split horizontally into Inbox | Detail
                // ────────────────────────────────────────────────────────────────
                let main_area = chunks[0];
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    // Left 30% for Inbox, Right 70% for View/Compose
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(main_area);

                // ────────────────────────────────────────────────────────────────
                // 2a) LEFT COLUMN (columns[0]): always render the Inbox list
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
                f.render_stateful_widget(list, columns[0], &mut state);

                // ────────────────────────────────────────────────────────────────
                // 2b) RIGHT COLUMN (columns[1]): render View / Compose / Placeholder
                // ────────────────────────────────────────────────────────────────
                match self.mode {
                    Mode::Inbox => {
                        // In “Inbox” mode (no email opened), show a simple placeholder
                        let placeholder = Paragraph::new(
                            "Press 'v' to view, 'c' to compose, 'm' to load more, 'd' to delete, 'q' to quit",
                        )
                        .block(Block::default().borders(Borders::ALL).title("Instructions"))
                        .wrap(Wrap { trim: true });
                        f.render_widget(placeholder, columns[1]);
                    }
                    Mode::View | Mode::ConfirmDelete => {
                        // In “View” or “ConfirmDelete” mode, show the message body scrollable
                        let title = match self.mode {
                            Mode::View => "Message",
                            Mode::ConfirmDelete => "Confirm Delete (Press d again)",
                            _ => unreachable!(),
                        };
                        let mut block = Block::default().borders(Borders::ALL).title(title);
                        // If ConfirmDelete, highlight the border in reversed style:
                        if let Mode::ConfirmDelete = self.mode {
                            block = block.style(Style::default().add_modifier(Modifier::REVERSED));
                        }
                        let p = Paragraph::new(self.view_buffer.as_ref())
                            .block(block)
                            .wrap(Wrap { trim: true })
                            .scroll((self.view_scroll, 0));
                        f.render_widget(p, columns[1]);
                    }
                    Mode::Compose => {
                        // In Compose mode, split the right column vertically into To / Subject / Body
                        let compose_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Min(0),
                            ])
                            .split(columns[1]);

                        // “To” field
                        let to_title = if matches!(self.compose_field, ComposeField::To) {
                            "To*"
                        } else {
                            "To"
                        };
                        let p_to = Paragraph::new(self.compose_to.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(to_title));
                        f.render_widget(p_to, compose_chunks[0]);

                        // “Subject” field
                        let sub_title = if matches!(self.compose_field, ComposeField::Subject) {
                            "Subject*"
                        } else {
                            "Subject"
                        };
                        let p_sub = Paragraph::new(self.compose_subject.as_ref())
                            .block(Block::default().borders(Borders::ALL).title(sub_title));
                        f.render_widget(p_sub, compose_chunks[1]);

                        // “Body” field
                        let p_body = Paragraph::new(self.compose_body.as_ref())
                            .block(Block::default().borders(Borders::ALL).title("Body"))
                            .wrap(Wrap { trim: true })
                            .scroll((self.compose_scroll, 0));
                        f.render_widget(p_body, compose_chunks[2]);
                    }
                }

                // ────────────────────────────────────────────────────────────────
                // 3) TOOLTIP ROW (chunks[1]): display `self.tooltip` in a bordered box
                // ────────────────────────────────────────────────────────────────
                let tip = Paragraph::new(self.tooltip.as_ref())
                    .block(Block::default().borders(Borders::ALL).title("Status"));
                f.render_widget(tip, chunks[1]);
            })?;

            // ────────────────────────────────────────────────────────────────────
            // 4) HANDLE INPUT (Inbox / View / Compose / ConfirmDelete)
            // ────────────────────────────────────────────────────────────────────
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                match self.mode {
                    Mode::Inbox => match code {
                        KeyCode::Char('q') => break,

                        KeyCode::Char('v') => {
                            // Open the selected message in “View” mode
                            let uid = self.items[self.selected].0;
                            self.view_buffer = (self.on_view)(uid)?;
                            self.view_scroll = 0;
                            self.mode = Mode::View;
                            self.tooltip.clear();
                        }

                        KeyCode::Char('c') => {
                            // Switch to Compose mode
                            self.compose_to.clear();
                            self.compose_subject.clear();
                            self.compose_body.clear();
                            self.compose_field = ComposeField::To;
                            self.compose_scroll = 0;
                            self.mode = Mode::Compose;
                            self.tooltip.clear();
                        }

                        KeyCode::Char('m') => {
                            // Load more emails: display a tooltip, fetch, then clear tooltip
                            self.tooltip = "Loading more…".into();
                            self.inbox_count += 10;
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            self.selected = 0;
                            // After re-fetch, update tooltip:
                            self.tooltip = format!("Successfully loaded {} messages", self.inbox_count);
                        }

                        KeyCode::Char('d') => {
                            // First press of `d` enters “ConfirmDelete” mode (tooltip prompt)
                            if let Mode::Inbox = self.mode {
                                self.mode = Mode::ConfirmDelete;
                                self.tooltip = "Are you sure you want to delete? Press 'd' again to confirm.".into();
                            }
                        }

                        KeyCode::Down => {
                            if !self.items.is_empty() {
                                self.selected = (self.selected + 1) % self.items.len();
                            }
                            self.tooltip.clear();
                        }

                        KeyCode::Up => {
                            if self.selected > 0 {
                                self.selected -= 1;
                            }
                            self.tooltip.clear();
                        }

                        _ => {}
                    },

                    Mode::ConfirmDelete => match code {
                        KeyCode::Char('d') => {
                            // Second press of `d` actually deletes
                            let uid = self.items[self.selected].0;
                            (self.on_delete)(uid)?;
                            // Re-fetch so the deleted message vanishes
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            self.selected = 0;
                            self.mode = Mode::Inbox;
                            self.tooltip = "Deleted!".into();
                        }
                        KeyCode::Esc => {
                            // Cancel delete
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                        _ => {
                            // Any other key just cancels confirm
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                    },

                    Mode::View => match code {
                        KeyCode::Esc => {
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                        KeyCode::Down => {
                            self.view_scroll = self.view_scroll.saturating_add(1);
                        }
                        KeyCode::Up => {
                            self.view_scroll = self.view_scroll.saturating_sub(1);
                        }
                        _ => {}
                    },

                    Mode::Compose => match (code, modifiers) {
                        (KeyCode::Esc, _) => {
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                        (KeyCode::Tab, _) => {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Subject,
                                ComposeField::Subject => ComposeField::Body,
                                ComposeField::Body => ComposeField::To,
                            };
                            self.tooltip.clear();
                        }
                        (KeyCode::BackTab, _) => {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Body,
                                ComposeField::Subject => ComposeField::To,
                                ComposeField::Body => ComposeField::Subject,
                            };
                            self.tooltip.clear();
                        }
                        (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                            (self.on_send)(&self.compose_to, &self.compose_subject, &self.compose_body)?;
                            self.mode = Mode::Inbox;
                            self.tooltip = "Sent!".into();
                        }
                        (KeyCode::Char(c), _) => {
                            match self.compose_field {
                                ComposeField::To => self.compose_to.push(c),
                                ComposeField::Subject => self.compose_subject.push(c),
                                ComposeField::Body => self.compose_body.push(c),
                            }
                            self.tooltip.clear();
                        }
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

