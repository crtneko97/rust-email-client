// src/ui/mod.rs

// ─── EXTERNAL CRATES & IMPORTS ─────────────────────────────────────────────────

// We need KeyCode and KeyModifiers for handling key events.
// The previous `KeyEvent` import was unused.
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::{error::Error, io};

// Import TextArea<'static> from tui-textarea v0.7.0.
// It must be <'static> so that &TextArea<'static> implements Widget for ratatui 0.29.
use tui_textarea::TextArea;

/// ——————— APPLICATION MODES —————————————————————————————————————————————
///
/// We track which “screen” the user is on:
///   • Mode::Inbox        → show the list of message summaries.
///   • Mode::View         → display the full content (headers + body) of one message.
///   • Mode::Compose      → show “To / Subject / Body” input fields for sending mail.
///   • Mode::ConfirmDelete→ a temporary “Are you sure?” state after pressing ‘d’ once.
///
pub enum Mode {
    Inbox,
    View,
    Compose,
    ConfirmDelete,
}

/// ——————— COMPOSE SUB-FIELDS ——————————————————————————————————————————
///
/// In Compose mode, we have three sub-fields:
///   • ComposeField::To      → editing the “To:” line (single line).
///   • ComposeField::Subject → editing the “Subject:” line (single line).
///   • ComposeField::Body    → editing the multiline body (TextArea).
///
pub enum ComposeField {
    To,
    Subject,
    Body,
}

/// ——————— APP STATE & CALLBACKS —————————————————————————————————————————
///
/// This struct holds all state and four callbacks:
///
///   • on_send(&str, &str, &str) → send a message via SMTP
///   • on_view(u32)             → fetch and return message content via IMAP
///   • on_refresh(usize)        → re-fetch N message summaries via IMAP
///   • on_delete(u32)           → delete a single message via IMAP
///
/// It also holds:
///   • items: Vec<(u32, String)> – the inbox list (UID, “From … Date”)
///   • selected: usize           – which row is highlighted in the inbox list
///   • mode: Mode                – which screen is currently active
///   • view_buffer: String       – full text (headers + body) of the viewed message
///   • view_scroll: u16          – vertical scroll offset in View mode
///   • compose_to: String        – “To:” line text
///   • compose_subject: String   – “Subject:” line text
///   • compose_body: TextArea<'static> – multiline widget for the “Body:” text
///   • compose_field: ComposeField      – which compose sub-field is active
///   • inbox_count: usize        – how many messages to request from IMAP (e.g. 20, then +10)
///   • tooltip: String           – small status line at the bottom (“Sent!”, “Loading more…”)
///
pub struct App<F, G, H, J>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
    H: FnMut(usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> + 'static,
    J: FnMut(u32) -> Result<(), Box<dyn Error>> + 'static,
{
    // ─── INBOX DATA ─────────────────────────────────────────────────────────────
    items: Vec<(u32, String)>, // (UID, “From    Date”) pairs for the inbox list
    selected: usize,           // which row is currently highlighted
    mode: Mode,                // which screen we’re on

    // ─── VIEW MODE ──────────────────────────────────────────────────────────────
    view_buffer: String, // “View” mode: full message text (headers + body)
    view_scroll: u16,    // vertical scroll offset in View mode

    // ─── COMPOSE MODE ───────────────────────────────────────────────────────────
    compose_to: String,              // “To:” line
    compose_subject: String,         // “Subject:” line
    compose_body: TextArea<'static>, // multiline “Body:” editor widget
    compose_field: ComposeField,     // which of To/Subject/Body is focused

    // ─── CALLBACKS (SMTP / IMAP) ────────────────────────────────────────────────
    on_send: F,      // called when sending mail (Ctrl+S)
    on_view: G,      // called when viewing a message (v)
    on_refresh: H,   // called when loading more messages (m)
    on_delete: J,    // called when deleting a message (d)

    // ─── OTHER STATE ─────────────────────────────────────────────────────────────
    inbox_count: usize, // how many messages to fetch from IMAP
    tooltip: String,    // status line at the bottom (“Sent!”, “Loading…”)
}

impl<F, G, H, J> App<F, G, H, J>
where
    F: FnMut(&str, &str, &str) -> Result<(), Box<dyn Error>> + 'static,
    G: FnMut(u32) -> Result<String, Box<dyn Error>> + 'static,
    H: FnMut(usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> + 'static,
    J: FnMut(u32) -> Result<(), Box<dyn Error>> + 'static,
{
    /// Constructor: supply four callbacks plus initial inbox items, inbox_count, and tooltip.
    ///
    ///  • `items`: Vec<(UID, “From … Date”)> – initial inbox list
    ///  • `on_view`: FnMut(u32) -> Result<String> – fetch full message by UID
    ///  • `on_send`: FnMut(&str, &str, &str) -> Result<()> – send a new message
    ///  • `on_refresh`: FnMut(usize) -> Result<Vec<(UID, String)>> – load more summaries
    ///  • `on_delete`: FnMut(u32) -> Result<()> – delete a message by UID
    ///  • `inbox_count`: usize – how many messages to fetch initially
    ///  • `tooltip`: String – initial status line (usually empty)
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
            // ─── INBOX ───────────────────────────────────────────────────────────
            items,
            selected: 0,
            mode: Mode::Inbox,

            // ─── VIEW ────────────────────────────────────────────────────────────
            view_buffer: String::new(),
            view_scroll: 0,

            // ─── COMPOSE ─────────────────────────────────────────────────────────
            compose_to: String::new(),
            compose_subject: String::new(),
            // MUST be TextArea<'static> so that &TextArea<'static> implements Widget
            compose_body: TextArea::default(),
            compose_field: ComposeField::To,

            // ─── CALLBACKS ───────────────────────────────────────────────────────
            on_send,
            on_view,
            on_refresh,
            on_delete,

            // ─── OTHER STATE ─────────────────────────────────────────────────────
            inbox_count,
            tooltip,
        }
    }

    /// The main event loop. Enable raw mode, switch to alternate screen,
    /// then loop until the user presses ‘q’. On each iteration:
    ///   1) draw the UI (Inbox / View / Compose / ConfirmDelete + tooltip)
    ///   2) read a KeyEvent
    ///   3) update state based on the current mode + key
    ///   4) repeat
    pub fn run(mut self) -> Result<(), Box<dyn Error>> {
        // Enable raw mode (keypresses go straight to us)
        enable_raw_mode()?;

        // Switch to the alternate screen so our TUI doesn’t overwrite the shell
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

        loop {
            // ─────────────────────────────────────────────────────────────────
            // 1) DRAW THE UI
            // Split vertically: 90% for main, 10% for tooltip/status
            term.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(90), Constraint::Percentage(10)])
                    .split(f.size());

                // The top 90% (chunks[0]) is split horizontally:
                //   • Left  30% → Inbox list
                //   • Right 70% → View / Compose / ConfirmDelete
                let main_area = chunks[0];
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(main_area);

                // ─────────────────────────────────────────────────────────────
                // 2a) LEFT COLUMN: render the Inbox list
                // ─────────────────────────────────────────────────────────────
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

                // ─────────────────────────────────────────────────────────────
                // 2b) RIGHT COLUMN: depends on `self.mode`
                // ─────────────────────────────────────────────────────────────
                match self.mode {
                    Mode::Inbox => {
                        // Show a placeholder when no message is open
                        let placeholder = Paragraph::new(
                            "Press 'v' to view, 'r' to reply, 'c' to compose,\n\
                             'm' to load more, 'd' to delete, 'q' to quit",
                        )
                        .block(Block::default().borders(Borders::ALL).title("Instructions"))
                        .wrap(Wrap { trim: true });
                        f.render_widget(placeholder, columns[1]);
                    }

                    Mode::View | Mode::ConfirmDelete => {
                        // Draw the message content or the “Confirm Delete” box
                        let title = match self.mode {
                            Mode::View => "Message",
                            Mode::ConfirmDelete => "Confirm Delete (Press d again)",
                            _ => unreachable!(),
                        };
                        let mut block = Block::default().borders(Borders::ALL).title(title);
                        if let Mode::ConfirmDelete = self.mode {
                            block = block.style(Style::default().add_modifier(Modifier::REVERSED));
                        }
                        // IMPORTANT: use `.as_str()` so Paragraph::new knows it’s &str
                        let p = Paragraph::new(self.view_buffer.as_str())
                            .block(block)
                            .wrap(Wrap { trim: true })
                            .scroll((self.view_scroll, 0));
                        f.render_widget(p, columns[1]);
                    }

                    Mode::Compose => {
                        // COMPOSE MODE: split into three vertical areas: To, Subject, Body
                        let compose_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Min(0),
                            ])
                            .split(columns[1]);

                        // 2c) “To:” field (Paragraph)
                        // Use .as_str() so the compiler infers &str
                        let to_title = if matches!(self.compose_field, ComposeField::To) {
                            "To*"
                        } else {
                            "To"
                        };
                        let p_to = Paragraph::new(self.compose_to.as_str())
                            .block(Block::default().borders(Borders::ALL).title(to_title));
                        f.render_widget(p_to, compose_chunks[0]);

                        // 2d) “Subject:” field (Paragraph)
                        let sub_title = if matches!(self.compose_field, ComposeField::Subject) {
                            "Subject*"
                        } else {
                            "Subject"
                        };
                        let p_sub = Paragraph::new(self.compose_subject.as_str())
                            .block(Block::default().borders(Borders::ALL).title(sub_title));
                        f.render_widget(p_sub, compose_chunks[1]);

                        // 2e) “Body:” field
                        // (a) Draw a Block with a border and title “Body”
                        let body_block = Block::default().borders(Borders::ALL).title("Body");
                        f.render_widget(body_block, compose_chunks[2]);

                        // (b) Compute the “inner” Rect inset by 1 cell so TextArea draws inside
                        let inner = Rect {
                            x:      compose_chunks[2].x + 1,
                            y:      compose_chunks[2].y + 1,
                            width:  compose_chunks[2].width.saturating_sub(2),
                            height: compose_chunks[2].height.saturating_sub(2),
                        };

                        // (c) Render the TextArea<'static> inside that inner area.
                        f.render_widget(&self.compose_body, inner);

                        // (d) If Body is focused, place the blinking cursor at TextArea’s (row, col).
                        if matches!(self.compose_field, ComposeField::Body) {
                            let (row, col) = self.compose_body.cursor();
                            // Note: row/col are 0-based inside the TextArea.
                            f.set_cursor(inner.x + col as u16, inner.y + row as u16);
                        }
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // 3) BOTTOM ROW: render the tooltip/status box
                // ─────────────────────────────────────────────────────────────
                let tip = Paragraph::new(self.tooltip.as_str())
                    .block(Block::default().borders(Borders::ALL).title("Status"));
                f.render_widget(tip, chunks[1]);
            })?;
            // End of term.draw

            // ─────────────────────────────────────────────────────────────────
            // 4) HANDLE KEY EVENTS (Inbox / View / Compose / ConfirmDelete)
            // ─────────────────────────────────────────────────────────────────
            if let Event::Key(key_event) = event::read()? {
                match self.mode {
                    // ─────────────────────────────────────────────────────────
                    // MODE: INBOX
                    // ─────────────────────────────────────────────────────────
                    Mode::Inbox => match key_event.code {
                        KeyCode::Char('q') => break, // Quit the application

                        KeyCode::Char('v') => {
                            // Open the selected message in View mode
                            let uid = self.items[self.selected].0;
                            self.view_buffer = (self.on_view)(uid)?;
                            self.view_scroll = 0;
                            self.mode = Mode::View;
                            self.tooltip.clear();
                        }

                        KeyCode::Char('r') => {
                            // Reply to selected message:
                            //  1) Fetch full text (headers + body)
                            let uid = self.items[self.selected].0;
                            let full_text = (self.on_view)(uid)?;
                            let mut lines = full_text.lines();
                            let from_line = lines
                                .next()
                                .and_then(|l| l.strip_prefix("From: "))
                                .unwrap_or("")
                                .to_string();
                            let subject_line = lines
                                .next()
                                .and_then(|l| l.strip_prefix("Subject: "))
                                .unwrap_or("")
                                .to_string();

                            //  2) Prefill To: and Subject:
                            self.compose_to = from_line.clone();
                            self.compose_subject = if subject_line
                                .to_lowercase()
                                .starts_with("re:")
                            {
                                subject_line.clone()
                            } else {
                                format!("Re: {}", subject_line)
                            };

                            //  3) Clear the Body TextArea
                            self.compose_body = TextArea::default();

                            //  4) Switch to Compose mode, focusing on Body
                            self.compose_field = ComposeField::Body;
                            self.mode = Mode::Compose;
                            self.tooltip.clear();
                        }

                        KeyCode::Char('c') => {
                            // Compose a new blank message
                            self.compose_to.clear();
                            self.compose_subject.clear();
                            self.compose_body = TextArea::default();
                            self.compose_field = ComposeField::To;
                            self.mode = Mode::Compose;
                            self.tooltip.clear();
                        }

                        KeyCode::Char('m') => {
                            // Load more messages from IMAP
                            self.tooltip = "Loading more…".into();
                            self.inbox_count += 10;
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            self.selected = 0;
                            self.tooltip =
                                format!("Successfully loaded {} messages", self.inbox_count);
                        }

                        KeyCode::Char('d') => {
                            // Enter ConfirmDelete (first ‘d’)
                            self.mode = Mode::ConfirmDelete;
                            self.tooltip =
                                "Are you sure you want to delete? Press 'd' again to confirm."
                                    .into();
                        }

                        KeyCode::Down => {
                            // Move highlight down in Inbox
                            if !self.items.is_empty() {
                                self.selected = (self.selected + 1) % self.items.len();
                            }
                            self.tooltip.clear();
                        }

                        KeyCode::Up => {
                            // Move highlight up in Inbox
                            if self.selected > 0 {
                                self.selected -= 1;
                            }
                            self.tooltip.clear();
                        }

                        _ => {}
                    },

                    // ─────────────────────────────────────────────────────────
                    // MODE: CONFIRM DELETE
                    // ─────────────────────────────────────────────────────────
                    Mode::ConfirmDelete => match key_event.code {
                        KeyCode::Char('d') => {
                            // Second ‘d’ actually deletes
                            let uid = self.items[self.selected].0;
                            (self.on_delete)(uid)?;
                            // Re-fetch inbox so the deleted message disappears
                            self.items = (self.on_refresh)(self.inbox_count)?;
                            self.selected = 0;
                            self.mode = Mode::Inbox;
                            self.tooltip = "Deleted!".into();
                        }
                        KeyCode::Esc => {
                            // Cancel deletion
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                        _ => {
                            // Any other key cancels deletion
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                    },

                    // ─────────────────────────────────────────────────────────
                    // MODE: VIEW
                    // ─────────────────────────────────────────────────────────
                    Mode::View => match key_event.code {
                        KeyCode::Esc => {
                            // Return to Inbox
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                        }
                        KeyCode::Down => {
                            // Scroll down in the message
                            self.view_scroll = self.view_scroll.saturating_add(1);
                        }
                        KeyCode::Up => {
                            // Scroll up in the message
                            self.view_scroll = self.view_scroll.saturating_sub(1);
                        }
                        _ => {}
                    },

                    // ─────────────────────────────────────────────────────────
                    // MODE: COMPOSE
                    // ─────────────────────────────────────────────────────────
                    Mode::Compose => {
                        // 1) ESC = cancel compose → back to Inbox
                        if key_event.code == KeyCode::Esc {
                            self.mode = Mode::Inbox;
                            self.tooltip.clear();
                            continue;
                        }

                        // 2) Tab / Shift+Tab = cycle focus among To, Subject, Body
                        if key_event.code == KeyCode::Tab {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Subject,
                                ComposeField::Subject => ComposeField::Body,
                                ComposeField::Body => ComposeField::To,
                            };
                            self.tooltip.clear();
                            continue;
                        }
                        if key_event.code == KeyCode::BackTab {
                            self.compose_field = match self.compose_field {
                                ComposeField::To => ComposeField::Body,
                                ComposeField::Subject => ComposeField::To,
                                ComposeField::Body => ComposeField::Subject,
                            };
                            self.tooltip.clear();
                            continue;
                        }

                        // 3) Ctrl+S = send message
                        if key_event.code == KeyCode::Char('s')
                            && key_event.modifiers == KeyModifiers::CONTROL
                        {
                            let body_text = self.compose_body.lines().join("\n");
                            (self.on_send)(
                                &self.compose_to,
                                &self.compose_subject,
                                &body_text,
                            )?;
                            self.mode = Mode::Inbox;
                            self.tooltip = "Sent!".into();
                            continue;
                        }

                        // 4) If focus is To or Subject, handle them manually:
                        match self.compose_field {
                            ComposeField::To => {
                                match key_event.code {
                                    KeyCode::Char(c) => {
                                        self.compose_to.push(c);
                                        self.tooltip.clear();
                                    }
                                    KeyCode::Backspace => {
                                        self.compose_to.pop();
                                    }
                                    KeyCode::Enter => {
                                        // Move focus from To → Subject
                                        self.compose_field = ComposeField::Subject;
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            ComposeField::Subject => {
                                match key_event.code {
                                    KeyCode::Char(c) => {
                                        self.compose_subject.push(c);
                                        self.tooltip.clear();
                                    }
                                    KeyCode::Backspace => {
                                        self.compose_subject.pop();
                                    }
                                    KeyCode::Enter => {
                                        // Move focus from Subject → Body
                                        self.compose_field = ComposeField::Body;
                                    }
                                    _ => {}
                                }
                                continue;
                            }

                            // 5) If focus is Body, pass the raw KeyEvent to TextArea:
                            ComposeField::Body => {
                                // TextArea handles arrow keys, backspace, newline, wrapping, scrolling
                                self.compose_body.input(key_event.clone());
                                self.tooltip.clear();
                                continue;
                            }
                        }
                    }
                }
            }
            // ─────────────────────────────────────────────────────────────────
        }

        // Before exiting, restore normal terminal mode and screen
        disable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen)?;
        Ok(())
    }
}

