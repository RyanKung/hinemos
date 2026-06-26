#![deny(missing_docs)]

//! Shared domain types for the Hinemos open world.

pub mod command;
pub mod ids;
pub mod model;
pub mod observation;
pub mod sample_world;

pub use command::{
    BuildAction, BuildSheet, Direction, EntityRef, Gender, InboxAction, LandAction, MbtiType,
    PayAction, ROLE_CARD_INTRO_MAX_CHARS, ROLE_CARD_NAME_MAX_CHARS, SemanticCommand,
    SettingsAction, ShopAction, extension_commands, role_card_intro_is_valid,
    role_card_name_is_valid,
};
pub use ids::{EntityId, PlayerId, ViewId};
pub use model::{
    ADMISSION_STATE_AGREED, ADMISSION_STATE_PENDING, ActionKind, BulletinItem,
    DEFAULT_ADMISSION_BOARD_ENTITY_ID, DEFAULT_ADMISSION_VIEW_ID, DEFAULT_AGREEMENT_VERSION,
    DialogueLine, Entity, EntityCollection, EntityKind, Exit, FEEDBACK_QUIT, INBOX_FILTER_ALL,
    INBOX_FILTER_CLAIMED, INBOX_FILTER_DONE, INBOX_FILTER_OPEN, INBOX_FILTER_UNREAD,
    INBOX_STATUS_ACKED, INBOX_STATUS_ARCHIVED, INBOX_STATUS_CLAIMED, INBOX_STATUS_UNREAD,
    OPERATOR_COMMAND_STATUS_DELIVERED, OPERATOR_COMMAND_STATUS_HANDLED,
    OPERATOR_COMMAND_STATUS_PENDING, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED,
    PARCEL_STATUS_VACANT, PAYMENT_REQUEST_STATUS_CANCELLED, PAYMENT_REQUEST_STATUS_PAID,
    PAYMENT_REQUEST_STATUS_PENDING, PlayerState, Requirement, RuntimeSnapshot, View, ViewLayout,
    WorldDefinition, WorldMetadata, WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
