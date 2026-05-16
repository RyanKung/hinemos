//! Observations emitted by the runtime after commands.

use serde::{Deserialize, Serialize};

use crate::command::{Direction, SemanticCommand};
use crate::ids::{EntityId, ViewId};
use crate::model::{ActionKind, EntityKind};

/// Human-readable observation data before localization rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextObservation {
    /// Localized view title.
    pub title: String,
    /// Localized view description.
    pub description: String,
    /// Localized exit labels.
    pub exits: Vec<String>,
    /// Localized entity names.
    pub entities: Vec<String>,
    /// Localized event lines.
    pub events: Vec<String>,
}

/// Structured observation for agents and web clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonObservation {
    /// Player id associated with this observation.
    pub player_id: String,
    /// Current view id.
    pub view_id: ViewId,
    /// Localized view title for display.
    pub title: String,
    /// Localized view description for display.
    pub description: String,
    /// Exits visible from the current view.
    pub exits: Vec<ExitObservation>,
    /// Entities visible in the current view.
    pub entities: Vec<EntityObservation>,
    /// Canonical commands available to the player.
    pub available_commands: Vec<SemanticCommand>,
    /// Recent world events caused by the last command.
    pub events: Vec<ObservationEvent>,
}

/// Exit data visible in a structured observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitObservation {
    /// Direction of travel.
    pub direction: Direction,
    /// Target id when known by the client.
    pub target_known: bool,
    /// Optional localized label.
    pub label: Option<String>,
}

/// Entity data visible in a structured observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityObservation {
    /// Stable entity id.
    pub id: EntityId,
    /// Finite entity kind.
    pub kind: EntityKind,
    /// Localized entity name.
    pub name: String,
    /// Localized entity description.
    pub description: String,
    /// Supported action kinds.
    pub actions: Vec<ActionKind>,
}

/// Event included in observations and replay logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ObservationEvent {
    /// Informational message.
    Message {
        /// Localized event text.
        text: String,
    },
    /// Player moved between views.
    Move {
        /// Previous view id.
        from: ViewId,
        /// New view id.
        to: ViewId,
        /// Direction used.
        direction: Direction,
    },
}
