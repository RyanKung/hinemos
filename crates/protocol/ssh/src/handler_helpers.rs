use super::*;

pub(super) fn slash_parse_feedback(line: &str, error: &SlashParseError) -> String {
    let command = line
        .trim()
        .strip_prefix('/')
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("")
        .to_ascii_lowercase();
    match error {
        SlashParseError::MissingArgument => match command.as_str() {
            "read" => "What do you want to read? Try /read <name>.".to_owned(),
            "inspect" | "x" | "examine" => {
                "What do you want to inspect? Try /inspect <name>.".to_owned()
            }
            "go" | "move" => "Which street direction do you want? Try /go north or /go west.".to_owned(),
            "enter" | "visit" => "Where do you want to enter? Try /enter <place>.".to_owned(),
            "talk" => "Who do you want to talk to? Try /talk <name>.".to_owned(),
            "take" | "get" | "pick" => "What do you want to take? Try /take <name>.".to_owned(),
            "pay" => {
                "Who do you want to pay, and how much? Try /pay <user> <amount> <memo>.".to_owned()
            }
            "mail" | "inbox" => "Which mail item do you mean? Try /mail read <id>.".to_owned(),
            "parcel" => "Which parcel command do you need? Try /parcel list, /parcel info <parcel>, /parcel build, or /parcel work list <parcel>.".to_owned(),
            "settings" => "Which setting do you want to change? Try /settings.".to_owned(),
            _ => "That command needs a little more detail. Choose one Available command and include its target."
                .to_owned(),
        },
        SlashParseError::UnexpectedArgument => {
            "That command does not need anything after it. Send it by itself.".to_owned()
        }
        SlashParseError::InvalidAmount => "The amount must be a plain number of MARK.".to_owned(),
        SlashParseError::InvalidInboxFilter => {
            "That mailbox shelf is unknown. Try open, unread, claimed, done, or all.".to_owned()
        }
        SlashParseError::InvalidJson => {
            "The build sheet could not be read as JSON. Check the braces and quotes.".to_owned()
        }
        SlashParseError::InvalidRoleCardName => {
            "Role-card name must be one non-empty line, at most 64 characters. Try /settings name <name>."
                .to_owned()
        }
        SlashParseError::InvalidGender => {
            "Gender must be male, female, or none. Try /settings gender none.".to_owned()
        }
        SlashParseError::InvalidMbti => {
            "MBTI must be one of the 16 standard types, like INTJ, INFP, ESTP, or ESFJ. Try /settings mbti INTJ."
                .to_owned()
        }
        SlashParseError::InvalidIntro => {
            "Self introduction must be a single line at most 160 characters. Try /settings intro <one line> or /settings intro clear."
                .to_owned()
        }
        SlashParseError::UnknownCommand => {
            "That command is not on the town board. Choose one Available command.".to_owned()
        }
    }
}

pub(super) fn mailbox_help() -> &'static str {
    "Commands: HELP, IDLE, LIST [open|unread|claimed|done|all], READ <id>, SEND <user-or-address> <body>, ACK <id>, NOOP, QUIT"
}

pub(super) fn generate_mail_auth_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn parse_mailbox_item_id(input: &str) -> Result<i64> {
    input
        .trim()
        .parse::<i64>()
        .context("mail item id must be an integer")
}
