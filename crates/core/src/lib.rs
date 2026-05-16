#![deny(missing_docs)]

//! Shared domain types for the Agentopia MUD world.

pub mod command;
pub mod ids;
pub mod model;
pub mod observation;
pub mod sample_world;

pub use command::{Direction, EntityRef, SemanticCommand};
pub use ids::{EntityId, PlayerId, TextKey, ViewId};
pub use model::{
    ActionKind, Entity, EntityKind, Exit, PlayerState, Requirement, View, ViewLayout, WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
