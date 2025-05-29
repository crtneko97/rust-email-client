mod config;
mod imap_client;
mod smtp_client;
mod ui;

use config::Config;
use imap_client::ImapClient;
use smtp_client::SmtpClient;
use ui::App;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> 
{
    let cfg = Config::from_env();

    let mut imap = ImapClient::connect(&cfg.imap_user, &cfg.imap_pass)?;
    let items = imap.fetch_inbox(20)?;
    let mut smtp = SmtpClient::connect(&cfg.smtp_user, &cfg.smtp_pass)?;

    let mut on_view = move |uid: u32| imap.fetch_body(uid);

    let mut on_send = move |_to: &str, _subject: &str, body: &str| 
    {
        // here we just echo back to ourselves:
        let email = lettre::Message::builder()
            .from(cfg.smtp_user.parse()?)
            .to(cfg.smtp_user.parse()?)
            .subject("Sent via bps-mail")
            .body(body.to_string())?;
        smtp.send(email)?;
        Ok(())
    };

    let app = App::new(items, on_view, on_send);
    app.run()
}

