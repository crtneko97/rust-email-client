pub mod client;
pub mod parser;
pub mod models;

pub use client::ImapClient;
pub use parser::{find_html, find_plain};
pub use models::{MailDetail, MailSummary};
