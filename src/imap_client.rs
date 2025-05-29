use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use std::{error::Error, net::TcpStream};
use mailparse::parse_headers;

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
        uids.sort_unstable(); // ascending
        let newest = uids.into_iter().rev().take(count);

        let mut list = Vec::new();
        for uid in newest 
        {
            let resp = self.session.uid_fetch(uid.to_string(), "RFC822.HEADER")?;
            for fetch in resp.iter() 
            {
                if let Some(header) = fetch.header() 
                {
                    let (hdrs, _) = parse_headers(header)?;
                    let subject = hdrs.iter()
                        .find(|h| h.get_key().eq_ignore_ascii_case("Subject"))
                        .map(|h| h.get_value().to_string())
                        .unwrap_or_default();
                    let from = hdrs.iter()
                        .find(|h| h.get_key().eq_ignore_ascii_case("From"))
                        .map(|h| h.get_value().to_string())
                        .unwrap_or_default();
                    list.push((uid, format!("{} — {}", from, subject)));
                }
            }
        }
        Ok(list)
    }

    pub fn fetch_body(&mut self, uid: u32) -> Result<String, Box<dyn Error>> 
    {
        let resp = self.session.uid_fetch(uid.to_string(), "RFC822")?;
        if let Some(fetch) = resp.iter().next() 
        {
            if let Some(body) = fetch.body() 
            {
                return Ok(String::from_utf8_lossy(body).into_owned());
            }
        }
        Ok(String::new())
    }
}

