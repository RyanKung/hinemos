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
    /// Command-line prompt shown before reading player input.
    pub const PROMPT: &'static str = "> ";

    /// Heading printed before the comma-separated exit directions.
    pub const LABEL_EXITS: &'static str = "Exits";

    /// Heading printed before visible entity names.
    pub const LABEL_VISIBLE: &'static str = "Visible";

    /// Verb printed before a movement direction (for example "You go north").
    pub const MOVE_VERB: &'static str = "You go";

    /// Summary shown after the `/help` command.
    pub const HELP_SUMMARY: &'static str = "Commands: /look /go <dir> /inspect <target> /read <target> /take <target> /talk <target> /inventory /quit.";

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

/// Slash parsing failures (shown to the player).
#[derive(Debug, Error, Eq, PartialEq, Clone)]
pub enum SlashParseError {
    /// Unknown command word.
    #[error("unknown command")]
    UnknownCommand,
    /// Missing argument where required.
    #[error("missing command argument")]
    MissingArgument,
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
            output.push_str(line);
            output.push('\n');
        }
    }
    output.push('\n');
    output.push_str(&observation.description);
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
