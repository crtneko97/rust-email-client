mod config;
mod imap;
mod smtp;
mod ui;

use config::Config;
use imap::ImapClient;
use smtp::SmtpClient;
use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;
use ui::App;

fn main() -> Result<(), Box<dyn Error>> 
{
    let cfg = Config::from_env();

    let imap = Rc::new(RefCell::new(ImapClient::connect(
        &cfg.imap_user,
        &cfg.imap_pass,
    )?));

    let smtp = Rc::new(RefCell::new(SmtpClient::connect(
        &cfg.smtp_user,
        &cfg.smtp_pass,
    )?));

    let mut inbox_count: usize = 20;
    let initial_items = 
    {
        let mut imap_ref = imap.borrow_mut();
       let summaries = imap_ref.fetch_inbox(inbox_count)?;
        summaries
            .into_iter()
            .map(|ms| 
            {
                let date_str = ms.date.format("%a, %e %b %Y %T %z").to_string();
                (ms.uid, format!("{}    {}", ms.from, date_str))
            })
            .collect::<Vec<(u32, String)>>()
    };

    let imap_for_view = Rc::clone(&imap);
    let on_view = move |uid: u32| -> Result<String, Box<dyn Error>> 
    {
        let mut imap_ref = imap_for_view.borrow_mut();
        let (from_val, subject_val, date_val) = imap_ref.fetch_headers(uid)?;
        let body_text = imap_ref.fetch_body(uid)?;
        let combined = format!(
            "From: {}\nSubject: {}\nDate: {}\n\n{}",
            from_val, subject_val, date_val, body_text
        );
        Ok(combined)
    };

    let imap_for_refresh = Rc::clone(&imap);
    let on_refresh = move |new_count: usize| 
    {
        let mut imap_ref = imap_for_refresh.borrow_mut();
        let summaries = imap_ref.fetch_inbox(new_count)?;
        Ok(summaries
            .into_iter()
            .map(|ms| {
                let date_str = ms.date.format("%a, %e %b %Y %T %z").to_string();
                (ms.uid, format!("{}    {}", ms.from, date_str))
            })
            .collect::<Vec<(u32, String)>>())
    };

    let imap_for_delete = Rc::clone(&imap);
    let on_delete = move |uid: u32| 
    {
        let mut imap_ref = imap_for_delete.borrow_mut();
        imap_ref.delete_message(uid)
    };

    let smtp_for_send = Rc::clone(&smtp);
    let on_send = move |to: &str, subject: &str, body: &str| 
    {
        let email = lettre::Message::builder()
            .from(cfg.smtp_user.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .body(body.to_string())?;
        let smtp_ref = smtp_for_send.borrow();
        smtp_ref.send(email)?;
        Ok(())
    };

    let mut app = App::new
    (
        initial_items,
        on_view,
        on_send,
        on_refresh,
        on_delete,
        inbox_count,
        String::new(),
    );
    app.run()
}

