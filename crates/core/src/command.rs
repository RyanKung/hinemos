//! Semantic commands accepted by the world runtime.

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

/// Canonical commands produced by the slash parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SemanticCommand {
    /// Describe the current view again.
    Look,
    /// Show the current view map again.
    Map,
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
    /// Read visible text (poster, board, plaque).
    Read {
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
    /// Send a message to players in the same view.
    Say {
        /// Message body.
        text: String,
    },
    /// Send a direct message to a user or player id.
    Mail {
        /// User name or player id.
        target: String,
        /// Message body.
        text: String,
    },
    /// Send a global message to all connected players.
    Broadcast {
        /// Message body.
        text: String,
    },
    /// Show private persistent mail.
    Mailbox,
    /// Show current view message history.
    History,
    /// Show global broadcast news.
    News,
    /// Show other online users in the current view.
    Who,
    /// Show wallet balance.
    Balance,
    /// Wallet payment action.
    Pay {
        /// Payment action.
        action: PayAction,
    },
    /// Manage commercial street parcels.
    Land {
        /// Land command action.
        action: LandAction,
    },
    /// Edit the current owned parcel build sheet.
    Build {
        /// Build field update.
        action: BuildAction,
    },
    /// Manage an operated shop.
    Shop {
        /// Shop command action.
        action: ShopAction,
    },
    /// Run a registered extension command.
    Extension {
        /// Registered extension command name.
        name: String,
        /// Raw slash-prefixed input line.
        input: String,
    },
    /// Show carried items.
    Inventory,
    /// Show help text.
    Help,
    /// End the local CLI loop.
    Quit,
}

/// Land management actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LandAction {
    /// List all commercial parcels.
    List,
    /// Show one parcel.
    Info {
        /// Parcel id.
        parcel_id: String,
    },
    /// Claim a free parcel.
    Claim {
        /// Parcel id.
        parcel_id: String,
    },
    /// Transfer an owned parcel to another user or player id.
    Transfer {
        /// Parcel id.
        parcel_id: String,
        /// Target user or player id.
        target: String,
    },
}

/// Wallet payment actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PayAction {
    /// Transfer MARK directly to another player.
    Direct {
        /// User name or player id.
        target: String,
        /// Positive MARK amount.
        amount: i64,
        /// Optional transfer memo.
        memo: String,
    },
    /// List pending payment requests.
    Requests,
    /// Accept and pay a pending request.
    Accept {
        /// Payment request id.
        request_id: i64,
    },
}

/// Build sheet actions for an owned parcel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BuildAction {
    /// Show build help for the current parcel.
    Help,
    /// Apply a structured build sheet in one command.
    Apply {
        /// Structured build sheet supplied by a user or agent.
        sheet: BuildSheet,
    },
    /// Set a build field.
    Set {
        /// Field name: title, description, style, prompt, commands.
        field: String,
        /// Field value.
        value: String,
    },
    /// Publish the build sheet.
    Publish,
}

/// Structured shop build sheet supplied as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSheet {
    /// Shop title.
    pub title: Option<String>,
    /// Shop description.
    pub description: Option<String>,
    /// Presentation style note.
    pub style: Option<String>,
    /// Operator prompt shown to visitors and shop operators.
    pub prompt: Option<String>,
    /// Custom command help. If omitted, the server may generate defaults.
    pub commands: Option<String>,
}

/// Shop operation actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShopAction {
    /// Show custom commands sent to shops owned by this player.
    Inbox,
    /// Create a payment request for a visitor command.
    RequestPayment {
        /// Operator command id this request answers.
        command_id: i64,
        /// Positive MARK amount.
        amount: i64,
        /// Content delivered only after the visitor accepts and pays.
        delivery: String,
    },
}
