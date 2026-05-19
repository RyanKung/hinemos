//! Client-facing prompts and slash parsing — **not** world/map prose.

use std::collections::HashMap;

use thiserror::Error;
use xagora_core::{
    Direction, EntityRef, JsonObservation, ObservationEvent, SemanticCommand, WorldState,
};

/// Engine chrome plus [`WorldState::entity_alias_map`] for slash targets.
#[derive(Debug, Clone)]
pub struct Chrome {
    entity_aliases: HashMap<String, String>,
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

    /// Short protocol hint for agents and first-time text clients.
    pub const WORLD_PROTOCOL: &'static str = "Skill: Xagora is a MUD-like open world over SSH. Read each observation, choose one Available command, send it, then observe the changed world state. Interactive TTY sessions can stay open. Non-TTY agents should either keep stdin open or run short command batches, then reconnect and continue from the saved player state. Wallet controls use /balance and /pay.";

    /// Legend for semantic ASCII markers.
    pub const MAP_LEGEND: &'static str =
        "Map: <Me> is you, [name] is a place/shopfront/room, {name} is an item/object.";

    /// Verb printed before a movement direction (for example "You go north").
    pub const MOVE_VERB: &'static str = "You go";

    /// Summary shown after the `/help` command.
    pub const HELP_SUMMARY: &'static str = "Commands: /look /go <dir> /inspect <target> /read <target> /take <target> /talk <target> /say <text> /history /mail <user> <text> /mailbox /broadcast <text> /news /balance /pay <user> <amount> [memo] /inventory /quit.";

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
        }
    }

    /// Builds chrome with a precomputed alias map (for example SSH after loading entity aliases).
    #[must_use]
    pub fn with_aliases(entity_aliases: HashMap<String, String>) -> Self {
        Self { entity_aliases }
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
            "inventory" | "inv" | "i" => Ok(SemanticCommand::Inventory),
            "help" | "h" => Ok(SemanticCommand::Help),
            "quit" | "exit" | "q" => Ok(SemanticCommand::Quit),
            "mailbox" | "inbox" => Ok(SemanticCommand::Mailbox),
            "history" => Ok(SemanticCommand::History),
            "news" => Ok(SemanticCommand::News),
            "balance" | "bal" => Ok(SemanticCommand::Balance),
            "go" | "move" => {
                let direction_token = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let direction =
                    parse_direction(direction_token).ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Move { direction })
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
            "pay" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let amount_text = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let amount = amount_text
                    .parse::<i64>()
                    .map_err(|_| SlashParseError::InvalidAmount)?;
                let memo = rest_after_token(trimmed, amount_text).unwrap_or_default();
                Ok(SemanticCommand::Pay {
                    target: target.to_owned(),
                    amount,
                    memo,
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
    /// Invalid integer amount.
    #[error("invalid amount")]
    InvalidAmount,
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
    output.push('\n');
    output.push_str(&observation.title);
    output.push('\n');
    if !observation.ascii_art.is_empty() {
        output.push('\n');
        for line in &observation.ascii_art {
            output.push_str(&highlight_ascii_markers(line));
            output.push('\n');
        }
    }
    output.push('\n');
    output.push_str(&observation.description);
    output.push('\n');
    output.push_str(Chrome::WORLD_PROTOCOL);
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

    if !observation.available_commands.is_empty() {
        output.push_str(&format!(
            "{}: {}\n",
            Chrome::LABEL_AVAILABLE,
            render_available_commands(&observation.available_commands)
        ));
    }

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

fn render_available_commands(commands: &[SemanticCommand]) -> String {
    let mut rendered = Vec::new();
    for command in commands {
        let text = render_command(command);
        if !rendered.contains(&text) {
            rendered.push(text);
        }
    }
    rendered.join(", ")
}

fn render_command(command: &SemanticCommand) -> String {
    match command {
        SemanticCommand::Look => "/look".to_owned(),
        SemanticCommand::Move { direction } => format!("/go {}", direction.as_str()),
        SemanticCommand::Inspect { target } => format!("/inspect {}", target.id),
        SemanticCommand::Read { target } => format!("/read {}", target.id),
        SemanticCommand::Take { target } => format!("/take {}", target.id),
        SemanticCommand::Talk { target } => format!("/talk {}", target.id),
        SemanticCommand::Say { .. } => "/say <text>".to_owned(),
        SemanticCommand::Mail { .. } => "/mail <user> <text>".to_owned(),
        SemanticCommand::Broadcast { .. } => "/broadcast <text>".to_owned(),
        SemanticCommand::Mailbox => "/mailbox".to_owned(),
        SemanticCommand::History => "/history".to_owned(),
        SemanticCommand::News => "/news".to_owned(),
        SemanticCommand::Balance => "/balance".to_owned(),
        SemanticCommand::Pay { .. } => "/pay <user> <amount> [memo]".to_owned(),
        SemanticCommand::Inventory => "/inventory".to_owned(),
        SemanticCommand::Help => "/help".to_owned(),
        SemanticCommand::Quit => "/quit".to_owned(),
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
        "[Tavern]",
        &styled_marker("[Tavern]", Chrome::ANSI_PLACE_MARKER),
    )
    .replace(
        "[Workshop]",
        &styled_marker("[Workshop]", Chrome::ANSI_PLACE_MARKER),
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
    use xagora_core::{JsonObservation, ObservationEvent};

    use super::{Chrome, render_text_observation};

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
            ascii_art: vec!["[Tavern] -- {bulletin board}".to_owned()],
            description: "A crossing.".to_owned(),
            exits: Vec::new(),
            entities: Vec::new(),
            available_commands: Vec::new(),
            events: Vec::new(),
        });

        assert!(rendered.contains(Chrome::ANSI_PLACE_MARKER));
        assert!(rendered.contains("[Tavern]"));
        assert!(rendered.contains(Chrome::ANSI_ITEM_MARKER));
        assert!(rendered.contains("{bulletin board}"));
    }
}
