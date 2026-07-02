#![deny(missing_docs)]

//! Shared domain types for the Hinemos open world.

pub mod agent_task;
mod agent_task_match;
pub mod command;
pub mod grid_map;
pub mod ids;
pub mod model;
pub mod observation;
pub mod sample_world;

pub use agent_task::{
    HungerPolicy, HungerSignal, ObservedTaskState, RewardSpec, TaskCommand, TaskCommandError,
    TaskCommandRecord, TaskConstraints, TaskMode, TaskModeError, TaskSnapshot, TaskStepEvaluation,
};
pub use command::{
    BadgeAction, BuildAction, BuildSheet, Direction, EntityRef, Gender, InboxAction, LandAction,
    MbtiType, PayAction, ROLE_CARD_INTRO_MAX_CHARS, ROLE_CARD_NAME_MAX_CHARS,
    SHOP_BADGE_DESCRIPTION_MAX_CHARS, SHOP_BADGE_NOTE_MAX_CHARS, SHOP_BADGE_SLUG_MAX_CHARS,
    SHOP_BADGE_TITLE_MAX_CHARS, SHOP_BADGES_PER_PARCEL_MAX, SHOP_MAILING_LIST_BODY_MAX_CHARS,
    SHOP_MAILING_LIST_SLUG_MAX_CHARS, SHOP_MAILING_LIST_SUBJECT_MAX_CHARS,
    SHOP_MAILING_LIST_TITLE_MAX_CHARS, SHOP_MAILING_LISTS_PER_PARCEL_MAX, SemanticCommand,
    SettingsAction, ShopAction, ShopBadgeAction, ShopMailingListAction, SubscriptionAction,
    extension_command_input_matches_template, extension_commands, role_card_intro_is_valid,
    role_card_name_is_valid, shop_badge_description_is_valid, shop_badge_note_is_valid,
    shop_badge_slug_is_valid, shop_badge_title_is_valid, shop_mailing_list_body_is_valid,
    shop_mailing_list_slug_is_valid, shop_mailing_list_subject_is_valid,
    shop_mailing_list_title_is_valid,
};
pub use grid_map::{
    GRID_PARCEL_VIEW_PREFIX, GRID_ROAD_VIEW_PREFIX, GridParcelAddress, GridRoad, grid_view,
    is_grid_view_id,
};
pub use ids::{EntityId, PlayerId, ViewId};
pub use model::{
    ADMISSION_STATE_AGREED, ADMISSION_STATE_PENDING, ActionKind, BulletinItem,
    DEFAULT_ADMISSION_BOARD_ENTITY_ID, DEFAULT_ADMISSION_VIEW_ID, DEFAULT_AGREEMENT_VERSION,
    DEFAULT_VIRTUAL_DAY_SECONDS, DialogueLine, Entity, EntityCollection, EntityKind, Exit,
    FEEDBACK_QUIT, INBOX_FILTER_ALL, INBOX_FILTER_CLAIMED, INBOX_FILTER_DONE, INBOX_FILTER_OPEN,
    INBOX_FILTER_UNREAD, INBOX_STATUS_ACKED, INBOX_STATUS_ARCHIVED, INBOX_STATUS_CLAIMED,
    INBOX_STATUS_UNREAD, OPERATOR_COMMAND_STATUS_DELIVERED, OPERATOR_COMMAND_STATUS_HANDLED,
    OPERATOR_COMMAND_STATUS_PENDING, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED,
    PARCEL_STATUS_VACANT, PAYMENT_REQUEST_STATUS_CANCELLED, PAYMENT_REQUEST_STATUS_PAID,
    PAYMENT_REQUEST_STATUS_PENDING, PlayerState, Requirement, RuntimeSnapshot,
    SHOP_BADGE_AWARD_ACTIVE, SHOP_BADGE_AWARD_REVOKED, SHOP_MAILING_LIST_STATUS_CLOSED,
    SHOP_MAILING_LIST_STATUS_OPEN, SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE,
    SHOP_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED, View, ViewLayout, WorldDefinition, WorldMetadata,
    WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
