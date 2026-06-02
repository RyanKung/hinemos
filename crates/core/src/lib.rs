#![deny(missing_docs)]

//! Shared domain types for the Hinemos open world.

pub mod command;
pub mod ids;
pub mod model;
pub mod observation;
pub mod sample_world;

pub use command::{
    BuildAction, BuildSheet, Direction, EntityRef, InboxAction, LandAction, PayAction,
    SemanticCommand, SettingsAction, ShopAction,
};
pub use ids::{EntityId, PlayerId, ViewId};
pub use model::{
    ActionKind, BulletinItem, DialogueLine, Entity, EntityCollection, EntityKind, Exit,
    PlayerState, Requirement, RuntimeSnapshot, View, ViewLayout, WorldDefinition, WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
