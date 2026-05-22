//! Observations emitted by the runtime after commands.

use serde::{Deserialize, Serialize};

use crate::command::{Direction, SemanticCommand};
use crate::ids::{EntityId, ViewId};
use crate::model::{ActionKind, EntityKind};

/// Human-readable observation lines for plain-text clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextObservation {
    /// View title from world data.
    pub title: String,
    /// ASCII map lines shown above the description.
    #[serde(default)]
    pub ascii_art: Vec<String>,
    /// View description from world data.
    pub description: String,
    /// Exit directions as plain strings.
    pub exits: Vec<String>,
    /// Visible entity names from world data.
    pub entities: Vec<String>,
    /// Other online users visible in the current view.
    #[serde(default)]
    pub online_users: Vec<String>,
    /// Event lines produced by the last command.
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
    /// View title from world data.
    pub title: String,
    /// ASCII map lines rendered above the description.
    #[serde(default)]
    pub ascii_art: Vec<String>,
    /// View description from world data.
    pub description: String,
    /// Exits visible from the current view.
    pub exits: Vec<ExitObservation>,
    /// Entities visible in the current view.
    pub entities: Vec<EntityObservation>,
    /// Other online users visible in the current view.
    #[serde(default)]
    pub online_users: Vec<String>,
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
    /// Optional player-facing label from world data.
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
    /// Display name from world data.
    pub name: String,
    /// Description from world data.
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
        /// Message body shown to the player.
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
