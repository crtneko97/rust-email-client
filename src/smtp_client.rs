use lettre::transport::smtp::{authentication::{Credentials, Mechanism}, SmtpTransport};
use lettre::{Message, Transport};
use std::error::Error;

pub struct SmtpClient 
{
    mailer: SmtpTransport,
}

impl SmtpClient 
{
    pub fn connect(user: &str, pass: &str) -> Result<Self, Box<dyn Error>> 
    {
        let creds = Credentials::new(user.into(), pass.into());
        let mailer = SmtpTransport::starttls_relay("smtp.gmail.com")?
            .credentials(creds)
            .authentication(vec![Mechanism::Plain])
            .build();
        Ok(Self { mailer })
    }

    pub fn send(&self, email: Message) -> Result<(), Box<dyn Error>> 
    {
        self.mailer.send(&email)?;
        Ok(())
    }
}
