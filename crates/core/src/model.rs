//! Core world model for views, exits, entities, and players.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::command::Direction;
use crate::ids::{EntityId, PlayerId, TextKey, ViewId};

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

/// A navigable location in the world graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    /// Stable canonical id.
    pub id: ViewId,
    /// Localized title key.
    pub title_key: TextKey,
    /// Localized description key.
    pub description_key: TextKey,
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
    /// Optional localized label for the exit destination.
    pub label_key: Option<TextKey>,
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
    /// Localized name key.
    pub name_key: TextKey,
    /// Localized description key.
    pub description_key: TextKey,
    /// Commands supported by this entity.
    pub actions: Vec<ActionKind>,
    /// Whether the entity can be carried.
    pub portable: bool,
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
