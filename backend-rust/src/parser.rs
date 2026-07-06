use anyhow::{anyhow, Result};
use chrono::NaiveDateTime;
use regex::Regex;

use crate::models::ParsedMessage;

pub struct MessageParser {
    custom_format: Option<String>,
    line_re: Regex,
    attachment_re: Regex,
}

impl MessageParser {
    pub fn new(custom_format: Option<String>) -> Result<Self> {
        Ok(Self {
            custom_format,
            line_re: Regex::new(
                r#"^\u{200e}?\[?(\d{1,4}[-/.]\d{1,4}[-/.]\d{1,4}[,.]? \d{1,2}:\d{2}(?::\d{2})?\s?(?:[aApP][mM])?)\]?(?: - |: |\s+)(?:(.*?): )?(.*)$"#,
            )?,
            attachment_re: Regex::new(r#"^(.*?)\s+\((.*?)\)$"#)?,
        })
    }

    pub fn parse_messages(&self, text: &str) -> Vec<ParsedMessage> {
        let mut messages = Vec::new();
        let mut current: Option<(String, Option<String>, String)> = None;

        for raw_line in text.lines() {
            let line = raw_line.trim_end_matches('\0');
            if let Some(captures) = self.line_re.captures(line) {
                if let Some((date, author, body)) = current.take() {
                    if let Some(message) = self.build_message(&date, author, body) {
                        messages.push(message);
                    }
                }
                current = Some((
                    captures
                        .get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
                    captures
                        .get(2)
                        .map(|m| m.as_str().trim().to_string())
                        .filter(|s| !s.is_empty()),
                    captures
                        .get(3)
                        .map(|m| m.as_str().trim().to_string())
                        .unwrap_or_default(),
                ));
            } else if let Some((_, _, body)) = &mut current {
                body.push('\n');
                body.push_str(line);
            }
        }

        if let Some((date, author, body)) = current.take() {
            if let Some(message) = self.build_message(&date, author, body) {
                messages.push(message);
            }
        }

        messages
    }

    fn build_message(
        &self,
        date: &str,
        author: Option<String>,
        body: String,
    ) -> Option<ParsedMessage> {
        let created_at = self.parse_date(date).ok()?;
        let attachment_name = self
            .attachment_re
            .captures(body.lines().next().unwrap_or(""))
            .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()));
        Some(ParsedMessage {
            author: author.unwrap_or_default(),
            author_type: if body.is_empty() { "SYSTEM" } else { "USER" }.to_string(),
            created_at,
            content: body,
            attachment_name,
        })
    }

    fn parse_date(&self, text: &str) -> Result<NaiveDateTime> {
        let cleaned = text.trim().trim_matches(&['[', ']'][..]).replace(',', ".");
        if let Some(format) = &self.custom_format {
            return Ok(NaiveDateTime::parse_from_str(&cleaned, format)?);
        }
        let normalized = cleaned.replace(['-', '/', ','], ".");
        let candidates = [
            "%d.%m.%Y. %H:%M:%S",
            "%d.%m.%Y. %H:%M",
            "%d.%m.%y. %H:%M:%S",
            "%d.%m.%y. %H:%M",
            "%m.%d.%Y. %H:%M:%S",
            "%m.%d.%Y. %H:%M",
            "%m.%d.%y. %H:%M:%S",
            "%m.%d.%y. %H:%M",
            "%d.%m.%Y. %I:%M %p",
            "%m.%d.%Y. %I:%M %p",
        ];
        for candidate in candidates {
            if let Ok(parsed) = NaiveDateTime::parse_from_str(&normalized, candidate) {
                return Ok(parsed);
            }
        }
        Err(anyhow!("unsupported WhatsApp date: {text}"))
    }
}
