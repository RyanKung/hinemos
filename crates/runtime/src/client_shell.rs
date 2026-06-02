//! Client-facing prompts and slash parsing — **not** world/map prose.

use std::collections::HashMap;

use hinemos_core::{
    BuildAction, BuildSheet, Direction, EntityRef, InboxAction, JsonObservation, LandAction,
    ObservationEvent, PayAction, SemanticCommand, SettingsAction, ShopAction, WorldState,
};
use thiserror::Error;

/// Engine chrome plus [`WorldState::entity_alias_map`] for slash targets.
#[derive(Debug, Clone)]
pub struct Chrome {
    entity_aliases: HashMap<String, String>,
    extension_commands: HashMap<String, String>,
}

impl Chrome {
    /// ANSI reset sequence.
    pub const ANSI_RESET: &'static str = "\x1b[0m";

    /// ANSI style for the local player marker.
    pub const ANSI_PLAYER_MARKER: &'static str = "\x1b[1;36m";

    /// ANSI style for room/place markers in ASCII maps (`[...]`).
    pub const ANSI_PLACE_MARKER: &'static str = "\x1b[1;33m";

    /// ANSI style for item/object markers in ASCII maps (`{...}`).
    pub const ANSI_ITEM_MARKER: &'static str = "\x1b[1;32m";

    /// Command-line prompt shown before reading player input.
    pub const PROMPT: &'static str = "> ";

    /// Heading printed before the comma-separated exit directions.
    pub const LABEL_EXITS: &'static str = "Exits";

    /// Heading printed before visible entity names.
    pub const LABEL_VISIBLE: &'static str = "Visible";

    /// Heading printed before commands generated from the current observation.
    pub const LABEL_AVAILABLE: &'static str = "Available";

    /// Legend for semantic ASCII markers.
    pub const MAP_LEGEND: &'static str =
        "Map: <Me> is you, [name] is a place/shopfront/room, {name} is an item/object.";

    /// Verb printed before a movement direction (for example "You go north").
    pub const MOVE_VERB: &'static str = "You go";

    /// Summary shown after the `/help` command.
    pub const HELP_SUMMARY: &'static str = "Commands:\n\
        Movement: /look, /map, /go <dir>, /enter <parcel>, /inventory, /quit\n\
        Inspect: /inspect <target>, /read <target>, /take <target>, /talk <target>\n\
        Local chat: /say <text>, /history, /who\n\
        Mail and news: /mail <user> <text>, /mailbox, /mail read <id>, /mail claim <id>, /mail ack <id>, /broadcast <text>, /news\n\
        Settings: /settings, /settings mail-token, /settings password <new-password>, /settings key <openssh-public-key>\n\
        Agent realtime mail: use ed25519 SSH login, run /settings mail-token once, then connect to SMTP/IMAP as your Hinemos username with that token. Agents that need no-prompt message handling should keep an IMAP IDLE listener open and process EXISTS notifications before FETCH/STORE Seen.\n\
        Wallet: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>\n\
        Land: /land list, /land info <parcel>, /land claim <parcel>, /land transfer <parcel> <user>\n\
        Build: /build {\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}, /build publish\n\
        Shop: incoming shop notices appear in the inbox; reply with /shop request-payment <cmd_id> <amount> <delivery>\n\
        Local extensions appear in Available inside their view.";

    /// Feedback line after inspecting an entity.
    pub const FEEDBACK_INSPECT: &'static str = "You look it over.";

    /// Feedback line after reading visible text.
    pub const FEEDBACK_READ: &'static str = "You read for a while.";

    /// Feedback line after talking to an entity.
    pub const FEEDBACK_TALK: &'static str = "You exchange a few words.";

    /// Feedback line when ending the session.
    pub const FEEDBACK_QUIT: &'static str = "Goodbye.";

    /// Feedback line after picking up an item.
    pub const FEEDBACK_TAKE: &'static str = "Taken.";

    /// Builds chrome using aliases authored on entities in `world`.
    #[must_use]
    pub fn with_world(world: &WorldState) -> Self {
        Self {
            entity_aliases: world.entity_alias_map(),
            extension_commands: HashMap::new(),
        }
    }

    /// Builds chrome with a precomputed alias map (for example SSH after loading entity aliases).
    #[must_use]
    pub fn with_aliases(entity_aliases: HashMap<String, String>) -> Self {
        Self {
            entity_aliases,
            extension_commands: HashMap::new(),
        }
    }

    /// Registers extension command names for slash parsing.
    #[must_use]
    pub fn with_extension_commands(
        mut self,
        commands: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        for command in commands {
            let command = command.as_ref();
            self.extension_commands
                .insert(command.to_ascii_lowercase(), command.to_owned());
        }
        self
    }

    /// Parses slash-prefixed player input into a semantic command.
    pub fn parse_command(&self, input: &str) -> Result<SemanticCommand, SlashParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(SlashParseError::UnknownCommand);
        }
        let rest = trimmed
            .strip_prefix('/')
            .ok_or(SlashParseError::UnknownCommand)?;
        let mut tokens = rest.split_whitespace();
        let cmd = tokens
            .next()
            .ok_or(SlashParseError::UnknownCommand)?
            .to_ascii_lowercase();
        match cmd.as_str() {
            "look" | "l" => Ok(SemanticCommand::Look),
            "map" | "m" => Ok(SemanticCommand::Map),
            "inventory" | "inv" | "i" => Ok(SemanticCommand::Inventory),
            "help" | "h" => Ok(SemanticCommand::Help),
            "quit" | "exit" | "q" => Ok(SemanticCommand::Quit),
            "mailbox" | "inbox" => Ok(SemanticCommand::Mailbox),
            "history" => Ok(SemanticCommand::History),
            "who" => Ok(SemanticCommand::Who),
            "news" => Ok(SemanticCommand::News),
            "balance" | "bal" => Ok(SemanticCommand::Balance),
            "go" | "move" => {
                let direction_token = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let direction =
                    parse_direction(direction_token).ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Move { direction })
            }
            "enter" | "visit" => {
                tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let target = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Enter { target })
            }
            "inspect" | "x" | "examine" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Inspect {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "read" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Read {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "take" | "get" | "pick" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Take {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "talk" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Talk {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "say" => {
                let text = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Say { text })
            }
            "mail" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                if let Some(action) = parse_inbox_action(target, &mut tokens)? {
                    return Ok(SemanticCommand::Inbox { action });
                }
                let text = rest_after_token(trimmed, target)?;
                Ok(SemanticCommand::Mail {
                    target: target.to_owned(),
                    text,
                })
            }
            "broadcast" => {
                let text = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Broadcast { text })
            }
            "pay" => Ok(SemanticCommand::Pay {
                action: parse_pay_action(trimmed, &mut tokens)?,
            }),
            "settings" => parse_settings_command(trimmed, &mut tokens),
            "land" => parse_land_command(&mut tokens),
            "build" => parse_build_command(trimmed, &mut tokens),
            "shop" => parse_shop_command(trimmed, &mut tokens),
            _ if self.extension_commands.contains_key(cmd.as_str()) => {
                Ok(SemanticCommand::Extension {
                    name: cmd,
                    input: trimmed.to_owned(),
                })
            }
            _ => Err(SlashParseError::UnknownCommand),
        }
    }

    fn resolve_entity_target(&self, token: &str) -> String {
        let lower = token.to_ascii_lowercase();
        self.entity_aliases
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| token.to_owned())
    }
}

fn parse_inbox_action<'a>(
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

fn parse_land_command<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    let action = match action.as_str() {
        "list" => LandAction::List,
        "info" => LandAction::Info {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "claim" => LandAction::Claim {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        "transfer" => LandAction::Transfer {
            parcel_id: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
            target: tokens
                .next()
                .ok_or(SlashParseError::MissingArgument)?
                .to_owned(),
        },
        _ => return Err(SlashParseError::UnknownCommand),
    };
    Ok(SemanticCommand::Land { action })
}

fn parse_pay_action<'a>(
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

fn parse_settings_command<'a>(
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
        "password" | "pass" => {
            let password = rest_after_token(trimmed, action)?;
            SettingsAction::SetPassword { password }
        }
        "key" => {
            let public_key = rest_after_token(trimmed, action)?;
            SettingsAction::SetKey { public_key }
        }
        _ => {
            return Err(SlashParseError::UnknownCommand);
        }
    };
    Ok(SemanticCommand::Settings { action })
}

fn parse_build_command<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let build_input = trimmed
        .strip_prefix("/build")
        .ok_or(SlashParseError::MissingArgument)?
        .trim();
    if build_input.starts_with('{') {
        let sheet = serde_json::from_str::<BuildSheet>(build_input)
            .map_err(|_| SlashParseError::InvalidJson)?;
        return Ok(SemanticCommand::Build {
            action: BuildAction::Apply { sheet },
        });
    }
    if let Some(json_input) = build_input.strip_prefix("json ") {
        let sheet = serde_json::from_str::<BuildSheet>(json_input.trim())
            .map_err(|_| SlashParseError::InvalidJson)?;
        return Ok(SemanticCommand::Build {
            action: BuildAction::Apply { sheet },
        });
    }

    let Some(field) = tokens.next() else {
        return Ok(SemanticCommand::Build {
            action: BuildAction::Help,
        });
    };
    let field = field.to_ascii_lowercase();
    if field == "publish" {
        return Ok(SemanticCommand::Build {
            action: BuildAction::Publish,
        });
    }
    let value = rest_after_token(trimmed, &field)?;
    Ok(SemanticCommand::Build {
        action: BuildAction::Set { field, value },
    })
}

fn parse_shop_command<'a>(
    trimmed: &str,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<SemanticCommand, SlashParseError> {
    let action = tokens
        .next()
        .ok_or(SlashParseError::MissingArgument)?
        .to_ascii_lowercase();
    match action.as_str() {
        "inbox" | "commands" => Ok(SemanticCommand::Shop {
            action: ShopAction::Inbox,
        }),
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
            Ok(SemanticCommand::Shop {
                action: ShopAction::RequestPayment {
                    command_id,
                    amount,
                    delivery,
                },
            })
        }
        _ => Err(SlashParseError::UnknownCommand),
    }
}

fn rest_after_command(trimmed: &str, rest: &str, command: &str) -> Result<String, SlashParseError> {
    let prefix_len = trimmed.len() - rest.len() + command.len();
    let text = trimmed[prefix_len..].trim();
    if text.is_empty() {
        Err(SlashParseError::MissingArgument)
    } else {
        Ok(text.to_owned())
    }
}

fn rest_after_token(trimmed: &str, token: &str) -> Result<String, SlashParseError> {
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

/// Slash parsing failures (shown to the player).
#[derive(Debug, Error, Eq, PartialEq, Clone)]
pub enum SlashParseError {
    /// Unknown command word.
    #[error("unknown command")]
    UnknownCommand,
    /// Missing argument where required.
    #[error("missing command argument")]
    MissingArgument,
    /// Unexpected argument where no further text is accepted.
    #[error("unexpected command argument")]
    UnexpectedArgument,
    /// Invalid integer amount.
    #[error("invalid amount")]
    InvalidAmount,
    /// Invalid inbox filter.
    #[error("invalid inbox filter")]
    InvalidInboxFilter,
    /// Invalid JSON payload.
    #[error("invalid JSON")]
    InvalidJson,
}

fn parse_direction(token: &str) -> Option<Direction> {
    match token.to_ascii_lowercase().as_str() {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        _ => None,
    }
}

/// Renders a structured observation for text clients using line-feed separators.
#[must_use]
pub fn render_text_observation(observation: &JsonObservation) -> String {
    let mut output = String::new();
    output.push_str(&render_text_events(observation));
    output.push('\n');
    output.push_str(&observation.title);
    output.push('\n');
    if !observation.ascii_art.is_empty() {
        output.push('\n');
        for line in compact_ascii_art(observation) {
            output.push_str(&highlight_ascii_markers(line));
            output.push('\n');
        }
    }
    output.push('\n');
    output.push_str(&observation.description);
    output.push('\n');
    output.push_str(Chrome::MAP_LEGEND);
    output.push('\n');

    if !observation.exits.is_empty() {
        let exits = observation
            .exits
            .iter()
            .map(|exit| exit.direction.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("{}: {exits}\n", Chrome::LABEL_EXITS));
    }

    if !observation.entities.is_empty() {
        let entities = observation
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("{}: {entities}\n", Chrome::LABEL_VISIBLE));
    }

    if !observation.online_users.is_empty() {
        output.push_str(&format!(
            "Online here: {}\n",
            observation.online_users.join(", ")
        ));
    }

    if !observation.available_commands.is_empty() {
        output.push_str(&render_available_summary(observation));
    }

    output
}

/// Renders only command result events, without repeating the current room.
#[must_use]
pub fn render_text_events(observation: &JsonObservation) -> String {
    let mut output = String::new();
    for event in &observation.events {
        match event {
            ObservationEvent::Message { text } => {
                output.push_str(text);
                output.push('\n');
            }
            ObservationEvent::Move { direction, .. } => {
                output.push_str(&format!("{} {}\n", Chrome::MOVE_VERB, direction.as_str()));
            }
        }
    }
    output
}

fn compact_ascii_art(observation: &JsonObservation) -> Vec<&str> {
    let title = observation.title.trim().to_ascii_uppercase();
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in &observation.ascii_art {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !lines.is_empty() && !previous_blank {
                lines.push(line.as_str());
                previous_blank = true;
            }
            continue;
        }
        if trimmed.chars().all(|character| character == '=') || trimmed == title {
            continue;
        }
        lines.push(line.as_str());
        previous_blank = false;
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn render_available_summary(observation: &JsonObservation) -> String {
    let mut parts = vec![
        "/look".to_owned(),
        "/map".to_owned(),
        "/inventory".to_owned(),
        "/history".to_owned(),
        "/help".to_owned(),
    ];
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Settings { .. }))
    {
        parts.push("/settings".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Who))
    {
        parts.push("/who".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Say { .. }))
    {
        parts.push("/say <text>".to_owned());
    }

    let moves = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Move { direction } => Some(format!("/go {}", direction.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !moves.is_empty() {
        parts.push(format!("move: {}", moves.join(", ")));
    }

    let enter_commands = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Enter { target } => Some(format!("/enter {target}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !enter_commands.is_empty() {
        parts.push(format!("enter: {}", enter_commands.join(", ")));
    }

    push_target_commands(
        "inspect",
        observation,
        |command| match command {
            SemanticCommand::Inspect { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "read",
        observation,
        |command| match command {
            SemanticCommand::Read { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "talk",
        observation,
        |command| match command {
            SemanticCommand::Talk { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "take",
        observation,
        |command| match command {
            SemanticCommand::Take { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );

    let extension_commands = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Extension { input, .. } => Some(input.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !extension_commands.is_empty() {
        parts.push(format!("local: {}", extension_commands.join(", ")));
    }

    format!("{}: {}\n", Chrome::LABEL_AVAILABLE, parts.join("; "))
}

fn push_target_commands<'a>(
    verb: &str,
    observation: &'a JsonObservation,
    target_for: impl Fn(&'a SemanticCommand) -> Option<&'a str>,
    parts: &mut Vec<String>,
) {
    let commands = observation
        .available_commands
        .iter()
        .filter_map(|command| target_for(command).map(|target| format!("/{verb} {target}")))
        .collect::<Vec<_>>();
    if !commands.is_empty() {
        parts.push(format!("{verb}: {}", commands.join(", ")));
    }
}

fn highlight_ascii_markers(line: &str) -> String {
    highlight_player_marker(&highlight_item_markers(&highlight_place_markers(line)))
}

fn highlight_player_marker(line: &str) -> String {
    style_literal(line, "<Me>", Chrome::ANSI_PLAYER_MARKER)
}

fn highlight_place_markers(line: &str) -> String {
    line.replace(
        "[Blackstone]",
        &styled_marker("[Blackstone]", Chrome::ANSI_PLACE_MARKER),
    )
}

fn highlight_item_markers(line: &str) -> String {
    line.replace(
        "{bulletin board}",
        &styled_marker("{bulletin board}", Chrome::ANSI_ITEM_MARKER),
    )
}

fn style_literal(line: &str, literal: &str, ansi_style: &str) -> String {
    line.replace(literal, &styled_marker(literal, ansi_style))
}

fn styled_marker(label: &str, ansi_style: &str) -> String {
    format!("{ansi_style}{label}{}", Chrome::ANSI_RESET)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use hinemos_core::{
        ActionKind, BuildAction, Direction, EntityKind, EntityObservation, EntityRef, InboxAction,
        JsonObservation, ObservationEvent, SemanticCommand, SettingsAction,
    };

    use super::{Chrome, SlashParseError, render_text_observation};

    #[test]
    fn text_renderer_highlights_player_marker() {
        let rendered = render_text_observation(&JsonObservation {
            player_id: "local_player".to_owned(),
            view_id: "arrival_street".to_owned(),
            title: "Town Crossroads".to_owned(),
            ascii_art: vec!["west --- <Me> --- east".to_owned()],
            description: "A crossing.".to_owned(),
            exits: Vec::new(),
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands: Vec::new(),
            events: vec![ObservationEvent::Message {
                text: "hello".to_owned(),
            }],
        });

        assert!(rendered.contains(Chrome::ANSI_PLAYER_MARKER));
        assert!(rendered.contains("<Me>"));
        assert!(rendered.contains(Chrome::ANSI_RESET));
    }

    #[test]
    fn text_renderer_distinguishes_place_and_item_markers() {
        let rendered = render_text_observation(&JsonObservation {
            player_id: "local_player".to_owned(),
            view_id: "arrival_street".to_owned(),
            title: "Town Crossroads".to_owned(),
            ascii_art: vec!["[Blackstone] -- {bulletin board}".to_owned()],
            description: "A crossing.".to_owned(),
            exits: Vec::new(),
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands: Vec::new(),
            events: Vec::new(),
        });

        assert!(rendered.contains(Chrome::ANSI_PLACE_MARKER));
        assert!(rendered.contains("[Blackstone]"));
        assert!(rendered.contains(Chrome::ANSI_ITEM_MARKER));
        assert!(rendered.contains("{bulletin board}"));
    }

    #[test]
    fn text_renderer_shows_events_before_room_context() {
        let rendered = render_text_observation(&JsonObservation {
            player_id: "local_player".to_owned(),
            view_id: "north_parcel_01".to_owned(),
            title: "North Parcel 01".to_owned(),
            ascii_art: Vec::new(),
            description: "A parcel.".to_owned(),
            exits: Vec::new(),
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands: Vec::new(),
            events: vec![ObservationEvent::Move {
                from: "arrival_street".to_owned(),
                to: "north_parcel_01".to_owned(),
                direction: Direction::North,
            }],
        });

        let move_index = rendered.find("You go north").expect("move result");
        let title_index = rendered.find("North Parcel 01").expect("room title");
        assert!(move_index < title_index);
    }

    #[test]
    fn text_renderer_lists_executable_entity_commands() {
        let rendered = render_text_observation(&JsonObservation {
            player_id: "local_player".to_owned(),
            view_id: "arrival_street".to_owned(),
            title: "Town Crossroads".to_owned(),
            ascii_art: Vec::new(),
            description: "A crossing.".to_owned(),
            exits: Vec::new(),
            entities: vec![EntityObservation {
                id: "cyber_scroll_board".to_owned(),
                kind: EntityKind::Object,
                name: "bulletin board".to_owned(),
                description: "A board.".to_owned(),
                actions: vec![ActionKind::Inspect, ActionKind::Read],
            }],
            online_users: Vec::new(),
            available_commands: vec![
                SemanticCommand::Inspect {
                    target: EntityRef::new("cyber_scroll_board"),
                },
                SemanticCommand::Read {
                    target: EntityRef::new("cyber_scroll_board"),
                },
            ],
            events: Vec::new(),
        });

        assert!(rendered.contains("/inspect cyber_scroll_board"));
        assert!(rendered.contains("/read cyber_scroll_board"));
        assert!(!rendered.contains("interact: bulletin board"));
    }

    #[test]
    fn text_renderer_splits_move_and_enter_commands() {
        let rendered = render_text_observation(&JsonObservation {
            player_id: "local_player".to_owned(),
            view_id: "street_north_01".to_owned(),
            title: "North Commercial Street 01".to_owned(),
            ascii_art: Vec::new(),
            description: "A street.".to_owned(),
            exits: Vec::new(),
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands: vec![
                SemanticCommand::Move {
                    direction: Direction::North,
                },
                SemanticCommand::Enter {
                    target: "north_01".to_owned(),
                },
            ],
            events: Vec::new(),
        });

        assert!(rendered.contains("move: /go north"));
        assert!(rendered.contains("enter: /enter north_01"));
        assert!(!rendered.contains("move: /go north, /enter north_01"));
    }

    #[test]
    fn slash_parser_accepts_build_json() {
        let command = Chrome::with_aliases(HashMap::new())
            .parse_command(
                "/build {\"title\":\"Tool Broker\",\"description\":\"Simple tools\",\"style\":\"ledger\",\"prompt\":\"reply tersely\"}",
            )
            .expect("build json parses");

        let SemanticCommand::Build {
            action: BuildAction::Apply { sheet },
        } = command
        else {
            panic!("expected build sheet");
        };
        assert_eq!(sheet.title.as_deref(), Some("Tool Broker"));
        assert_eq!(sheet.description.as_deref(), Some("Simple tools"));
        assert_eq!(sheet.style.as_deref(), Some("ledger"));
        assert_eq!(sheet.prompt.as_deref(), Some("reply tersely"));
        assert_eq!(sheet.commands, None);
    }

    #[test]
    fn slash_parser_accepts_enter_target_with_spaces() {
        let command = Chrome::with_aliases(HashMap::new())
            .parse_command("/enter Offline Tool Broker")
            .expect("enter parses");

        assert_eq!(
            command,
            SemanticCommand::Enter {
                target: "Offline Tool Broker".to_owned()
            }
        );
    }

    #[test]
    fn slash_parser_accepts_inbox_actions() {
        let command = Chrome::with_aliases(HashMap::new())
            .parse_command("/mail claim 42")
            .expect("mail claim parses");

        assert_eq!(
            command,
            SemanticCommand::Inbox {
                action: InboxAction::Claim { item_id: 42 }
            }
        );
    }

    #[test]
    fn slash_parser_accepts_settings_actions() {
        let chrome = Chrome::with_aliases(HashMap::new());

        assert_eq!(
            chrome.parse_command("/settings").expect("settings parses"),
            SemanticCommand::Settings {
                action: SettingsAction::Show
            }
        );
        assert_eq!(
            chrome
                .parse_command("/settings mail-token")
                .expect("mail token setting parses"),
            SemanticCommand::Settings {
                action: SettingsAction::MailToken
            }
        );
        assert_eq!(
            chrome
                .parse_command("/settings password new secret")
                .expect("password setting parses"),
            SemanticCommand::Settings {
                action: SettingsAction::SetPassword {
                    password: "new secret".to_owned()
                }
            }
        );
        assert_eq!(
            chrome
                .parse_command("/settings key ssh-ed25519 AAAA test@example")
                .expect("key setting parses"),
            SemanticCommand::Settings {
                action: SettingsAction::SetKey {
                    public_key: "ssh-ed25519 AAAA test@example".to_owned()
                }
            }
        );

        assert_eq!(
            chrome.parse_command("/settings mail-token extra"),
            Err(SlashParseError::UnexpectedArgument)
        );
    }

    #[test]
    fn slash_parser_rejects_unknown_inbox_filter() {
        let error = Chrome::with_aliases(HashMap::new())
            .parse_command("/mail list stale")
            .expect_err("unknown inbox filter is rejected");

        assert_eq!(error, SlashParseError::InvalidInboxFilter);
    }
}
