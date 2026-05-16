//! Language-independent commands accepted by the world runtime.

use serde::{Deserialize, Serialize};

use crate::ids::EntityId;

/// Cardinal movement directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Move north.
    North,
    /// Move south.
    South,
    /// Move east.
    East,
    /// Move west.
    West,
    /// Move upward.
    Up,
    /// Move downward.
    Down,
}

impl Direction {
    /// Returns a stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Reference to an entity in a player command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRef {
    /// Stable entity id.
    pub id: EntityId,
}

impl EntityRef {
    /// Creates an entity reference from an id-like value.
    #[must_use]
    pub fn new(id: impl Into<EntityId>) -> Self {
        Self { id: id.into() }
    }
}

/// Canonical commands after language-specific parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SemanticCommand {
    /// Describe the current view again.
    Look,
    /// Move through an exit.
    Move {
        /// Direction to move.
        direction: Direction,
    },
    /// Inspect a visible entity.
    Inspect {
        /// Target entity.
        target: EntityRef,
    },
    /// Pick up a visible item.
    Take {
        /// Target entity.
        target: EntityRef,
    },
    /// Talk to a visible NPC.
    Talk {
        /// Target entity.
        target: EntityRef,
    },
    /// Show carried items.
    Inventory,
    /// Show help text.
    Help,
    /// End the local CLI loop.
    Quit,
}
