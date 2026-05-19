#![deny(missing_docs)]

//! Shared domain types for the Xagora open world.

pub mod command;
pub mod ids;
pub mod model;
pub mod observation;
pub mod sample_world;

pub use command::{Direction, EntityRef, SemanticCommand};
pub use ids::{EntityId, PlayerId, ViewId};
pub use model::{
    ActionKind, BulletinItem, DialogueLine, Entity, EntityCollection, EntityKind, Exit,
    PlayerState, Requirement, RuntimeSnapshot, View, ViewLayout, WorldDefinition, WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
