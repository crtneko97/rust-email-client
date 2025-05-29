use dotenvy::dotenv;
use std::env;

pub struct Config 
{
    pub imap_user: String,
    pub imap_pass: String,
    pub smtp_user: String,
    pub smtp_pass: String,
}

impl Config 
{
    pub fn from_env() -> Self 
    {
        dotenv().ok();
        Self 
        {
            imap_user: env::var("IMAP_USER").expect("IMAP_USER must be set"),
            imap_pass: env::var("IMAP_PASS").expect("IMAP_PASS must be set"),
            smtp_user: env::var("SMTP_USER").expect("SMTP_USER must be set"),
            smtp_pass: env::var("SMTP_PASS").expect("SMTP_PASS must be set"),
        }
    }
}
