//! Core world model for views, exits, entities, and players.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::command::Direction;
use crate::ids::{EntityId, PlayerId, ViewId};

/// Default admission view id used when world metadata does not override it.
pub const DEFAULT_ADMISSION_VIEW_ID: &str = "arrival_street";
/// Default agreement board entity id used when world metadata does not override it.
pub const DEFAULT_ADMISSION_BOARD_ENTITY_ID: &str = "cyber_scroll_board";
/// Default agreement version used when world metadata does not override it.
pub const DEFAULT_AGREEMENT_VERSION: &str = "2026-06-03";
/// Default virtual day length in real-world seconds.
pub const DEFAULT_VIRTUAL_DAY_SECONDS: u64 = 300;
/// Standard quit feedback shown to players.
pub const FEEDBACK_QUIT: &str = "Goodbye.";
/// Parcel status indicating a vacant lot.
pub const PARCEL_STATUS_VACANT: &str = "vacant";
/// Parcel status indicating a claimed but unbuilt lot.
pub const PARCEL_STATUS_CLAIMED: &str = "claimed";
/// Parcel status indicating a built shop.
pub const PARCEL_STATUS_BUILT: &str = "built";
/// Admission state indicating the board has not been accepted yet.
pub const ADMISSION_STATE_PENDING: &str = "pending";
/// Admission state indicating the board has been accepted.
pub const ADMISSION_STATE_AGREED: &str = "agreed";
/// Inbox item status indicating a new unread item.
pub const INBOX_STATUS_UNREAD: &str = "unread";
/// Inbox item status indicating a claimed item.
pub const INBOX_STATUS_CLAIMED: &str = "claimed";
/// Inbox item status indicating an acknowledged item.
pub const INBOX_STATUS_ACKED: &str = "acked";
/// Inbox item status indicating an archived item.
pub const INBOX_STATUS_ARCHIVED: &str = "archived";
/// Inbox filter value indicating open or claimed work.
pub const INBOX_FILTER_OPEN: &str = "open";
/// Inbox filter value indicating only unread items.
pub const INBOX_FILTER_UNREAD: &str = "unread";
/// Inbox filter value indicating only claimed items.
pub const INBOX_FILTER_CLAIMED: &str = "claimed";
/// Inbox filter value indicating completed items.
pub const INBOX_FILTER_DONE: &str = "done";
/// Inbox filter value indicating all items.
pub const INBOX_FILTER_ALL: &str = "all";
/// Payment request status indicating a pending request.
pub const PAYMENT_REQUEST_STATUS_PENDING: &str = "pending";
/// Payment request status indicating a paid request.
pub const PAYMENT_REQUEST_STATUS_PAID: &str = "paid";
/// Payment request status indicating a cancelled request.
pub const PAYMENT_REQUEST_STATUS_CANCELLED: &str = "cancelled";
/// Operator command status indicating a newly queued command.
pub const OPERATOR_COMMAND_STATUS_PENDING: &str = "pending";
/// Operator command status indicating a delivered command.
pub const OPERATOR_COMMAND_STATUS_DELIVERED: &str = "delivered";
/// Operator command status indicating a handled command.
pub const OPERATOR_COMMAND_STATUS_HANDLED: &str = "handled";
/// Shop mailing list status indicating new subscriptions are accepted.
pub const SHOP_MAILING_LIST_STATUS_OPEN: &str = "open";
/// Shop mailing list status indicating new subscriptions are closed.
pub const SHOP_MAILING_LIST_STATUS_CLOSED: &str = "closed";
/// Shop mailing list subscription status indicating active delivery.
pub const SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE: &str = "active";
/// Shop mailing list subscription status indicating the player opted out.
pub const SHOP_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED: &str = "unsubscribed";
/// Shop work desk status indicating the desk accepts routed work.
pub const SHOP_WORK_DESK_STATUS_OPEN: &str = "open";
/// Shop work desk status indicating the desk is closed.
pub const SHOP_WORK_DESK_STATUS_CLOSED: &str = "closed";
/// Shop staff status indicating the worker may start shifts.
pub const SHOP_WORK_STAFF_ACTIVE: &str = "active";
/// Shop staff status indicating the worker was removed.
pub const SHOP_WORK_STAFF_REMOVED: &str = "removed";
/// Shop shift status indicating the worker is currently on site.
pub const SHOP_WORK_SHIFT_ACTIVE: &str = "active";
/// Shop shift status indicating the worker ended the shift.
pub const SHOP_WORK_SHIFT_ENDED: &str = "ended";
/// Shop work item status indicating the item is waiting for a worker.
pub const SHOP_WORK_ITEM_QUEUED: &str = "queued";
/// Shop work item status indicating the item is claimed by a worker.
pub const SHOP_WORK_ITEM_CLAIMED: &str = "claimed";
/// Shop work item status indicating the item is complete.
pub const SHOP_WORK_ITEM_DONE: &str = "done";
/// Shop work item status indicating the item was cancelled.
pub const SHOP_WORK_ITEM_CANCELLED: &str = "cancelled";
/// Shop badge award status indicating the badge is currently held.
pub const SHOP_BADGE_AWARD_ACTIVE: &str = "active";
/// Shop badge award status indicating the badge has been revoked.
pub const SHOP_BADGE_AWARD_REVOKED: &str = "revoked";

/// The mutable state of a world instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldState {
    /// Views keyed by stable id.
    pub views: HashMap<ViewId, View>,
    /// Entities keyed by stable id.
    pub entities: HashMap<EntityId, Entity>,
    /// Player states keyed by stable id.
    pub players: HashMap<PlayerId, PlayerState>,
}

/// Static world definition shared by all runtime sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldDefinition {
    /// Views keyed by stable id.
    pub views: HashMap<ViewId, View>,
    /// Entities keyed by stable id.
    pub entities: HashMap<EntityId, Entity>,
}

/// Protocol-neutral world metadata loaded from `meta.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorldMetadata {
    /// View where pending or rescued players should be anchored.
    #[serde(default = "default_admission_view_id")]
    pub admission_view_id: String,
    /// Entity id that exposes the admission agreement.
    #[serde(default = "default_admission_board_entity_id")]
    pub admission_board_entity_id: String,
    /// Current admission agreement version.
    #[serde(default = "default_agreement_version")]
    pub agreement_version: String,
    /// Whether the hunger survival gate participates in ordinary commands.
    #[serde(default)]
    pub hunger_loop_enabled: bool,
    /// Real-world seconds represented by one in-world day.
    #[serde(default = "default_virtual_day_seconds")]
    pub virtual_day_seconds: u64,
}

impl Default for WorldMetadata {
    fn default() -> Self {
        Self {
            admission_view_id: default_admission_view_id(),
            admission_board_entity_id: default_admission_board_entity_id(),
            agreement_version: default_agreement_version(),
            hunger_loop_enabled: false,
            virtual_day_seconds: default_virtual_day_seconds(),
        }
    }
}

fn default_admission_view_id() -> String {
    DEFAULT_ADMISSION_VIEW_ID.to_owned()
}

fn default_admission_board_entity_id() -> String {
    DEFAULT_ADMISSION_BOARD_ENTITY_ID.to_owned()
}

fn default_agreement_version() -> String {
    DEFAULT_AGREEMENT_VERSION.to_owned()
}

fn default_virtual_day_seconds() -> u64 {
    DEFAULT_VIRTUAL_DAY_SECONDS
}

/// Mutable runtime snapshot for player-specific state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    /// Player states keyed by stable id.
    pub players: HashMap<PlayerId, PlayerState>,
}

/// A navigable location in the world graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    /// Stable canonical id.
    pub id: ViewId,
    /// Player-facing title from world data.
    pub title: String,
    /// Player-facing description from world data.
    pub description: String,
    /// Optional ASCII map rendered above the description.
    #[serde(default)]
    pub ascii_art: Vec<String>,
    /// Directed exits from this view.
    pub exits: Vec<Exit>,
    /// Entities currently visible in this view.
    pub entities: Vec<EntityId>,
    /// Optional tile layout for future GUI renderers.
    pub layout: Option<ViewLayout>,
}

/// A directed edge from one view to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    /// Direction exposed to players and agents.
    pub direction: Direction,
    /// Target view id.
    pub target: ViewId,
    /// Optional player-facing label for the exit destination.
    #[serde(default)]
    pub label: Option<String>,
    /// Requirements that must be satisfied to use this exit.
    pub requirements: Vec<Requirement>,
}

/// Requirement placeholder for future gates, locks, and quests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Requirement {
    /// The exit is available without extra state.
    None,
}

/// Entity visible in a view or held by a player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    /// Stable canonical id.
    pub id: EntityId,
    /// Finite entity category.
    pub kind: EntityKind,
    /// Player-facing display name from world data.
    pub name: String,
    /// Player-facing description from world data.
    pub description: String,
    /// Slash-command tokens that resolve to this entity id (world-authored).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Commands supported by this entity.
    pub actions: Vec<ActionKind>,
    /// Optional structured payload (bulletin boards, shops, etc.).
    #[serde(default)]
    pub collection: Option<EntityCollection>,
    /// Whether the entity can be carried.
    #[serde(default)]
    pub portable: bool,
}

/// Structured entity payloads referenced by [`Entity::collection`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EntityCollection {
    /// Ordered bulletin entries on a board entity.
    BulletinBoard {
        /// Ordered bulletin entries.
        items: Vec<BulletinItem>,
    },
    /// Ordered dialogue lines offered by an NPC.
    Dialogue {
        /// Ordered dialogue lines.
        lines: Vec<DialogueLine>,
    },
}

/// Single bulletin entry shown through [`EntityCollection::BulletinBoard`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulletinItem {
    /// Stable bulletin id unique within its board.
    pub id: String,
    /// Player-facing title from world data.
    pub title: String,
    /// Player-facing body from world data.
    pub body: String,
}

/// Single dialogue line shown when talking to an NPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueLine {
    /// Stable dialogue id unique within its entity.
    pub id: String,
    /// Player-facing speaker name.
    pub speaker: String,
    /// Player-facing line body.
    pub body: String,
}

/// Finite entity categories used by rules and renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityKind {
    /// Non-player character.
    Npc,
    /// Carryable item.
    Item,
    /// Static scenery object.
    Object,
}

/// Canonical action kinds attached to entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// Inspect the entity.
    Inspect,
    /// Read readable text on the entity.
    Read,
    /// Take the entity.
    Take,
    /// Talk to the entity.
    Talk,
}

/// Player-specific mutable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Stable player id.
    pub id: PlayerId,
    /// Current view id.
    pub current_view: ViewId,
    /// Carried entity ids.
    pub inventory: Vec<EntityId>,
}

/// Optional structured layout for future tile rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewLayout {
    /// Tile width.
    pub width: u16,
    /// Tile height.
    pub height: u16,
    /// Row-major tile ids.
    pub tiles: Vec<String>,
    /// Row-major collision flags.
    pub collision: Vec<bool>,
    /// Entity placements for visual clients.
    pub placements: Vec<EntityPlacement>,
}

/// Location of an entity inside a tile layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPlacement {
    /// Entity id.
    pub entity_id: EntityId,
    /// Horizontal tile coordinate.
    pub x: u16,
    /// Vertical tile coordinate.
    pub y: u16,
}

impl WorldState {
    /// Builds a world state from static definition and mutable snapshot parts.
    #[must_use]
    pub fn from_parts(definition: WorldDefinition, snapshot: RuntimeSnapshot) -> Self {
        Self {
            views: definition.views,
            entities: definition.entities,
            players: snapshot.players,
        }
    }

    /// Returns the static world definition portion.
    #[must_use]
    pub fn definition(&self) -> WorldDefinition {
        WorldDefinition {
            views: self.views.clone(),
            entities: self.entities.clone(),
        }
    }

    /// Returns the mutable runtime snapshot portion.
    #[must_use]
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            players: self.players.clone(),
        }
    }

    /// Lowercased [`Entity::aliases`] entries mapped to canonical [`Entity::id`].
    #[must_use]
    pub fn entity_alias_map(&self) -> HashMap<String, EntityId> {
        let mut map = HashMap::new();
        for entity in self.entities.values() {
            for alias in &entity.aliases {
                map.insert(alias.to_ascii_lowercase(), entity.id.clone());
            }
        }
        map
    }

    /// Returns the requested player state.
    #[must_use]
    pub fn player(&self, player_id: &str) -> Option<&PlayerState> {
        self.players.get(player_id)
    }

    /// Returns the requested player state mutably.
    pub fn player_mut(&mut self, player_id: &str) -> Option<&mut PlayerState> {
        self.players.get_mut(player_id)
    }

    /// Returns a view by id.
    #[must_use]
    pub fn view(&self, view_id: &str) -> Option<&View> {
        self.views.get(view_id)
    }

    /// Returns an entity by id.
    #[must_use]
    pub fn entity(&self, entity_id: &str) -> Option<&Entity> {
        self.entities.get(entity_id)
    }
}
