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
    BadgeAction, BuildAction, BuildSheet, Direction, EntityRef, Gender, InboxAction, MbtiType,
    PARCEL_BADGE_DESCRIPTION_MAX_CHARS, PARCEL_BADGE_NOTE_MAX_CHARS, PARCEL_BADGE_SLUG_MAX_CHARS,
    PARCEL_BADGE_TITLE_MAX_CHARS, PARCEL_BADGES_PER_PARCEL_MAX,
    PARCEL_COMMAND_ROUTE_PREFIX_MAX_CHARS, PARCEL_JOB_GUIDE_BODY_MAX_CHARS,
    PARCEL_JOB_GUIDE_SLUG_MAX_CHARS, PARCEL_JOB_GUIDE_TITLE_MAX_CHARS,
    PARCEL_JOB_GUIDES_PER_PARCEL_MAX, PARCEL_MAILING_LIST_BODY_MAX_CHARS,
    PARCEL_MAILING_LIST_SLUG_MAX_CHARS, PARCEL_MAILING_LIST_SUBJECT_MAX_CHARS,
    PARCEL_MAILING_LIST_TITLE_MAX_CHARS, PARCEL_MAILING_LISTS_PER_PARCEL_MAX,
    PARCEL_WORK_DESK_SLUG_MAX_CHARS, PARCEL_WORK_DESK_TITLE_MAX_CHARS,
    PARCEL_WORK_DESKS_PER_PARCEL_MAX, PARCEL_WORK_RESULT_MAX_CHARS, ParcelAction,
    ParcelBadgeAction, ParcelDeskAction, ParcelJobAction, ParcelMailingListAction,
    ParcelRouteAction, ParcelShiftAction, ParcelStaffAction, ParcelWorkAction, PayAction,
    ROLE_CARD_INTRO_MAX_CHARS, ROLE_CARD_NAME_MAX_CHARS, SemanticCommand, SettingsAction,
    extension_command_input_matches_template, extension_commands,
    parcel_badge_description_is_valid, parcel_badge_note_is_valid, parcel_badge_slug_is_valid,
    parcel_badge_title_is_valid, parcel_command_route_prefix_is_valid,
    parcel_job_guide_body_is_valid, parcel_job_guide_slug_is_valid,
    parcel_job_guide_title_is_valid, parcel_mailing_list_body_is_valid,
    parcel_mailing_list_slug_is_valid, parcel_mailing_list_subject_is_valid,
    parcel_mailing_list_title_is_valid, parcel_work_desk_slug_is_valid,
    parcel_work_desk_title_is_valid, parcel_work_result_is_valid, role_card_intro_is_valid,
    role_card_name_is_valid,
};
pub use grid_map::{
    GRID_PARCEL_VIEW_PREFIX, GRID_ROAD_VIEW_PREFIX, GridOrigin, GridParcelAddress, GridRoad,
    generated_grid_label, generated_map_ascii_with_origin, generated_origin_view, grid_view,
    grid_view_with_origin, is_grid_view_id,
};
pub use ids::{EntityId, PlayerId, ViewId};
pub use model::{
    ADMISSION_STATE_AGREED, ADMISSION_STATE_PENDING, ActionKind, BulletinItem,
    DEFAULT_ADMISSION_BOARD_ENTITY_ID, DEFAULT_ADMISSION_VIEW_ID, DEFAULT_AGREEMENT_VERSION,
    DEFAULT_VIRTUAL_DAY_SECONDS, DialogueLine, Entity, EntityCollection, EntityKind, Exit,
    FEEDBACK_QUIT, INBOX_FILTER_ALL, INBOX_FILTER_CLAIMED, INBOX_FILTER_DONE, INBOX_FILTER_OPEN,
    INBOX_FILTER_UNREAD, INBOX_STATUS_ACKED, INBOX_STATUS_ARCHIVED, INBOX_STATUS_CLAIMED,
    INBOX_STATUS_UNREAD, OPERATOR_COMMAND_STATUS_DELIVERED, OPERATOR_COMMAND_STATUS_HANDLED,
    OPERATOR_COMMAND_STATUS_PENDING, PARCEL_BADGE_AWARD_ACTIVE, PARCEL_BADGE_AWARD_REVOKED,
    PARCEL_MAILING_LIST_STATUS_CLOSED, PARCEL_MAILING_LIST_STATUS_OPEN,
    PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE, PARCEL_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED,
    PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, PARCEL_STATUS_VACANT,
    PARCEL_WORK_DESK_STATUS_CLOSED, PARCEL_WORK_DESK_STATUS_OPEN, PARCEL_WORK_ITEM_CANCELLED,
    PARCEL_WORK_ITEM_CLAIMED, PARCEL_WORK_ITEM_DONE, PARCEL_WORK_ITEM_QUEUED,
    PARCEL_WORK_SHIFT_ACTIVE, PARCEL_WORK_SHIFT_ENDED, PARCEL_WORK_STAFF_ACTIVE,
    PARCEL_WORK_STAFF_REMOVED, PAYMENT_REQUEST_STATUS_CANCELLED, PAYMENT_REQUEST_STATUS_PAID,
    PAYMENT_REQUEST_STATUS_PENDING, PlayerState, Requirement, RuntimeSnapshot, View, ViewLayout,
    WorldDefinition, WorldMetadata, WorldState,
};
pub use observation::{
    EntityObservation, ExitObservation, JsonObservation, ObservationEvent, TextObservation,
};
