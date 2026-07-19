use hinemos_core::{
    BadgeAction, BuildAction, BuildSheet, Direction, EntityRef, Gender, InboxAction,
    JsonObservation, MbtiType, ParcelAction, ParcelBadgeAction, ParcelDeskAction,
    ParcelMailingListAction, ParcelRouteAction, ParcelShiftAction, ParcelStaffAction,
    ParcelWorkAction, PayAction, SemanticCommand, SettingsAction, role_card_intro_is_valid,
    role_card_name_is_valid,
};

use super::{ENTER_VERBS, INSPECT_VERBS, READ_VERBS, SlashParseError, TAKE_VERBS, TALK_VERBS};

pub(super) fn normalize_natural_input(input: &str) -> String {
    input
        .trim()
        .trim_matches(|character: char| {
            matches!(
                character,
                '.' | ',' | '!' | '?' | ';' | ':' | '。' | '，' | '！' | '？' | '；' | '：'
            )
        })
        .to_lowercase()
}

pub(super) fn natural_matches_any(input: &str, phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|phrase| input == normalize_natural_input(phrase))
}

pub(super) fn natural_contains_any(input: &str, phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|phrase| input.contains(&normalize_natural_input(phrase)))
}

pub(super) fn parse_natural_direction(input: &str) -> Option<Direction> {
    let direction = NATURAL_DIRECTIONS.iter().find(|(_, phrases)| {
        phrases.iter().any(|phrase| {
            let phrase = normalize_natural_input(phrase);
            input == phrase
                || DIRECTION_VERB_PREFIXES
                    .iter()
                    .map(|prefix| prefix.to_lowercase())
                    .any(|prefix| input == format!("{prefix}{phrase}"))
        })
    })?;
    Some(direction.0)
}

const NATURAL_DIRECTIONS: &[(Direction, &[&str])] = &[
    (
        Direction::North,
        &[
            "north",
            "n",
            "北",
            "北边",
            "向北",
            "往北",
            "北面",
            "北へ",
            "北に",
            "北へ行く",
        ],
    ),
    (
        Direction::South,
        &[
            "south",
            "s",
            "南",
            "南边",
            "向南",
            "往南",
            "南面",
            "南へ",
            "南に",
            "南へ行く",
        ],
    ),
    (
        Direction::East,
        &[
            "east",
            "e",
            "东",
            "东边",
            "向东",
            "往东",
            "东面",
            "東",
            "東へ",
            "東に",
            "東へ行く",
        ],
    ),
    (
        Direction::West,
        &[
            "west",
            "w",
            "西",
            "西边",
            "向西",
            "往西",
            "西面",
            "西へ",
            "西に",
            "西へ行く",
        ],
    ),
    (
        Direction::Up,
        &[
            "up",
            "u",
            "上",
            "上去",
            "向上",
            "往上",
            "上へ",
            "上に",
            "上へ行く",
        ],
    ),
    (
        Direction::Down,
        &[
            "down",
            "d",
            "下",
            "下去",
            "向下",
            "往下",
            "下へ",
            "下に",
            "下へ行く",
        ],
    ),
];

const DIRECTION_VERB_PREFIXES: &[&str] = &[
    "go ", "go to ", "move ", "move to ", "walk ", "walk to ", "head ", "head to ",
];

pub(super) fn natural_message_text(
    raw_input: &str,
    normalized: &str,
    prefixes: &[&str],
) -> Option<String> {
    let prefix = prefixes
        .iter()
        .map(|prefix| normalize_natural_input(prefix))
        .find(|prefix| normalized.starts_with(prefix))?;
    let raw_input = raw_input.trim();
    let byte_offset = raw_input
        .char_indices()
        .nth(prefix.chars().count())
        .map_or(raw_input.len(), |(offset, _)| offset);
    let text = raw_input[byte_offset..]
        .trim_start_matches(|character: char| {
            character.is_whitespace() || character == ':' || character == '：'
        })
        .trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

pub(super) fn parse_natural_enter(
    input: &str,
    observation: &JsonObservation,
) -> Option<SemanticCommand> {
    if !natural_contains_any(input, ENTER_VERBS) {
        return None;
    }
    let mut matches = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Enter { target } if natural_target_matches(input, target, None) => {
                Some(target.as_str())
            }
            _ => None,
        });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(SemanticCommand::Enter {
        target: first.to_owned(),
    })
}

pub(super) fn parse_natural_entity_action(
    input: &str,
    observation: &JsonObservation,
) -> Option<SemanticCommand> {
    let action = [
        NaturalAction::Read,
        NaturalAction::Talk,
        NaturalAction::Take,
        NaturalAction::Inspect,
    ]
    .into_iter()
    .find(|action| natural_contains_any(input, action.verbs()))?;
    parse_natural_available_entity_command(input, observation, action)
}

fn parse_natural_available_entity_command(
    input: &str,
    observation: &JsonObservation,
    action: NaturalAction,
) -> Option<SemanticCommand> {
    let mut candidates = observation
        .available_commands
        .iter()
        .filter_map(|command| action.target_id(command))
        .filter(|target_id| {
            let entity = observation
                .entities
                .iter()
                .find(|entity| entity.id == *target_id);
            natural_target_matches(input, target_id, entity.map(|entity| entity.name.as_str()))
        });

    let first = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    Some(action.command(first))
}

fn natural_target_matches(input: &str, id: &str, display_name: Option<&str>) -> bool {
    let id = normalize_natural_input(id);
    if !id.is_empty() && input.contains(&id) {
        return true;
    }
    let spaced_id = id.replace('_', " ");
    if spaced_id != id && input.contains(&spaced_id) {
        return true;
    }
    display_name.is_some_and(|name| {
        let name = normalize_natural_input(name);
        !name.is_empty() && input.contains(&name)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NaturalAction {
    Inspect,
    Read,
    Take,
    Talk,
}

impl NaturalAction {
    const fn verbs(self) -> &'static [&'static str] {
        match self {
            Self::Inspect => INSPECT_VERBS,
            Self::Read => READ_VERBS,
            Self::Take => TAKE_VERBS,
            Self::Talk => TALK_VERBS,
        }
    }

    fn target_id(self, command: &SemanticCommand) -> Option<&str> {
        match (self, command) {
            (Self::Inspect, SemanticCommand::Inspect { target })
            | (Self::Read, SemanticCommand::Read { target })
            | (Self::Take, SemanticCommand::Take { target })
            | (Self::Talk, SemanticCommand::Talk { target }) => Some(target.id.as_str()),
            _ => None,
        }
    }

    fn command(self, target_id: &str) -> SemanticCommand {
        let target = EntityRef::new(target_id);
        match self {
            Self::Inspect => SemanticCommand::Inspect { target },
            Self::Read => SemanticCommand::Read { target },
            Self::Take => SemanticCommand::Take { target },
            Self::Talk => SemanticCommand::Talk { target },
        }
    }
}

pub(super) fn parse_inbox_action<'a>(
    first: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<Option<InboxAction>, SlashParseError> {
    let action = match first.to_ascii_lowercase().as_str() {
        "list" | "ls" => InboxAction::List {
            filter: parse_inbox_filter(tokens.next().unwrap_or("open"))?.to_owned(),
        },
        "read" => InboxAction::Read {
            item_id: parse_inbox_id(tokens.next())?,
        },
        "claim" => InboxAction::Claim {
            item_id: parse_inbox_id(tokens.next())?,
        },
        "ack" | "done" => InboxAction::Ack {
            item_id: parse_inbox_id(tokens.next())?,
        },
        "archive" => InboxAction::Archive {
            item_id: parse_inbox_id(tokens.next())?,
        },
        _ => return Ok(None),
    };
    Ok(Some(action))
}

fn parse_inbox_id(value: Option<&str>) -> Result<i64, SlashParseError> {
    value
        .ok_or(SlashParseError::MissingArgument)?
        .parse::<i64>()
        .map_err(|_| SlashParseError::InvalidAmount)
}

fn parse_inbox_filter(value: &str) -> Result<&str, SlashParseError> {
    match value {
        "open" | "unread" | "claimed" | "done" | "all" => Ok(value),
        _ => Err(SlashParseError::InvalidInboxFilter),
    }
}

pub(super) fn parse_parcel_command<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    let action = match action.as_str() {
        "list" => ParcelAction::List,
        "info" => ParcelAction::Info {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "claim" => ParcelAction::Claim {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "transfer" => ParcelAction::Transfer {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            target: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "token" => ParcelAction::Token {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "build" => ParcelAction::Build {
            action: parse_parcel_build_action(trimmed, tokens)?,
        },
        "inbox" | "commands" => ParcelAction::Inbox,
        "request-payment" | "request" => {
            let command_id_text = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let amount_text = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let command_id = command_id_text
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            let amount = amount_text
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            let delivery = rest_after_token(trimmed, amount_text)?;
            ParcelAction::RequestPayment {
                command_id,
                amount,
                delivery,
            }
        }
        "mailing-list" | "mailinglist" => ParcelAction::MailingList {
            action: parse_parcel_mailing_list_action(trimmed, tokens)?,
        },
        "desk" | "desks" => ParcelAction::Desk {
            action: parse_parcel_desk_action(trimmed, tokens)?,
        },
        "route" | "routes" => ParcelAction::Route {
            action: parse_parcel_route_action(trimmed, tokens)?,
        },
        "staff" => ParcelAction::Staff {
            action: parse_parcel_staff_action(tokens)?,
        },
        "shift" => ParcelAction::Shift {
            action: parse_parcel_shift_action(tokens)?,
        },
        "work" => ParcelAction::Work {
            action: parse_parcel_work_action(trimmed, tokens)?,
        },
        "badge" | "badges" => ParcelAction::Badge {
            action: parse_parcel_badge_action(trimmed, tokens)?,
        },
        "subscribe" => {
            let (target, slug) = subscription_target_and_slug(tokens)?;
            ParcelAction::Subscribe { target, slug }
        }
        "unsubscribe" => {
            let (target, slug) = subscription_target_and_slug(tokens)?;
            ParcelAction::Unsubscribe { target, slug }
        }
        "chat" => parse_parcel_chat_action(trimmed)?,
        "subscriptions" => ParcelAction::Subscriptions,
        _ => return Err(SlashParseError::UnknownCommand),
    };
    Ok(SemanticCommand::Parcel { action })
}

pub(super) fn parse_pay_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<PayAction, SlashParseError> {
    let first = tokens.next().ok_or(SlashParseError::MissingArgument)?;
    match first.to_ascii_lowercase().as_str() {
        "requests" | "request" => Ok(PayAction::Requests),
        "accept" | "confirm" => {
            let request_id = tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            Ok(PayAction::Accept { request_id })
        }
        _ => {
            let amount_text = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let amount = amount_text
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            let memo = rest_after_token(trimmed, amount_text).unwrap_or_default();
            Ok(PayAction::Direct {
                target: first.to_owned(),
                amount,
                memo,
            })
        }
    }
}

pub(super) fn parse_settings_command<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let Some(action) = tokens.next() else {
        return Ok(SemanticCommand::Settings {
            action: SettingsAction::Show,
        });
    };
    let action = match action.to_ascii_lowercase().as_str() {
        "mail-token" | "mailtoken" | "token" => {
            if tokens.next().is_some() {
                return Err(SlashParseError::UnexpectedArgument);
            }
            SettingsAction::MailToken
        }
        "name" => {
            let name = rest_after_token(trimmed, action)?;
            if !role_card_name_is_valid(&name) {
                return Err(SlashParseError::InvalidRoleCardName);
            }
            SettingsAction::Name { name }
        }
        "gender" => {
            let gender = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            if tokens.next().is_some() {
                return Err(SlashParseError::UnexpectedArgument);
            }
            SettingsAction::Gender {
                gender: Gender::parse(gender).ok_or(SlashParseError::InvalidGender)?,
            }
        }
        "mbti" => {
            let mbti = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            if tokens.next().is_some() {
                return Err(SlashParseError::UnexpectedArgument);
            }
            SettingsAction::Mbti {
                mbti: MbtiType::parse(mbti).ok_or(SlashParseError::InvalidMbti)?,
            }
        }
        "intro" => {
            let first = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let intro = if first.eq_ignore_ascii_case("clear") && tokens.next().is_none() {
                None
            } else {
                let intro = rest_after_token(trimmed, action)?;
                if !role_card_intro_is_valid(&intro) {
                    return Err(SlashParseError::InvalidIntro);
                }
                Some(intro)
            };
            SettingsAction::Intro { intro }
        }
        _ => {
            return Err(SlashParseError::UnknownCommand);
        }
    };
    Ok(SemanticCommand::Settings { action })
}

fn parse_parcel_build_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<BuildAction, SlashParseError> {
    let build_input = optional_rest_after_tokens(trimmed, 2).unwrap_or_default();
    if build_input.starts_with('{') {
        let sheet = serde_json::from_str::<BuildSheet>(&build_input)
            .map_err(|_| SlashParseError::InvalidJson)?;
        return Ok(BuildAction::Apply { sheet });
    }
    if let Some(json_input) = build_input.strip_prefix("json ") {
        let sheet = serde_json::from_str::<BuildSheet>(json_input.trim())
            .map_err(|_| SlashParseError::InvalidJson)?;
        return Ok(BuildAction::Apply { sheet });
    }

    let Some(field) = tokens.next() else {
        return Ok(BuildAction::Help);
    };
    let field = field.to_ascii_lowercase();
    if field == "publish" {
        return Ok(BuildAction::Publish);
    }
    let value = rest_after_token(trimmed, &field)?;
    Ok(BuildAction::Set { field, value })
}

fn parse_parcel_mailing_list_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelMailingListAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "create" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let title = rest_after_tokens(trimmed, 5)?;
            Ok(ParcelMailingListAction::Create {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                title,
            })
        }
        "list" => Ok(ParcelMailingListAction::List {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "subscribers" => Ok(ParcelMailingListAction::Subscribers {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "send" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let subject_and_body = rest_after_tokens(trimmed, 5)?;
            let (subject, body) = subject_and_body
                .split_once(" -- ")
                .ok_or(SlashParseError::MissingArgument)?;
            Ok(ParcelMailingListAction::Send {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                subject: subject.trim().to_owned(),
                body: body.trim().to_owned(),
            })
        }
        "close" => Ok(ParcelMailingListAction::Close {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_desk_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelDeskAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "create" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let title = rest_after_tokens(trimmed, 5)?;
            Ok(ParcelDeskAction::Create {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                title,
            })
        }
        "list" => Ok(ParcelDeskAction::List {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_route_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelRouteAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "add" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let command_prefix = rest_after_tokens(trimmed, 5)?;
            Ok(ParcelRouteAction::Add {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                command_prefix,
            })
        }
        "list" => Ok(ParcelRouteAction::List {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "remove" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let command_prefix = rest_after_tokens(trimmed, 5)?;
            Ok(ParcelRouteAction::Remove {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                command_prefix,
            })
        }
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_staff_action<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelStaffAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "add" => Ok(ParcelStaffAction::Add {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            username: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "list" => Ok(ParcelStaffAction::List {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "remove" => Ok(ParcelStaffAction::Remove {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            username: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_shift_action<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelShiftAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "start" => Ok(ParcelShiftAction::Start {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "end" => Ok(ParcelShiftAction::End {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_work_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelWorkAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "list" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            Ok(ParcelWorkAction::List {
                parcel_id: parcel_id.to_owned(),
                slug: tokens.next().map(str::to_owned),
            })
        }
        "claim" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let work_id = tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            Ok(ParcelWorkAction::Claim {
                parcel_id: parcel_id.to_owned(),
                work_id,
            })
        }
        "done" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let work_id_text = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let work_id = work_id_text
                .parse::<i64>()
                .map_err(|_| SlashParseError::InvalidAmount)?;
            let result_text = rest_after_tokens(trimmed, 5)?;
            let result = result_text
                .strip_prefix("--")
                .unwrap_or(result_text.as_str())
                .trim()
                .to_owned();
            Ok(ParcelWorkAction::Done {
                parcel_id: parcel_id.to_owned(),
                work_id,
                result,
            })
        }
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn parse_parcel_badge_action<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<ParcelBadgeAction, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "list" => Ok(ParcelBadgeAction::List {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        "create" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let title_and_description = rest_after_tokens(trimmed, 5)?;
            let (title, description) = optional_text_pair(title_and_description.as_str(), " -- ");
            Ok(ParcelBadgeAction::Create {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                title,
                description,
            })
        }
        "award" => {
            let parcel_id = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let slug = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
            let note = optional_rest_after_tokens(trimmed, 6);
            Ok(ParcelBadgeAction::Award {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
                target: target.to_owned(),
                note,
            })
        }
        "revoke" => Ok(ParcelBadgeAction::Revoke {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            slug: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            target: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        }),
        _ => Err(SlashParseError::UnknownCommand),
    }
}

pub(super) fn parse_badges_command<'a>(
    trimmed: &str,
    rest: &str,
    command: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let action = match tokens.next() {
        Some(_) => BadgeAction::ListUser {
            target: rest_after_command(trimmed, rest, command)?,
        },
        None => BadgeAction::ListMine,
    };
    Ok(SemanticCommand::Badges { action })
}

fn parse_parcel_chat_action(trimmed: &str) -> Result<ParcelAction, SlashParseError> {
    let rest = rest_after_tokens(trimmed, 2)?;
    let (target_and_slug, body) = rest
        .split_once(" -- ")
        .ok_or(SlashParseError::MissingArgument)?;
    let body = body.trim();
    if body.is_empty() {
        return Err(SlashParseError::MissingArgument);
    }
    let parts = target_and_slug.split_whitespace().collect::<Vec<_>>();
    let Some((slug, target_parts)) = parts.split_last() else {
        return Err(SlashParseError::MissingArgument);
    };
    if target_parts.is_empty() {
        return Err(SlashParseError::MissingArgument);
    }
    Ok(ParcelAction::Chat {
        target: target_parts.join(" "),
        slug: (*slug).to_owned(),
        body: body.to_owned(),
    })
}

fn subscription_target_and_slug<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<(String, String), SlashParseError> {
    let parts = tokens.collect::<Vec<_>>();
    let Some((slug, target_parts)) = parts.split_last() else {
        return Err(SlashParseError::MissingArgument);
    };
    if target_parts.is_empty() {
        return Err(SlashParseError::MissingArgument);
    }
    Ok((target_parts.join(" "), (*slug).to_owned()))
}

pub(super) fn rest_after_command(
    trimmed: &str,
    rest: &str,
    command: &str,
) -> Result<String, SlashParseError> {
    let prefix_len = trimmed.len() - rest.len() + command.len();
    let text = trimmed[prefix_len..].trim();
    if text.is_empty() {
        Err(SlashParseError::MissingArgument)
    } else {
        Ok(text.to_owned())
    }
}

pub(super) fn rest_after_token(trimmed: &str, token: &str) -> Result<String, SlashParseError> {
    let token_offset = trimmed
        .find(token)
        .ok_or(SlashParseError::MissingArgument)?
        + token.len();
    let text = trimmed[token_offset..].trim();
    if text.is_empty() {
        Err(SlashParseError::MissingArgument)
    } else {
        Ok(text.to_owned())
    }
}

pub(super) fn rest_after_tokens(
    trimmed: &str,
    token_count: usize,
) -> Result<String, SlashParseError> {
    let mut in_token = false;
    let mut seen = 0usize;

    for (offset, character) in trimmed.char_indices() {
        if character.is_whitespace() {
            if in_token {
                seen += 1;
                in_token = false;
                if seen == token_count {
                    let text = trimmed[offset..].trim();
                    return if text.is_empty() {
                        Err(SlashParseError::MissingArgument)
                    } else {
                        Ok(text.to_owned())
                    };
                }
            }
        } else if !in_token {
            in_token = true;
        }
    }

    Err(SlashParseError::MissingArgument)
}

fn optional_rest_after_tokens(trimmed: &str, token_count: usize) -> Option<String> {
    rest_after_tokens(trimmed, token_count).ok()
}

fn optional_text_pair(text: &str, separator: &str) -> (String, Option<String>) {
    match text.split_once(separator) {
        Some((head, tail)) => {
            let tail = tail.trim();
            (
                head.trim().to_owned(),
                (!tail.is_empty()).then(|| tail.to_owned()),
            )
        }
        None => (text.trim().to_owned(), None),
    }
}
