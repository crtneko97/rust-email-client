use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use std::{error::Error, net::TcpStream};

use mailparse::{parse_mail, ParsedMail};

use html2text::from_read;

pub struct ImapClient 
{
    session: Session<TlsStream<TcpStream>>,
}

impl ImapClient 
{
    pub fn connect(user: &str, pass: &str) -> Result<Self, Box<dyn Error>> 
    {
        let domain = "imap.gmail.com";
        let tls = TlsConnector::builder().build()?;
        let client = imap::connect((domain, 993), domain, &tls)?;
        let session = client.login(user, pass).map_err(|e| e.0)?;
        Ok(Self { session })
    }

    pub fn fetch_inbox(&mut self, count: usize) -> Result<Vec<(u32, String)>, Box<dyn Error>> 
    {
        self.session.select("INBOX")?;

        let mut uids: Vec<u32> = self.session.search("ALL")?.into_iter().collect();
        uids.sort_unstable();
        let newest = uids.into_iter().rev().take(count);

        let mut list = Vec::new();
        for uid in newest 
        {
            let resp = self.session.uid_fetch(uid.to_string(), "RFC822.HEADER")?;
            for fetch in resp.iter() 
            {
                if let Some(header_bytes) = fetch.header() 
                {
                    let parsed = parse_mail(header_bytes)?.subparts; 
                    let mut from = String::new();
                    let mut subject = String::new();
                    for part in &parsed {
                    }
                    let raw = String::from_utf8_lossy(header_bytes).into_owned();
                    list.push((uid, raw.replace("\r\n", " ")));
                }
            }
        }
        Ok(list)
    }

    pub fn fetch_body(&mut self, uid: u32) -> Result<String, Box<dyn Error>> 
    {
        let resp = self.session.uid_fetch(uid.to_string(), "RFC822")?;
        let raw = match resp.iter().next().and_then(|f| f.body()) 
        {
            Some(b) => b,
            None => return Ok(String::new()),
        };

        let mail = parse_mail(raw)?;

        if let Some(txt) = find_plain(&mail)? 
        {
            return Ok(txt);
        }

        if let Some(html) = find_html(&mail)? 
        {
            let converted = from_read(html.as_bytes(), 80);
            return Ok(converted);
        }

        Ok(String::from_utf8_lossy(raw).into_owned())
    }
}

fn find_plain(mail: &ParsedMail) -> Result<Option<String>, Box<dyn Error>> 
{
    if mail.ctype.mimetype.eq_ignore_ascii_case("text/plain") 
    {
        return Ok(Some(mail.get_body()?));
    }
    for sub in &mail.subparts 
    {
        if let Some(t) = find_plain(sub)? 
        {
            return Ok(Some(t));
        }
    }
    Ok(None)
}

fn find_html(mail: &ParsedMail) -> Result<Option<String>, Box<dyn Error>> 
{
    if mail.ctype.mimetype.eq_ignore_ascii_case("text/html") 
    {
        return Ok(Some(mail.get_body()?));
    }
    for sub in &mail.subparts 
    {
        if let Some(h) = find_html(sub)? 
        {
            return Ok(Some(h));
        }
    }
    Ok(None)
}

