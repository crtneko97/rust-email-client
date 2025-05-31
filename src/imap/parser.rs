use mailparse::ParsedMail;
use std::error::Error;


pub fn find_plain(mail: &ParsedMail) -> Result<Option<String>, Box<dyn Error>> 
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
pub fn find_html(mail: &ParsedMail) -> Result<Option<String>, Box<dyn Error>> 
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

