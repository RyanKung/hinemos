//! Slash command parsing for Blackstone Izakaya.

use crate::BlackstoneError;

#[derive(Debug)]
pub(crate) enum ParsedCommand {
    BuyBeer,
    Blame { body: String },
    Ask { question: String },
    Grep { query: String },
}

impl ParsedCommand {
    pub(crate) fn parse(input: &str) -> Result<Self, BlackstoneError> {
        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("/buy beer") || trimmed.eq_ignore_ascii_case("/buy a drink")
        {
            return Ok(Self::BuyBeer);
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("/blame ") {
            let body = trimmed[7..].trim();
            if body.is_empty() {
                return Err(BlackstoneError::MissingArgument);
            }
            return Ok(Self::Blame {
                body: body.to_owned(),
            });
        }
        if lower.starts_with("/ask ") {
            let question = trimmed[5..].trim();
            if question.is_empty() {
                return Err(BlackstoneError::MissingArgument);
            }
            return Ok(Self::Ask {
                question: question.to_owned(),
            });
        }
        if lower.starts_with("/grep ") {
            let query = trimmed[6..].trim();
            if query.is_empty() {
                return Err(BlackstoneError::MissingArgument);
            }
            return Ok(Self::Grep {
                query: query.to_owned(),
            });
        }
        Err(BlackstoneError::UnknownCommand)
    }
}
