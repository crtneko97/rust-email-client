use chrono::{DateTime, FixedOffset};

#[derive(Debug, Clone)]
pub struct MailSummary 
{
    pub uid: u32,
    pub from: String,
    pub date: DateTime<FixedOffset>,
}

#[derive(Debug, Clone)]
pub struct MailDetail 
{
    pub from: String,
    pub subject: String,
    pub date: String,
    pub body: String,
}
