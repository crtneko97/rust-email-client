use crate::imap::models::{MailDetail, MailSummary};
use crate::imap::parser::{find_html, find_plain};

use chrono::{DateTime, FixedOffset};
use html2text::from_read;
use mailparse::parse_mail;
use native_tls::{TlsConnector, TlsStream};
use std::error::Error;
use std::net::TcpStream;

use imap::Session;

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

    pub fn fetch_inbox(&mut self, count: usize) -> Result<Vec<MailSummary>, Box<dyn Error>> 
    {
        self.session.select("INBOX")?;

        let all_fetches = self.session.fetch("1:*", "(UID INTERNALDATE)")?;

        let mut uid_dates: Vec<(u32, DateTime<FixedOffset>)> = Vec::with_capacity(all_fetches.len());
        for fetch in all_fetches.iter() 
        {
            if let (Some(uid), Some(internal_date)) = (fetch.uid, fetch.internal_date()) 
            {
                uid_dates.push((uid, internal_date));
            }
        }

        uid_dates.sort_unstable_by(|a, b| b.1.cmp(&a.1));

        let newest_uids = uid_dates
            .into_iter()
            .take(count)
            .map(|(uid, date)| (uid, date))
            .collect::<Vec<(u32, DateTime<FixedOffset>)>>();

        let mut list = Vec::with_capacity(newest_uids.len());
        for (uid, date) in newest_uids.into_iter() 
        {
            let resp = self.session.uid_fetch(
                uid.to_string(),
                "BODY.PEEK[HEADER.FIELDS (FROM DATE)]",
            )?;
            for fetch in resp.iter() 
            {
                if let Some(header_bytes) = fetch.header() 
                {
                    let header_text = String::from_utf8_lossy(header_bytes);
                    let mut from_line = String::new();

                    for raw_line in header_text.split("\r\n") 
                    {
                        let lower = raw_line.to_lowercase();
                        if lower.starts_with("from:") 
                        {
                            let after = raw_line["From:".len()..].trim();
                            if let Some(start) = after.find('<') {
                                if let Some(end) = after[start + 1..].find('>') 
                                {
                                    from_line = after[start + 1..start + 1 + end].to_string();
                                } 
                                else 
                                {
                                    from_line = after[start + 1..].to_string();
                                }
                            } 
                            else 
                            {
                                from_line = after.to_string();
                            }
                        }
                    }

                    list.push(MailSummary 
                    {
                        uid,
                        from: from_line,
                        date,
                    });
                }
            }
        }

        Ok(list)
    }

    pub fn fetch_headers(&mut self, uid: u32) -> Result<(String, String, String), Box<dyn Error>> 
    {
        self.session.select("INBOX")?;

        let resp = self
            .session
            .uid_fetch(uid.to_string(), "BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)]")?;
        for fetch in resp.iter() 
        {
            if let Some(header_bytes) = fetch.header() 
            {
                let header_text = String::from_utf8_lossy(header_bytes);
                let mut from_val = String::new();
                let mut subject_val = String::new();
                let mut date_val = String::new();

                for raw_line in header_text.split("\r\n") 
                {
                    let lower = raw_line.to_lowercase();
                    if lower.starts_with("from:") 
                    {
                        from_val = raw_line["From:".len()..].trim().to_string();
                    } 
                    else if lower.starts_with("subject:") 
                    {
                        subject_val = raw_line["Subject:".len()..].trim().to_string();
                    } 
                    else if lower.starts_with("date:") 
                    {
                        date_val = raw_line["Date:".len()..].trim().to_string();
                    }
                }

                return Ok((from_val, subject_val, date_val));
            }
        }

        Ok((String::new(), String::new(), String::new()))
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

    pub fn delete_message(&mut self, uid: u32) -> Result<(), Box<dyn Error>> 
    {
        self.session.select("INBOX")?;
        self.session.uid_store(uid.to_string(), "+FLAGS (\\Deleted)")?;
        self.session.expunge()?;
        Ok(())
    }
}

